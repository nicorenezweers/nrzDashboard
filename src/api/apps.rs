//! Installed apps for the home screen, with user and admin domain.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use super::err;

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
