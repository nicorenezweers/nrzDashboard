//! Installed apps with their domains: umbrelOS gives the app port, nrzNetwork's reverse proxy
//! maps names to ports – `bench.zweers.ch` (user) and `admin.bench.zweers.ch` (admin), always HTTPS.
//! Plus `/appicon/{id}`: app icons fetched on the server (no IP addresses in the browser).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::{dash, http, mcp, settings};
use crate::api::apps::AppTile;
use crate::model::AppInfo;

/// Port of a URL like `http://umbrel.local:8350/x`.
fn port_of(url: &str) -> Option<u16> {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split('/').next()?;
    authority.rsplit_once(':')?.1.parse().ok()
}

// ---------- App ports (umbrelOS) ----------

fn port_cache() -> &'static Mutex<HashMap<String, (Instant, Option<u16>)>> {
    static C: OnceLock<Mutex<HashMap<String, (Instant, Option<u16>)>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

async fn app_port(id: &str) -> Option<u16> {
    if let Some((t, p)) = port_cache().lock().ok().and_then(|c| c.get(id).copied()) {
        if t.elapsed() < Duration::from_secs(1800) {
            return p;
        }
    }
    let port = mcp::call("get_app_details", json!({ "appId": id }))
        .await
        .ok()
        .and_then(|v| v.get("url").and_then(Value::as_str).and_then(port_of));
    if let Ok(mut c) = port_cache().lock() {
        c.insert(id.to_string(), (Instant::now(), port));
    }
    port
}

// ---------- Domains (nrzNetwork) ----------

#[derive(Clone)]
struct ProxyHost {
    host: String,
    port: u16,
}

fn host_cache() -> &'static Mutex<Option<(Instant, Vec<ProxyHost>)>> {
    static C: OnceLock<Mutex<Option<(Instant, Vec<ProxyHost>)>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

pub fn clear_host_cache() {
    if let Ok(mut c) = host_cache().lock() {
        *c = None;
    }
}

/// Reverse proxy entries from nrzNetwork (`get_proxy`).
pub async fn proxy_hosts_raw(url: &str, token: &str) -> Result<Vec<(String, String, bool)>, String> {
    if token.trim().is_empty() {
        return Err("Kein nrzNetwork-Token".into());
    }
    let v = mcp::call_at("nrzNetwork", url, token, "get_proxy", json!({})).await?;
    let hosts = v.get("hosts").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(hosts
        .iter()
        .map(|h| {
            let s = |k: &str| h.get(k).and_then(Value::as_str).unwrap_or_default().to_lowercase();
            (s("host"), s("target"), h.get("enabled").and_then(Value::as_bool).unwrap_or(true))
        })
        .collect())
}

async fn proxy_hosts() -> Vec<ProxyHost> {
    if let Some((t, hosts)) = host_cache().lock().ok().and_then(|c| c.clone()) {
        if t.elapsed() < Duration::from_secs(120) {
            return hosts;
        }
    }
    let d = dash::get();
    let hosts: Vec<ProxyHost> = proxy_hosts_raw(&d.network_url, &d.network_token)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|(h, _, enabled)| *enabled && !h.is_empty())
        .filter_map(|(host, target, _)| Some(ProxyHost { port: port_of(&target)?, host }))
        .collect();
    if let Ok(mut c) = host_cache().lock() {
        *c = Some((Instant::now(), hosts.clone()));
    }
    hosts
}

/// (user, admin) domain for an app port.
fn domains_for(port: u16, hosts: &[ProxyHost]) -> (Option<String>, Option<String>) {
    let exists = |name: &str| hosts.iter().any(|h| h.host == name);
    let on_port: Vec<&ProxyHost> = hosts.iter().filter(|h| h.port == port).collect();
    let admin = on_port.iter().find(|h| h.host.starts_with("admin.")).map(|h| h.host.clone());
    let user = admin
        .as_deref()
        .and_then(|a| a.strip_prefix("admin."))
        .filter(|u| exists(u))
        .map(str::to_string)
        .or_else(|| on_port.iter().find(|h| !h.host.starts_with("admin.")).map(|h| h.host.clone()));
    // Only the user domain points at the app port: an admin twin may still exist.
    let admin = admin.or_else(|| user.as_ref().map(|u| format!("admin.{u}")).filter(|a| exists(a)));
    (user, admin)
}

