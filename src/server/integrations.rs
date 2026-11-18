//! Integration settings (`$DATA_DIR/integrations.json`, 0600) and MCP calls to nrzDNS / nrzBudget.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::integrations::{Integration, IntegrationInfo};

use super::{config, env, mcp};

#[derive(Clone, Default, Serialize, Deserialize)]
struct Entry {
    url: String,
    token: String,
}

fn file() -> std::path::PathBuf {
    config().data_dir.join("integrations.json")
}

fn env_defaults(kind: Integration) -> Entry {
    let (u, t) = match kind {
        Integration::Dns => ("NRZDNS_MCP_URL", "NRZDNS_MCP_TOKEN"),
        Integration::Budget => ("NRZBUDGET_MCP_URL", "NRZBUDGET_MCP_TOKEN"),
    };
    Entry {
        url: env(u).unwrap_or_else(|| kind.default_url().to_string()),
        token: env(t).unwrap_or_default(),
    }
}

fn store() -> &'static RwLock<HashMap<Integration, Entry>> {
    static S: OnceLock<RwLock<HashMap<Integration, Entry>>> = OnceLock::new();
    S.get_or_init(|| {
        let saved: HashMap<Integration, Entry> = std::fs::read_to_string(file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let map = Integration::ALL
            .into_iter()
            .map(|k| {
                let d = env_defaults(k);
                let s = saved.get(&k).cloned().unwrap_or_default();
                let e = Entry {
                    url: if s.url.is_empty() { d.url } else { s.url },
                    token: if s.token.is_empty() { d.token } else { s.token },
                };
                (k, e)
            })
            .collect();
        RwLock::new(map)
    })
}

fn get(kind: Integration) -> Entry {
    store().read().ok().and_then(|m| m.get(&kind).cloned()).unwrap_or_else(|| env_defaults(kind))
}

fn mask(token: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    let chars: Vec<char> = token.chars().collect();
    Some(if chars.len() > 16 {
        let head: String = chars.iter().take(10).collect();
        let tail: String = chars.iter().skip(chars.len() - 4).collect();
        format!("{head}…{tail}")
    } else {
        "••••••••".into()
    })
}

pub fn infos() -> Vec<IntegrationInfo> {
    Integration::ALL
        .into_iter()
        .map(|k| {
            let e = get(k);
            IntegrationInfo { kind: k, url: e.url, token_hint: mask(&e.token) }
        })
        .collect()
}

pub fn save(kind: Integration, url: &str, token: &str) -> Result<(), String> {
    let url = url.trim().trim_end_matches('/').to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("URL muss mit http:// oder https:// beginnen".into());
    }
    let mut map = store().write().map_err(|_| "Interner Fehler".to_string())?;
    let e = map.entry(kind).or_insert_with(|| env_defaults(kind));
    e.url = url;
    if !token.trim().is_empty() {
        e.token = token.trim().to_string();
    }
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::to_string_pretty(&*map).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("Konnte {} nicht schreiben: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub async fn call(kind: Integration, tool: &str, args: Value) -> Result<Value, String> {
    let e = get(kind);
    if e.token.is_empty() {
        return Err(format!("Kein {}-Token hinterlegt – unter System → Integrationen eintragen.", kind.label()));
    }
    mcp::call_at(kind.label(), &e.url, &e.token, tool, args).await
}

pub async fn call_as<T: serde::de::DeserializeOwned>(kind: Integration, tool: &str, args: Value) -> Result<T, String> {
    let v = call(kind, tool, args).await?;
    serde_json::from_value(v).map_err(|e| format!("Unerwartete Antwort von {}: {e}", kind.label()))
}

pub async fn test(kind: Integration) -> Result<String, String> {
    match kind {
        Integration::Dns => {
            let v = call(kind, "get_server_status", json!({})).await?;
            let version = v.get("version").and_then(Value::as_str).unwrap_or("?");
            Ok(format!("Verbunden mit nrzDNS {version}"))
        }
        Integration::Budget => {
            let v = call(kind, "get_status", json!({})).await?;
            let version = v.get("version").and_then(Value::as_str).unwrap_or("?");
            let user = v.get("user").and_then(|u| u.get("name").or(Some(u))).and_then(Value::as_str).unwrap_or("");
            Ok(if user.is_empty() { format!("Verbunden mit nrzBudget {version}") } else { format!("Verbunden mit nrzBudget {version} als {user}") })
        }
    }
}
