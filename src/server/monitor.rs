//! nrzMonitor client (REST): Docker containers, file systems, reachability, sensors, alerts and
//! events. Inside the umbrel network nrzMonitor listens on the gateway (`10.21.0.1:8571`) without a
//! key; for a public address (`https://monitor…`) the MCP key goes through `/mcp/api/v1`.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::{dash, http};
use crate::model::{Container, Filesystem, MonEvent, MonitorView, Probe, Sensor};

pub const DEFAULT_URL: &str = "http://10.21.0.1:8571";

/// API base: `…/api/v1` (internal, no key) or `…/mcp/api/v1` (with key).
fn base(url: &str, token: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix("/mcp").unwrap_or(url);
    if token.is_empty() {
        format!("{url}/api/v1")
    } else {
        format!("{url}/mcp/api/v1")
    }
}

async fn get_raw(url: &str, token: &str, path: &str) -> Result<Value, String> {
    let mut req = http()
        .get(format!("{}/{path}", base(url, token)))
        .timeout(Duration::from_secs(8));
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    let res = req
        .send()
        .await
        .map_err(|e| format!("nrzMonitor nicht erreichbar: {e}"))?;
    match res.status().as_u16() {
        200..=299 => res
            .json()
            .await
            .map_err(|e| format!("nrzMonitor: unerwartete Antwort ({e})")),
        401 | 403 => Err("nrzMonitor: Schlüssel fehlt oder ist falsch".into()),
        s => Err(format!("nrzMonitor: HTTP {s}")),
    }
}

/// `data` of a snapshot, empty on any error (one missing collector should not hide the rest).
async fn snap<T: DeserializeOwned + Default>(url: &str, token: &str, key: &str) -> T {
    get_raw(url, token, &format!("snap/{key}"))
        .await
        .ok()
        .and_then(|v| v.get("data").cloned())
        .and_then(|d| serde_json::from_value(d).ok())
        .unwrap_or_default()
}

/// Connection test: returns the host name.
pub async fn test(url: &str, token: &str) -> Result<String, String> {
    let v = get_raw(url, token, "snap/system.info").await?;
    Ok(v.pointer("/data/hostname")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_string())
}

pub async fn view() -> Result<MonitorView, String> {
    let d = dash::get();
    let (url, token) = (
        if d.monitor_url.is_empty() {
            DEFAULT_URL.to_string()
        } else {
            d.monitor_url
        },
        d.monitor_token,
    );
    if url == "-" {
        return Err("nrzMonitor ist ausgeschaltet".into());
    }
    // The container list doubles as the reachability check.
    let containers: Vec<Container> = serde_json::from_value(
        get_raw(&url, &token, "snap/docker.containers")
            .await?
            .get("data")
            .cloned()
            .unwrap_or_default(),
    )
    .map_err(|e| format!("nrzMonitor: Container nicht lesbar ({e})"))?;
    let (filesystems, probes, sensors, alerts, events) = futures::join!(
        snap::<Vec<Filesystem>>(&url, &token, "system.filesystems"),
        snap::<Value>(&url, &token, "probes.status"),
        snap::<Vec<Sensor>>(&url, &token, "system.sensors"),
        snap::<Vec<Value>>(&url, &token, "alerts.active"),
        get_raw(&url, &token, "events?limit=12"),
    );
    let probes: Vec<Probe> = probes
        .get("targets")
        .cloned()
        .and_then(|t| serde_json::from_value(t).ok())
        .unwrap_or_default();
    let alerts = alerts
        .iter()
        .map(|a| {
            ["title", "name", "message", "rule"]
                .iter()
                .find_map(|k| a.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty()))
                .unwrap_or("Alarm")
                .to_string()
        })
        .collect();
    let events: Vec<MonEvent> = events
        .ok()
        .and_then(|v| v.get("events").cloned())
        .and_then(|e| serde_json::from_value(e).ok())
        .unwrap_or_default();
    Ok(MonitorView {
        containers,
        filesystems,
        probes,
        sensors: sensors
            .into_iter()
            .filter(|s| s.kind == "temp" && s.value.is_some())
            .collect(),
        alerts,
        events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bases() {
        assert_eq!(
            base("http://10.21.0.1:8571/", ""),
            "http://10.21.0.1:8571/api/v1"
        );
        assert_eq!(
            base("https://monitor.zweers.ch/mcp", "k"),
            "https://monitor.zweers.ch/mcp/api/v1"
        );
        assert_eq!(
            base("https://monitor.zweers.ch", "k"),
            "https://monitor.zweers.ch/mcp/api/v1"
        );
    }
}
