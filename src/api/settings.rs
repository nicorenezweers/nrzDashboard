//! Settings: greeting, app links, nrzNetwork, umbrelOS connection, password and 2FA.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use super::{err, mask};
#[cfg(feature = "server")]
use crate::server::{dash, mcp, settings};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct SettingsView {
    pub user: String,
    pub name: String,
    /// `user` or `admin`
    pub link_mode: String,
    pub network_url: String,
    pub network_token_hint: Option<String>,
    pub monitor_url: String,
    pub monitor_token_hint: Option<String>,
    pub umbrel_url: String,
    pub umbrel_token_hint: Option<String>,
    pub totp_enabled: bool,
    /// Hidden apps (id, name).
    pub hidden: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct TotpSetup {
    pub secret: String,
    pub uri: String,
    pub qr_svg: String,
}

#[cfg(feature = "server")]
fn pending_secret() -> &'static std::sync::Mutex<Option<String>> {
    static P: std::sync::OnceLock<std::sync::Mutex<Option<String>>> = std::sync::OnceLock::new();
    P.get_or_init(Default::default)
}

#[server]
pub async fn settings_get() -> Result<SettingsView, ServerFnError> {
    Ok(settings_view())
}

#[cfg(feature = "server")]
pub fn settings_view() -> SettingsView {
    let d = dash::get();
    let u = settings::umbrel();
    SettingsView {
        user: crate::server::config().user.clone(),
        name: d.name,
        link_mode: if d.link_mode == "admin" { "admin".into() } else { "user".into() },
        network_url: d.network_url,
        network_token_hint: mask(&d.network_token),
        monitor_url: if d.monitor_url.is_empty() { crate::server::monitor::DEFAULT_URL.into() } else { d.monitor_url },
        monitor_token_hint: mask(&d.monitor_token),
        umbrel_url: u.url,
        umbrel_token_hint: mask(&u.token),
        totp_enabled: d.totp_enabled,
        hidden: d.hidden,
    }
}

#[server]
pub async fn settings_save_general(name: String, link_mode: String) -> Result<(), ServerFnError> {
    let name: String = name.trim().chars().take(40).collect();
    let mode = if link_mode == "admin" { "admin" } else { "user" };
    dash::update(|d| {
        d.name = name;
        d.link_mode = mode.into();
    })
    .map_err(err)
}

/// Saves the nrzNetwork connection (empty token keeps the current one) and counts the domains.
#[server]
pub async fn settings_save_network(url: String, token: String) -> Result<String, ServerFnError> {
    let url = url.trim().trim_end_matches('/').to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(err("Adresse muss mit http:// oder https:// beginnen"));
    }
    let token = if token.trim().is_empty() { dash::get().network_token } else { token.trim().to_string() };
    let hosts = crate::server::apps::proxy_hosts_raw(&url, &token).await.map_err(err)?;
    dash::update(|d| {
        d.network_url = url;
        d.network_token = token;
    })
    .map_err(err)?;
    crate::server::apps::clear_host_cache();
    crate::server::cache::refresh_tiles().await;
    Ok(format!("Verbunden – {} Domains gefunden", hosts.len()))
}

/// Saves the nrzMonitor connection and tests it. Empty token keeps the current one, `-` removes it.
#[server]
pub async fn settings_save_monitor(url: String, token: String) -> Result<String, ServerFnError> {
    let url = url.trim().trim_end_matches('/').to_string();
    let url = if url.is_empty() { crate::server::monitor::DEFAULT_URL.to_string() } else { url };
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(err("Adresse muss mit http:// oder https:// beginnen"));
    }
    let token = match token.trim() {
        "" => dash::get().monitor_token,
        "-" => String::new(),
        t => t.to_string(),
    };
    let host = crate::server::monitor::test(&url, &token).await.map_err(err)?;
    dash::update(|d| {
        d.monitor_url = url;
        d.monitor_token = token;
    })
    .map_err(err)?;
    crate::server::cache::reset_monitor();
    Ok(format!("Verbunden mit {host}"))
}

/// Saves the umbrelOS connection (empty token keeps the current one) and tests it.
#[server]
pub async fn settings_save_umbrel(url: String, token: String) -> Result<String, ServerFnError> {
    settings::save(&url, &token).map_err(err)?;
    let info: crate::model::SystemInfo = mcp::call_as("get_system_info", serde_json::json!({})).await.map_err(err)?;
    Ok(format!("Verbunden mit {}", info.hostname))
}

#[server]
pub async fn password_change(current: String, new: String) -> Result<(), ServerFnError> {
    if !crate::server::auth::verify_password(&current) {
        return Err(err("Aktuelles Passwort ist falsch"));
    }
    if new.chars().count() < 10 {
        return Err(err("Das neue Passwort braucht mindestens 10 Zeichen"));
    }
    let hash = crate::server::auth::hash_password(&new).map_err(err)?;
    dash::update(|d| d.password_hash = hash).map_err(err)
}

/// Starts the 2FA setup: new secret + QR code (active only after `totp_enable`).
#[server]
pub async fn totp_begin() -> Result<TotpSetup, ServerFnError> {
    let secret = dash::new_secret();
    let uri = dash::otpauth_uri(&secret, &crate::server::config().user);
    let qr_svg = dash::qr_svg(&uri);
    if let Ok(mut p) = pending_secret().lock() {
        *p = Some(secret.clone());
    }
    Ok(TotpSetup { secret, uri, qr_svg })
}

#[server]
pub async fn totp_enable(code: String) -> Result<(), ServerFnError> {
    let secret = pending_secret().lock().ok().and_then(|p| p.clone()).ok_or_else(|| err("Einrichtung neu starten"))?;
    if !dash::totp_check(&secret, &code) {
        return Err(err("Code stimmt nicht – Uhrzeit des Handys prüfen und neuen Code eingeben"));
    }
    dash::update(|d| {
        d.totp_secret = secret;
        d.totp_enabled = true;
    })
    .map_err(err)?;
    if let Ok(mut p) = pending_secret().lock() {
        *p = None;
    }
    Ok(())
}

#[server]
pub async fn totp_disable(code: String) -> Result<(), ServerFnError> {
    let d = dash::get();
    if !dash::totp_check(&d.totp_secret, &code) {
        return Err(err("Code stimmt nicht"));
    }
    dash::update(|d| {
        d.totp_enabled = false;
        d.totp_secret.clear();
    })
    .map_err(err)
}
