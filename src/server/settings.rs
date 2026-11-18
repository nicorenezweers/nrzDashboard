//! Connection settings: environment as default, overridable in the UI – umbrelOS MCP
//! (`$DATA_DIR/settings.json`) and nrzNews MCP (`$DATA_DIR/news.json`).

use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use super::{config, env, tracing_warn};

pub const NO_TOKEN: &str = "Kein Umbrel-MCP-Token hinterlegt – unter System → Umbrel-Verbindung eintragen.";
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
            tracing_warn("Kein Umbrel-MCP-Token – in der Oberfläche unter System eintragen oder UMBREL_MCP_TOKEN setzen.");
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
    std::fs::write(&path, json).map_err(|e| format!("Konnte {} nicht schreiben: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

// ---------- nrzNews ----------

pub const NO_NEWS_TOKEN: &str = "Kein nrzNews-Token hinterlegt – in nrzNews unter Einstellungen → Konto → API-Tokens anlegen und unter System → nrzNews eintragen.";
/// nrzNews on the same Umbrel (app port 8340, reached from the container via the host).
const DEFAULT_NEWS_URL: &str = "http://host.docker.internal:8340";

fn news_file() -> std::path::PathBuf {
    config().data_dir.join("news.json")
}

fn news_store() -> &'static RwLock<Umbrel> {
    static S: OnceLock<RwLock<Umbrel>> = OnceLock::new();
    S.get_or_init(|| {
        let saved: Umbrel = std::fs::read_to_string(news_file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let url = Some(saved.url)
            .filter(|u| !u.is_empty())
            .or_else(|| env("NRZNEWS_URL"))
            .unwrap_or_else(|| DEFAULT_NEWS_URL.into());
        let token = Some(saved.token).filter(|t| !t.is_empty()).or_else(|| env("NRZNEWS_TOKEN")).unwrap_or_default();
        RwLock::new(Umbrel { url: normalize_url(&url), token: token.trim().to_string() })
    })
}

/// nrzNews connection (same shape as the Umbrel one: base URL + token).
pub fn news() -> Umbrel {
    news_store().read().map(|u| u.clone()).unwrap_or_default()
}

/// New nrzNews connection from the form, not saved yet: an empty `token` keeps the current
/// one, `clear` removes it.
pub fn news_candidate(url: &str, token: &str, clear: bool) -> Result<Umbrel, String> {
    let url = normalize_url(url);
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("URL muss mit http:// oder https:// beginnen".into());
    }
    let token = token.trim();
    if !token.is_empty() && !token.starts_with("nrznews_") {
        return Err("Das ist kein nrzNews-Token (beginnt mit nrznews_).".into());
    }
    let token = if clear {
        String::new()
    } else if token.is_empty() {
        news().token
    } else {
        token.to_string()
    };
    Ok(Umbrel { url, token })
}

/// Saves the nrzNews connection (after a successful test).
pub fn save_news(value: Umbrel) -> Result<(), String> {
    let mut cur = news_store().write().map_err(|_| "Interner Fehler".to_string())?;
    *cur = value;
    let path = news_file();
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
