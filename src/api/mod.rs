//! Server functions called by the UI. All of them sit behind the auth middleware (`/api/*`).

pub mod apps;
pub mod files;
pub mod home;
pub mod settings;

use dioxus::prelude::*;

#[cfg(feature = "server")]
use serde_json::{Value, json};

use crate::model::Overview;

#[cfg(feature = "server")]
use crate::server::mcp;

#[cfg(feature = "server")]
pub(crate) fn err(e: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::new(e.to_string())
}

/// Token for display: `umbrelmcp_54a6…18e1`, `None` when empty.
#[cfg(feature = "server")]
pub(crate) fn mask(token: &str) -> Option<String> {
    (!token.is_empty()).then(|| {
        let chars: Vec<char> = token.chars().collect();
        if chars.len() > 20 {
            let head: String = chars.iter().take(14).collect();
            let tail: String = chars.iter().skip(chars.len() - 4).collect();
            format!("{head}…{tail}")
        } else {
            "••••••••".to_string()
        }
    })
}

// ---------- Overview ----------

#[server]
pub async fn get_overview() -> Result<Overview, ServerFnError> {
    Ok(crate::server::cache::overview().await)
}

// ---------- Apps ----------

#[server]
pub async fn app_action(app_id: String, action: String) -> Result<String, ServerFnError> {
    let tool = match action.as_str() {
        "start" => "start_app",
        "stop" => "stop_app",
        "restart" => "restart_app",
        "update" => "update_app",
        _ => return Err(err("Unbekannte Aktion")),
    };
    let v = mcp::call(tool, json!({ "appId": app_id })).await.map_err(err)?;
    crate::server::cache::kick();
    Ok(value_to_text(&v))
}

#[cfg(feature = "server")]
pub(crate) fn value_to_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}
