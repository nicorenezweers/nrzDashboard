//! nrzNews: Kennzahlen und Schlagzeilen über den MCP-Server von nrzNews (Werkzeug
//! `dashboard_overview`) – für das Board-Widget „Nachrichten“. Adresse und API-Token liegen nur
//! auf dem Server (`$DATA_DIR/news.json`, System → nrzNews), nie im Board oder im Browser.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use super::err;
#[cfg(feature = "server")]
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NewsSettings {
    pub url: String,
    /// Masked token, `None` when no token is set.
    pub token_hint: Option<String>,
}

/// Antwort von `dashboard_overview`. Unbekannte Felder werden ignoriert, fehlende bekommen
/// Standardwerte – so bleibt das Dashboard mit neueren nrzNews-Versionen kompatibel.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsOverview {
    /// Adresse der nrzNews-Verbindung (setzt nrzDashboard, nicht nrzNews)
    pub source_url: String,
    pub version: String,
    pub user: String,
    pub generated_at: i64,
    pub period_hours: i64,
    pub counts: NewsCounts,
    pub metrics: Vec<NewsMetric>,
    pub last_fetched_at: Option<i64>,
    pub next_fetch_at: Option<i64>,
    pub top_feeds: Vec<NewsFeed>,
    pub feed_errors: Vec<NewsFeedError>,
    pub latest: Vec<NewsArticle>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsCounts {
    pub unread: i64,
    pub today: i64,
    pub starred: i64,
    pub new_in_period: i64,
    pub feeds: i64,
    pub folders: i64,
    pub feeds_with_errors: i64,
    pub articles: i64,
}

/// Fertiger Wert für eine Kachel: `status` ist "ok" oder "warning".
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsMetric {
    pub key: String,
    pub label: String,
    pub value: i64,
    pub status: String,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsFeed {
    pub id: i64,
    pub title: String,
    pub unread: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsFeedError {
    pub id: i64,
    pub title: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsArticle {
    pub id: i64,
    pub title: String,
    pub feed_title: String,
    pub url: String,
    pub published_at: i64,
    pub starred: bool,
}

#[cfg(feature = "server")]
async fn fetch(cfg: &crate::server::settings::Umbrel, headlines: u32) -> Result<NewsOverview, String> {
    let v = crate::server::mcp::call_at(
        "nrzNews",
        &format!("{}/mcp", cfg.url),
        &cfg.token,
        "dashboard_overview",
        json!({ "headlines": headlines.min(20) }),
    )
    .await?;
    let mut ov: NewsOverview = serde_json::from_value(v).map_err(|e| format!("Unerwartete Antwort von nrzNews: {e}"))?;
    ov.source_url = cfg.url.clone();
    Ok(ov)
}

/// Kennzahlen und die neuesten `headlines` ungelesenen Artikel. `None`, solange keine
/// nrzNews-Verbindung eingerichtet ist.
#[server]
pub async fn news_overview(headlines: u32) -> Result<Option<NewsOverview>, ServerFnError> {
    let cfg = crate::server::settings::news();
    if cfg.token.is_empty() {
        return Ok(None);
    }
    fetch(&cfg, headlines).await.map(Some).map_err(err)
}

#[server]
pub async fn get_news_settings() -> Result<NewsSettings, ServerFnError> {
    let n = crate::server::settings::news();
    Ok(NewsSettings { token_hint: super::mask(&n.token), url: n.url })
}

/// Testet URL/Token und speichert sie nur, wenn nrzNews antwortet (leeres Token = bisheriges,
/// `clear` = Verbindung trennen).
#[server]
pub async fn save_news_settings(url: String, token: String, clear: bool) -> Result<String, ServerFnError> {
    use crate::server::settings;
    let cfg = settings::news_candidate(&url, &token, clear).map_err(err)?;
    if clear {
        settings::save_news(cfg).map_err(err)?;
        return Ok("nrzNews-Verbindung getrennt.".into());
    }
    if cfg.token.is_empty() {
        return Err(err(settings::NO_NEWS_TOKEN));
    }
    let ov = fetch(&cfg, 0).await.map_err(|e| err(format!("{e} – nicht gespeichert.")))?;
    settings::save_news(cfg).map_err(err)?;
    Ok(format!(
        "Verbunden mit nrzNews {} (Konto {}) – {} ungelesen in {} Feeds",
        ov.version, ov.user, ov.counts.unread, ov.counts.feeds
    ))
}
