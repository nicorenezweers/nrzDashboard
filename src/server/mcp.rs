//! Minimal MCP client (JSON-RPC over Streamable HTTP): the umbrelOS MCP server and other MCP
//! servers such as nrzNews (`call_at`).

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use super::{http, settings};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

async fn rpc(method: &str, params: Value) -> Result<Value, String> {
    let cfg = settings::umbrel();
    if cfg.token.is_empty() {
        return Err(settings::NO_TOKEN.into());
    }
    rpc_at(
        "Umbrel",
        &format!("{}/mcp", cfg.url),
        &cfg.token,
        method,
        params,
    )
    .await
}

/// JSON-RPC call to any MCP endpoint (`label` names the server in error messages).
pub async fn rpc_at(
    label: &str,
    endpoint: &str,
    token: &str,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    let res = http()
        .post(endpoint)
        .bearer_auth(token)
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2025-06-18")
        .timeout(std::time::Duration::from_secs(60))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("{label} nicht erreichbar: {e}"))?;
    let status = res.status();
    let is_sse = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/event-stream"));
    let text = res.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        // JSON-RPC error body (e.g. 401 with a message) if there is one
        let message = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| text.chars().take(300).collect());
        // Not "401"/"Unauthorized" in the text: the UI reads that as an expired dashboard
        // session and sends the user to the login page (utils::err_msg).
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            let detail = Some(message.trim())
                .filter(|m| {
                    !m.is_empty()
                        && !m.contains("401")
                        && !m.contains("403")
                        && !m.to_lowercase().contains("unauthorized")
                })
                .map(|m| format!(" ({m})"))
                .unwrap_or_default();
            return Err(format!(
                "{label} lehnt das Token ab{detail} – Token prüfen."
            ));
        }
        return Err(format!("{label} MCP HTTP {status}: {message}"));
    }

    // SSE: take the JSON-RPC message whose id matches ours.
    let messages: Vec<Value> = if is_sse {
        text.lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .filter_map(|d| serde_json::from_str(d.trim()).ok())
            .collect()
    } else {
        vec![serde_json::from_str(&text).map_err(|e| e.to_string())?]
    };
    let msg = messages
        .into_iter()
        .find(|m| m.get("id").and_then(Value::as_u64) == Some(id))
        .ok_or("Keine Antwort vom MCP-Server")?;
    if let Some(err) = msg.get("error") {
        return Err(err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("MCP-Fehler")
            .to_string());
    }
    Ok(msg.get("result").cloned().unwrap_or(Value::Null))
}

/// Calls a tool and returns its text content (parsed as JSON when possible).
pub async fn call(name: &str, args: Value) -> Result<Value, String> {
    tool_result(rpc("tools/call", json!({ "name": name, "arguments": args })).await?)
}

/// Calls a tool on any MCP endpoint; prefers `structuredContent`, else the text (as JSON when possible).
pub async fn call_at(
    label: &str,
    endpoint: &str,
    token: &str,
    name: &str,
    args: Value,
) -> Result<Value, String> {
    let result = rpc_at(
        label,
        endpoint,
        token,
        "tools/call",
        json!({ "name": name, "arguments": args }),
    )
    .await?;
    let failed = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match result.get("structuredContent") {
        Some(v) if !failed && !v.is_null() => Ok(v.clone()),
        _ => tool_result(result),
    }
}

fn tool_result(result: Value) -> Result<Value, String> {
    let text: String = result
        .get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|c| c.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    if result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(if text.is_empty() {
            "Werkzeug meldet Fehler".into()
        } else {
            text
        });
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

/// Calls a tool and deserializes the JSON result into `T`.
pub async fn call_as<T: serde::de::DeserializeOwned>(name: &str, args: Value) -> Result<T, String> {
    let v = call(name, args).await?;
    serde_json::from_value(v).map_err(|e| format!("Unerwartete Antwort von {name}: {e}"))
}