pub async fn tiles() -> Result<Vec<AppTile>, String> {
    let apps: Vec<AppInfo> = mcp::call_as("list_apps", json!({})).await?;
    let hosts = proxy_hosts().await;
    let hidden = dash::get().hidden;
    let ports = futures::future::join_all(apps.iter().map(|a| app_port(&a.id))).await;
    let mut tiles: Vec<AppTile> = apps
        .into_iter()
        .zip(ports)
        .map(|(a, port)| {
            let (user, admin) = port.map(|p| domains_for(p, &hosts)).unwrap_or_default();
            AppTile {
                hidden: hidden.contains(&a.id),
                user_url: user.map(|h| format!("https://{h}")),
                admin_url: admin.map(|h| format!("https://{h}")),
                fallback_url: port.map(|p| format!("http://umbrel.local:{p}")),
                name: a.name,
                state: a.state,
                update_available: a.update_available,
                id: a.id,
            }
        })
        .collect();
    tiles.sort_by_key(|t| t.name.to_lowercase());
    Ok(tiles)
}

// ---------- Icons ----------

type Icon = Option<(String, Vec<u8>)>;

fn icon_cache() -> &'static Mutex<HashMap<String, (Instant, Icon)>> {
    static C: OnceLock<Mutex<HashMap<String, (Instant, Icon)>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn icon_sources(id: &str) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(base) = super::env("NRZSTORE_ICON_BASE") {
        v.push(format!("{}/{id}/icon.png", base.trim_end_matches('/')));
    } else {
        // nrzGit on the same device (port 8390), reached through the configured umbrelOS address.
        let u = settings::umbrel().url;
        let host = u.split_once("://").map_or(u.as_str(), |(_, r)| r).split(['/', ':']).next().unwrap_or("umbrel.local").to_string();
        v.push(format!("http://{host}:8390/nicorenezweers/nrzStore/raw/branch/main/{id}/icon.png"));
    }
    v.push(format!("https://getumbrel.github.io/umbrel-apps-gallery/{id}/icon.svg"));
    v
}

async fn fetch_icon(id: &str) -> Icon {
    for src in icon_sources(id) {
        let Ok(res) = http().get(&src).timeout(Duration::from_secs(6)).send().await else { continue };
        if !res.status().is_success() {
            continue;
        }
        let ctype = res.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let ctype = if src.ends_with(".svg") { "image/svg+xml".to_string() } else if ctype.starts_with("image/") { ctype } else { "image/png".into() };
        if let Ok(bytes) = res.bytes().await {
            if !bytes.is_empty() && bytes.len() < 2_000_000 {
                return Some((ctype, bytes.to_vec()));
            }
        }
    }
    None
}

async fn icon_handler(Path(id): Path<String>) -> Response {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let cached = icon_cache().lock().ok().and_then(|c| c.get(&id).cloned());
    let icon = match cached {
        Some((_, Some(icon))) => Some(icon),
        Some((t, None)) if t.elapsed() < Duration::from_secs(600) => None,
        _ => {
            let icon = fetch_icon(&id).await;
            if let Ok(mut c) = icon_cache().lock() {
                c.insert(id.clone(), (Instant::now(), icon.clone()));
            }
            icon
        }
    };
    match icon {
        Some((ctype, bytes)) => ([(header::CONTENT_TYPE, ctype), (header::CACHE_CONTROL, "private, max-age=86400".into())], bytes).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub fn routes() -> axum::Router {
    axum::Router::new().route("/appicon/{id}", axum::routing::get(icon_handler))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(host: &str, port: u16) -> ProxyHost {
        ProxyHost { host: host.into(), port }
    }

    #[test]
    fn ports() {
        assert_eq!(port_of("http://100.78.107.162:8350/x"), Some(8350));
        assert_eq!(port_of("http://umbrel.local:2000"), Some(2000));
        assert_eq!(port_of("https://bench.zweers.ch"), None);
    }

    #[test]
    fn user_and_admin() {
        let hosts = [h("admin.bench.zweers.ch", 8500), h("bench.zweers.ch", 8501), h("music.zweers.ch", 8350)];
        assert_eq!(domains_for(8500, &hosts), (Some("bench.zweers.ch".into()), Some("admin.bench.zweers.ch".into())));
        assert_eq!(domains_for(8350, &hosts), (Some("music.zweers.ch".into()), None));
        assert_eq!(domains_for(8501, &hosts), (Some("bench.zweers.ch".into()), Some("admin.bench.zweers.ch".into())));
        assert_eq!(domains_for(9999, &hosts), (None, None));
    }
}
