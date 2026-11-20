//! Dashboard settings (`$DATA_DIR/dashboard.json`, 0600): greeting, app links (user/admin),
//! nrzNetwork connection, login (password, 2FA) and hidden apps. Plus TOTP (RFC 6238).

use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{config, env};

const DEFAULT_NETWORK_URL: &str = "http://nrz-dns_app_1:8380/mcp";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Dash {
    /// Name in the greeting.
    pub name: String,
    /// `user` (bench.zweers.ch) or `admin` (admin.bench.zweers.ch).
    pub link_mode: String,
    /// MCP endpoint of nrzNetwork (reverse proxy entries = app domains).
    pub network_url: String,
    pub network_token: String,
    /// nrzMonitor address (empty = internal default, `-` = off) and optional key.
    pub monitor_url: String,
    pub monitor_token: String,
    /// argon2 hash; empty = password from the environment.
    pub password_hash: String,
    /// Base32 TOTP secret; only used when `totp_enabled`.
    pub totp_secret: String,
    pub totp_enabled: bool,
    pub hidden: Vec<String>,
    /// Own name / public link / admin link per app (empty field = automatic).
    pub links: std::collections::HashMap<String, crate::api::apps::AppLinks>,
}

impl Default for Dash {
    fn default() -> Self {
        Self {
            name: "nrz".into(),
            link_mode: "user".into(),
            network_url: String::new(),
            network_token: String::new(),
            monitor_url: String::new(),
            monitor_token: String::new(),
            password_hash: String::new(),
            totp_secret: String::new(),
            totp_enabled: false,
            hidden: Vec::new(),
            links: Default::default(),
        }
    }
}

fn file() -> std::path::PathBuf {
    config().data_dir.join("dashboard.json")
}

fn read_json(name: &str) -> Option<Value> {
    std::fs::read_to_string(config().data_dir.join(name)).ok().and_then(|s| serde_json::from_str(&s).ok())
}

/// First start after the update: take nrzDNS connection and hidden apps from the old files.
fn migrate(d: &mut Dash) {
    if d.network_url.is_empty() || d.network_token.is_empty() {
        let old = read_json("integrations.json").and_then(|v| v.get("dns").cloned());
        let s = |k: &str| old.as_ref().and_then(|o| o.get(k)).and_then(Value::as_str).unwrap_or_default().to_string();
        if d.network_url.is_empty() {
            d.network_url = Some(s("url"))
                .filter(|u| !u.is_empty())
                .or_else(|| env("NRZDNS_MCP_URL"))
                .unwrap_or_else(|| DEFAULT_NETWORK_URL.into());
        }
        if d.network_token.is_empty() {
            d.network_token = Some(s("token")).filter(|t| !t.is_empty()).or_else(|| env("NRZDNS_MCP_TOKEN")).unwrap_or_default();
        }
    }
    if d.hidden.is_empty() {
        if let Some(Value::Object(o)) = read_json("app_overrides.json") {
            d.hidden = o
                .iter()
                .filter(|(_, v)| v.get("hidden").and_then(Value::as_bool).unwrap_or(false))
                .map(|(k, _)| k.clone())
                .collect();
        }
    }
}

fn store() -> &'static RwLock<Dash> {
    static S: OnceLock<RwLock<Dash>> = OnceLock::new();
    S.get_or_init(|| {
        let mut d: Dash = std::fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        migrate(&mut d);
        RwLock::new(d)
    })
}

pub fn get() -> Dash {
    store().read().map(|d| d.clone()).unwrap_or_default()
}

/// Changes and saves the settings.
pub fn update(f: impl FnOnce(&mut Dash)) -> Result<(), String> {
    let mut cur = store().write().map_err(|_| "Interner Fehler".to_string())?;
    f(&mut cur);
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::to_string_pretty(&*cur).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("Konnte {} nicht schreiben: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

// ---------- TOTP ----------

const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

fn b32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for &b in data {
        buf = (buf << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(B32[((buf >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(B32[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn b32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for c in s.chars().filter(|c| !c.is_whitespace() && *c != '=') {
        let v = B32.iter().position(|&x| x as char == c.to_ascii_uppercase())? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// New random secret (160 bit, Base32).
pub fn new_secret() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 20];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    b32_encode(&buf)
}

fn code_at(secret: &[u8], step: u64) -> u32 {
    use hmac::{Hmac, Mac};
    let Ok(mut mac) = Hmac::<sha1::Sha1>::new_from_slice(secret) else { return u32::MAX };
    mac.update(&step.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let off = (h[19] & 0x0f) as usize;
    let bin = (u32::from(h[off]) & 0x7f) << 24 | u32::from(h[off + 1]) << 16 | u32::from(h[off + 2]) << 8 | u32::from(h[off + 3]);
    bin % 1_000_000
}

/// Checks a 6-digit code (±30 s); a code can only be used once.
pub fn totp_check(secret_b32: &str, code: &str) -> bool {
    static LAST: OnceLock<std::sync::Mutex<u64>> = OnceLock::new();
    let code: String = code.chars().filter(char::is_ascii_digit).collect();
    let (Some(secret), Ok(n)) = (b32_decode(secret_b32), code.parse::<u32>()) else { return false };
    if code.len() != 6 || secret.is_empty() {
        return false;
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) / 30;
    let Some(step) = [now.saturating_sub(1), now, now + 1].into_iter().find(|s| code_at(&secret, *s) == n) else { return false };
    let mut last = LAST.get_or_init(|| std::sync::Mutex::new(0)).lock().unwrap_or_else(|e| e.into_inner());
    if step <= *last {
        return false;
    }
    *last = step;
    true
}

/// `otpauth://` link for authenticator apps.
pub fn otpauth_uri(secret: &str, account: &str) -> String {
    format!(
        "otpauth://totp/nrzDashboard:{}?secret={secret}&issuer=nrzDashboard&digits=6&period=30",
        urlencoding::encode(account)
    )
}

/// QR code as SVG.
pub fn qr_svg(data: &str) -> String {
    qrcode::QrCode::new(data.as_bytes())
        .map(|c| {
            c.render::<qrcode::render::svg::Color>()
                .min_dimensions(200, 200)
                .dark_color(qrcode::render::svg::Color("#000000"))
                .light_color(qrcode::render::svg::Color("#ffffff"))
                .build()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_vector() {
        // RFC 6238, SHA1, T = 59 s → 94287082 (8 digits) → 287082
        assert_eq!(code_at(b"12345678901234567890", 59 / 30), 287_082);
    }

    #[test]
    fn base32_roundtrip() {
        let s = b32_encode(b"hello world!");
        assert_eq!(b32_decode(&s).unwrap(), b"hello world!");
    }
}
