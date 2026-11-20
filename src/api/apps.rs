//! Installed apps for the home screen, with user and admin domain.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use super::err;

/// Own values for an app; empty = automatic (umbrelOS name, nrzNetwork domain).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct AppLinks {
    pub name: String,
    /// Public link, e.g. `https://bench.zweers.ch`
    pub user: String,
    /// Admin link, e.g. `https://admin.bench.zweers.ch`
    pub admin: String,
}

impl AppLinks {
    pub fn is_empty(&self) -> bool {
        self.name.trim().is_empty() && self.user.trim().is_empty() && self.admin.trim().is_empty()
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct AppTile {
    pub id: String,
    pub name: String,
    pub state: String,
    pub update_available: bool,
    pub hidden: bool,
    /// `https://bench.zweers.ch`
    pub user_url: Option<String>,
    /// `https://admin.bench.zweers.ch`
    pub admin_url: Option<String>,
    /// `http://umbrel.local:PORT` – only when nrzNetwork has no domain for the app.
    pub fallback_url: Option<String>,
    /// Name from umbrelOS and links found in nrzNetwork (before own values).
    pub auto_name: String,
    pub auto_user_url: Option<String>,
    pub auto_admin_url: Option<String>,
    /// Own values (right click → Bearbeiten).
    pub custom: AppLinks,
}

impl AppTile {
    /// Address for the chosen mode (`user` / `admin`), falling back to the other domain.
    pub fn url(&self, mode: &str) -> Option<String> {
        let (first, second) = if mode == "admin" { (&self.admin_url, &self.user_url) } else { (&self.user_url, &self.admin_url) };
        first.clone().or_else(|| second.clone()).or_else(|| self.fallback_url.as_deref().map(browser_url))
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state.as_str(), "ready" | "running")
    }
}

/// `umbrel.local` → host of the page (only for apps without a domain).
pub fn browser_url(url: &str) -> String {
    #[cfg(feature = "web")]
    if let Some(host) = web_sys::window().and_then(|w| w.location().hostname().ok()).filter(|h| !h.is_empty()) {
        return url.replace("umbrel.local", &host);
    }
    url.to_string()
}

#[server]
pub async fn app_tiles() -> Result<Vec<AppTile>, ServerFnError> {
    crate::server::apps::tiles().await.map_err(err)
}

#[server]
pub async fn app_set_hidden(id: String, hidden: bool) -> Result<(), ServerFnError> {
    crate::server::dash::update(|d| {
        d.hidden.retain(|h| *h != id);
        if hidden {
            d.hidden.push(id);
        }
    })
    .map_err(err)
}

/// Normalises a link: `bench.zweers.ch` → `https://bench.zweers.ch`, empty stays empty.
pub fn normalize_link(s: &str) -> Result<String, String> {
    let s = s.trim().trim_end_matches('/');
    if s.is_empty() {
        return Ok(String::new());
    }
    if s.chars().any(char::is_whitespace) {
        return Err(format!("„{s}“ ist keine gültige Adresse"));
    }
    let url = if s.starts_with("http://") || s.starts_with("https://") { s.to_string() } else { format!("https://{s}") };
    let host = url.split_once("://").map(|(_, r)| r).unwrap_or_default().split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() || host.starts_with('.') || host.starts_with(':') {
        return Err(format!("„{s}“ ist keine gültige Adresse"));
    }
    Ok(url)
}

/// Saves name, public link and admin link of an app (empty fields = automatic).
#[server]
pub async fn app_links_set(id: String, links: AppLinks) -> Result<(), ServerFnError> {
    let clean = AppLinks {
        name: links.name.trim().chars().take(40).collect(),
        user: normalize_link(&links.user).map_err(err)?,
        admin: normalize_link(&links.admin).map_err(err)?,
    };
    crate::server::dash::update(|d| {
        if clean.is_empty() {
            d.links.remove(&id);
        } else {
            d.links.insert(id, clean);
        }
    })
    .map_err(err)
}
