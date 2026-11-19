//! Home screen (umbrelOS style): App Store, activity and the generic umbrelOS MCP tool console.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use serde_json::{Value, json};

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

/// One tool of the umbrelOS MCP server.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    /// JSON schema of the arguments (pretty printed).
    pub schema: String,
    /// Example arguments built from the schema (required fields with defaults/placeholders).
    pub example: String,
    pub destructive: bool,
}

#[server]
pub async fn store_search(query: String, limit: u32) -> Result<Vec<StoreApp>, ServerFnError> {
    mcp::call_as("search_app_store", json!({ "query": query.trim(), "limit": limit.clamp(1, 50) }))
        .await
        .map_err(err)
}

#[server]
pub async fn store_details(app_id: String) -> Result<StoreDetails, ServerFnError> {
    mcp::call_as("get_app_store_details", json!({ "appId": app_id })).await.map_err(err)
}

/// Installs an App Store app (dependencies must already be installed).
#[server]
pub async fn store_install(app_id: String) -> Result<String, ServerFnError> {
    let v = mcp::call("install_app", json!({ "appId": app_id })).await.map_err(err)?;
    Ok(value_to_text(&v))
}

/// Uninstalls an app – deletes its data for good.
#[server]
pub async fn store_uninstall(app_id: String) -> Result<String, ServerFnError> {
    let v = mcp::call("uninstall_app", json!({ "appId": app_id })).await.map_err(err)?;
    Ok(value_to_text(&v))
}

/// Updates every installed app with an update; returns the ids that were started.
#[server]
pub async fn update_all_apps() -> Result<Vec<String>, ServerFnError> {
    let apps: Vec<crate::model::AppInfo> = mcp::call_as("list_apps", json!({})).await.map_err(err)?;
    let mut started = Vec::new();
    for a in apps.into_iter().filter(|a| a.update_available) {
        if mcp::call("update_app", json!({ "appId": a.id })).await.is_ok() {
            started.push(a.name);
        }
    }
    Ok(started)
}

/// All tools the umbrelOS MCP server offers to this token.
#[server]
pub async fn mcp_tools() -> Result<Vec<McpTool>, ServerFnError> {
    let tools = mcp::list_tools().await.map_err(err)?;
    let mut out: Vec<McpTool> = tools
        .into_iter()
        .map(|t| {
            let name = t.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            let schema = t.get("inputSchema").cloned().unwrap_or_else(|| json!({}));
            McpTool {
                destructive: crate::model::DESTRUCTIVE_TOOLS.contains(&name.as_str()),
                description: t.get("description").and_then(Value::as_str).unwrap_or_default().to_string(),
                example: serde_json::to_string_pretty(&example_args(&schema)).unwrap_or_else(|_| "{}".into()),
                schema: serde_json::to_string_pretty(&schema).unwrap_or_default(),
                name,
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Runs any umbrelOS MCP tool with JSON arguments and returns the result as text.
#[server]
pub async fn mcp_run(name: String, args: String) -> Result<String, ServerFnError> {
    let args: Value = if args.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&args).map_err(|e| err(format!("Argumente sind kein gültiges JSON: {e}")))?
    };
    if !args.is_object() {
        return Err(err("Argumente müssen ein JSON-Objekt sein"));
    }
    let v = mcp::call(&name, args).await.map_err(err)?;
    Ok(value_to_text(&v))
}

#[cfg(feature = "server")]
fn example_args(schema: &Value) -> Value {
    let mut out = serde_json::Map::new();
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if let Some(props) = schema.get("properties").and_then(Value::as_object) {
        for (key, p) in props {
            if !required.contains(&key.as_str()) {
                continue;
            }
            let v = p.get("default").cloned().unwrap_or_else(|| {
                if let Some(first) = p.get("enum").and_then(Value::as_array).and_then(|e| e.first()) {
                    return first.clone();
                }
                match p.get("type").and_then(Value::as_str) {
                    Some("integer" | "number") => json!(0),
                    Some("boolean") => json!(false),
                    Some("array") => json!([]),
                    Some("object") => json!({}),
                    _ => json!(""),
                }
            });
            out.insert(key.clone(), v);
        }
    }
    Value::Object(out)
}
