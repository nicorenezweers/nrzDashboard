//! Server functions called by the UI. All of them sit behind the auth middleware (`/api/*`).

pub mod agent;
pub mod board;
pub mod files;
pub mod home;
pub mod integrations;
pub mod monitor;
pub mod news;

use dioxus::prelude::*;

#[cfg(feature = "server")]
use serde_json::{Value, json};

use crate::model::{AppDetails, AppInfo, Overview};

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
    let (info, res, apps) = tokio::join!(
        mcp::call_as("get_system_info", json!({})),
        mcp::call_as("get_system_resources", json!({})),
        mcp::call_as::<Vec<AppInfo>>("list_apps", json!({})),
    );
    let error = [info.as_ref().err(), res.as_ref().err(), apps.as_ref().err()]
        .into_iter()
        .flatten()
        .next()
        .cloned();
    Ok(Overview { info: info.ok(), resources: res.ok(), apps: apps.unwrap_or_default(), error })
}

// ---------- Apps ----------

#[server]
pub async fn list_apps() -> Result<(Vec<AppInfo>, Option<crate::model::Resources>), ServerFnError> {
    let (apps, res) = tokio::join!(
        mcp::call_as::<Vec<AppInfo>>("list_apps", json!({})),
        mcp::call_as("get_system_resources", json!({})),
    );
    let mut apps = apps.map_err(err)?;
    apps.sort_by_key(|a| a.name.to_lowercase());
    Ok((apps, res.ok()))
}

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
    Ok(value_to_text(&v))
}

#[server]
pub async fn app_logs(app_id: String, lines: u32) -> Result<String, ServerFnError> {
    let v = mcp::call("get_app_logs", json!({ "appId": app_id, "lines": lines.clamp(1, 500) }))
        .await
        .map_err(err)?;
    Ok(logs_text(&v))
}

#[server]
pub async fn app_details(app_id: String) -> Result<AppDetails, ServerFnError> {
    let v = mcp::call("get_app_details", json!({ "appId": app_id })).await.map_err(err)?;
    let s = |p: &str| v.pointer(p).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    let f = |p: &str| v.pointer(p).and_then(Value::as_f64).unwrap_or(0.0);
    Ok(AppDetails {
        url: s("/url"),
        username: s("/credentials/username"),
        password: s("/credentials/password"),
        cpu: f("/usage/cpu"),
        memory: f("/usage/memory"),
        disk: f("/usage/disk"),
        tagline: s("/tagline").unwrap_or_default(),
    })
}

// ---------- System / MCP ----------

#[server]
pub async fn umbrel_tool_text(name: String, args: String) -> Result<String, ServerFnError> {
    // Read-only helpers used by the system page.
    const READ_ONLY: &[&str] = &["check_os_update", "get_network_settings", "get_system_logs", "get_os_update_status", "get_notifications"];
    if !READ_ONLY.contains(&name.as_str()) {
        return Err(err("Nicht erlaubt"));
    }
    let args: Value = serde_json::from_str(&args).unwrap_or_else(|_| json!({}));
    let v = mcp::call(&name, args).await.map_err(err)?;
    Ok(if name == "get_system_logs" { logs_text(&v) } else { value_to_text(&v) })
}

/// Restarts the umbrelOS device.
#[server]
pub async fn restart_device() -> Result<String, ServerFnError> {
    let v = mcp::call("restart_device", json!({})).await.map_err(err)?;
    Ok(value_to_text(&v))
}

#[cfg(feature = "server")]
pub(crate) fn value_to_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

#[cfg(feature = "server")]
fn logs_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(lines) => lines.iter().map(value_to_text).collect::<Vec<_>>().join("\n"),
        Value::Object(o) => o
            .get("logs")
            .or_else(|| o.get("lines"))
            .map(logs_text)
            .unwrap_or_else(|| value_to_text(v)),
        other => value_to_text(other),
    }
}

// ---------- Settings ----------

#[server]
pub async fn get_umbrel_settings() -> Result<crate::model::UmbrelSettings, ServerFnError> {
    let u = crate::server::settings::umbrel();
    Ok(crate::model::UmbrelSettings { token_hint: mask(&u.token), url: u.url })
}

/// Saves URL/token (empty token keeps the current one) and tests the connection.
#[server]
pub async fn save_umbrel_settings(url: String, token: String) -> Result<String, ServerFnError> {
    crate::server::settings::save(&url, &token).map_err(err)?;
    let info: crate::model::SystemInfo = mcp::call_as("get_system_info", json!({})).await.map_err(err)?;
    Ok(format!("Verbunden mit {} ({})", info.hostname, info.version_name))
}
