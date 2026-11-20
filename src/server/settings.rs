//! umbrelOS connection: environment as default, overridable in the UI (`$DATA_DIR/settings.json`).

use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use super::{config, env, tracing_warn};

pub const NO_TOKEN: &str =
    "Kein Umbrel-MCP-Token hinterlegt – unter Einstellungen → Umbrel eintragen.";
const DEFAULT_URL: &str = "http://umbrel.local";

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Umbrel {
    pub url: String,
    pub token: String,
}

fn file() -> std::path::PathBuf {
    config().data_dir.join("settings.json")
}

fn normalize_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn store() -> &'static RwLock<Umbrel> {
    static S: OnceLock<RwLock<Umbrel>> = OnceLock::new();
    S.get_or_init(|| {
        let saved: Umbrel = std::fs::read_to_string(file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let url = Some(saved.url)
            .filter(|u| !u.is_empty())
            .or_else(|| env("UMBREL_URL"))
            .unwrap_or_else(|| DEFAULT_URL.into());
        let token = Some(saved.token).filter(|t| !t.is_empty()).or_else(|| env("UMBREL_MCP_TOKEN")).unwrap_or_default();
        if token.is_empty() {
            tracing_warn("Kein Umbrel-MCP-Token – in der Oberfläche unter Einstellungen eintragen oder UMBREL_MCP_TOKEN setzen.");
        }
        RwLock::new(Umbrel { url: normalize_url(&url), token: token.trim().to_string() })
    })
}

pub fn umbrel() -> Umbrel {
    store().read().map(|u| u.clone()).unwrap_or_default()
}

/// Saves new values; an empty `token` keeps the current one.
pub fn save(url: &str, token: &str) -> Result<(), String> {
    let url = normalize_url(url);
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("URL muss mit http:// oder https:// beginnen".into());
    }
    let mut cur = store().write().map_err(|_| "Interner Fehler".to_string())?;
    cur.url = url;
    if !token.trim().is_empty() {
        cur.token = token.trim().to_string();
    }
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::to_string_pretty(&*cur).map_err(|e| e.to_string())?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Konnte {} nicht schreiben: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}
