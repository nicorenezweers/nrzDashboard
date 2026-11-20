//! App Store of the home screen (umbrelOS App Store through MCP).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use serde_json::json;

#[cfg(feature = "server")]
use super::{err, value_to_text};
#[cfg(feature = "server")]
use crate::server::mcp;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct StoreApp {
    pub id: String,
    pub name: String,
    pub tagline: String,
    pub version: String,
    pub category: String,
    pub compatible: bool,
    pub dependencies: Vec<String>,
    pub installed: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct StoreDetails {
    pub id: String,
    pub name: String,
    pub version: String,
    pub tagline: String,
    pub description: String,
    pub category: String,
    pub compatible: bool,
    pub dependencies: Vec<String>,
    pub release_notes: String,
    pub installed: bool,
    /// Install size in bytes, when umbrelOS reports one.
    pub install_size: Option<f64>,
}

#[server]
pub async fn store_search(query: String, limit: u32) -> Result<Vec<StoreApp>, ServerFnError> {
    mcp::call_as(
        "search_app_store",
        json!({ "query": query.trim(), "limit": limit.clamp(1, 50) }),
    )
    .await
    .map_err(err)
}

#[server]
pub async fn store_details(app_id: String) -> Result<StoreDetails, ServerFnError> {
    mcp::call_as("get_app_store_details", json!({ "appId": app_id }))
        .await
        .map_err(err)
}

/// Installs an App Store app (dependencies must already be installed).
#[server]
pub async fn store_install(app_id: String) -> Result<String, ServerFnError> {
    let v = mcp::call("install_app", json!({ "appId": app_id }))
        .await
        .map_err(err)?;
    Ok(value_to_text(&v))
}

/// Uninstalls an app – deletes its data for good.
#[server]
pub async fn store_uninstall(app_id: String) -> Result<String, ServerFnError> {
    let v = mcp::call("uninstall_app", json!({ "appId": app_id }))
        .await
        .map_err(err)?;
    Ok(value_to_text(&v))
}

/// Updates every installed app with an update; returns the ids that were started.
#[server]
pub async fn update_all_apps() -> Result<Vec<String>, ServerFnError> {
    let apps: Vec<crate::model::AppInfo> =
        mcp::call_as("list_apps", json!({})).await.map_err(err)?;
    let mut started = Vec::new();
    for a in apps.into_iter().filter(|a| a.update_available) {
        if mcp::call("update_app", json!({ "appId": a.id }))
            .await
            .is_ok()
        {
            started.push(a.name);
        }
    }
    Ok(started)
}

/// Docker containers, file systems, reachability, sensors, alerts and events from nrzMonitor.
#[server]
pub async fn monitor_view() -> Result<crate::model::MonitorView, ServerFnError> {
    crate::server::monitor::view().await.map_err(err)
}
