//! Boards (Homarr-style modular dashboards): types shared by client and server + server functions.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(feature = "server")]
use super::err;

/// Number of grid columns on desktop.
pub const COLS: i32 = 12;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "lowercase")]
pub enum WidgetKind {
    Apps,
    App,
    System,
    Monitors,
    News,
    Clock,
    Weather,
    Notes,
    Bookmarks,
    Search,
    Iframe,
    Dns,
    Finance,
}

impl WidgetKind {
    pub const ALL: [Self; 13] = [
        Self::Apps,
        Self::App,
        Self::System,
        Self::Monitors,
        Self::News,
        Self::Clock,
        Self::Weather,
        Self::Search,
        Self::Bookmarks,
        Self::Notes,
        Self::Iframe,
        Self::Dns,
        Self::Finance,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Apps => "Umbrel-Apps",
            Self::App => "App-Kachel",
            Self::System => "System",
            Self::Monitors => "Monitore",
            Self::News => "Nachrichten",
            Self::Clock => "Uhr",
            Self::Weather => "Wetter",
            Self::Notes => "Notizen",
            Self::Bookmarks => "Lesezeichen",
            Self::Search => "Suche",
            Self::Iframe => "iFrame",
            Self::Dns => "DNS (nrzDNS)",
            Self::Finance => "Finanzen (nrzBudget)",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Apps => "Alle installierten Apps als Kacheln mit Status",
            Self::App => "Eine App oder ein Link mit Erreichbarkeits-Ping",
            Self::System => "CPU, RAM, Speicher und Temperatur des Umbrel",
            Self::Monitors => "Status und Heartbeats deiner Monitore",
            Self::News => "Ungelesen, Heute, Neu, Gemerkt, Feed-Fehler und Schlagzeilen aus nrzNews",
            Self::Clock => "Uhrzeit und Datum",
            Self::Weather => "Aktuelles Wetter und Vorhersage (Open-Meteo)",
            Self::Notes => "Notizen in Markdown",
            Self::Bookmarks => "Liste mit Links",
            Self::Search => "Websuche direkt vom Board",
            Self::Iframe => "Beliebige Webseite einbetten",
            Self::Dns => "Anfragen, Sperrquote, Top-Domains und Upstreams von nrzDNS",
            Self::Finance => "Einnahmen, Ausgaben, Sparquote und Top-Ausgaben aus nrzBudget",
        }
    }

    /// Default (w, h) in grid cells.
    pub const fn default_size(self) -> (i32, i32) {
        match self {
            Self::Apps => (8, 4),
            Self::App => (2, 2),
            Self::System => (4, 3),
            Self::Monitors => (4, 4),
            Self::News => (4, 4),
            Self::Clock => (3, 2),
            Self::Weather => (3, 2),
            Self::Notes => (4, 3),
            Self::Bookmarks => (3, 3),
            Self::Search => (6, 1),
            Self::Iframe => (6, 4),
            Self::Dns => (4, 4),
            Self::Finance => (4, 4),
        }
    }

    /// Minimum (w, h).
    pub const fn min_size(self) -> (i32, i32) {
        match self {
            Self::Search => (3, 1),
            Self::App | Self::Clock | Self::Weather => (2, 1),
            _ => (2, 2),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Widget {
    pub id: String,
    pub kind: WidgetKind,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// Optional title shown above the widget.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub options: Value,
}

impl Widget {
    pub fn opt_str(&self, key: &str) -> String {
        self.options.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
    }

    pub fn opt_bool(&self, key: &str, default: bool) -> bool {
        self.options.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    pub fn opt_f64(&self, key: &str) -> Option<f64> {
        self.options.get(key).and_then(Value::as_f64)
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Board {
    pub id: String,
    pub name: String,
    pub widgets: Vec<Widget>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct BoardMeta {
    pub id: String,
    pub name: String,
}

// ---------- Widget data ----------

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct AppTile {
    pub id: String,
    /// Display name (custom name if set).
    pub name: String,
    /// Name reported by umbrelOS.
    pub original_name: String,
    pub state: String,
    /// App URL (custom URL if set, else `http://umbrel.local:PORT` from umbrelOS, rewritten in the browser).
    pub url: Option<String>,
    /// Custom icon URL.
    pub icon: Option<String>,
    pub hidden: bool,
    pub update_available: bool,
}

/// Per-app customisation (right-click → Anpassen), valid on every board.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct AppOverride {
    pub name: String,
    pub icon: String,
    pub url: String,
    pub hidden: bool,
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
impl AppOverride {
    #[cfg_attr(not(feature = "server"), allow(dead_code))]
    pub fn is_empty(&self) -> bool {
        self.name.is_empty() && self.icon.is_empty() && self.url.is_empty() && !self.hidden
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Weather {
    pub temperature: f64,
    pub apparent: f64,
    pub code: i32,
    pub wind: f64,
    pub humidity: f64,
    pub is_day: bool,
    /// (ISO date, code, max, min)
    pub daily: Vec<(String, i32, f64, f64)>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Place {
    pub name: String,
    pub region: String,
    pub lat: f64,
    pub lon: f64,
}

// ---------- Server functions ----------

#[server]
pub async fn boards_list() -> Result<Vec<BoardMeta>, ServerFnError> {
    Ok(crate::server::board::list())
}

#[server]
pub async fn board_get(id: String) -> Result<Board, ServerFnError> {
    crate::server::board::get(&id).map_err(err)
}

#[server]
pub async fn board_save(board: Board) -> Result<(), ServerFnError> {
    crate::server::board::save(board).map_err(err)
}

#[server]
pub async fn board_create(name: String) -> Result<Board, ServerFnError> {
    crate::server::board::create(&name).map_err(err)
}

#[server]
pub async fn board_delete(id: String) -> Result<(), ServerFnError> {
    crate::server::board::delete(&id).map_err(err)
}

#[server]
pub async fn app_tiles() -> Result<Vec<AppTile>, ServerFnError> {
    crate::server::board::app_tiles().await.map_err(err)
}

/// Reachability check for app tiles: latency in ms, `None` when down.
#[server]
pub async fn ping_url(url: String) -> Result<Option<f64>, ServerFnError> {
    Ok(crate::server::board::ping_url(&url).await)
}

#[server]
pub async fn weather(lat: f64, lon: f64) -> Result<Weather, ServerFnError> {
    crate::server::board::weather(lat, lon).await.map_err(err)
}

#[server]
pub async fn geocode(query: String) -> Result<Vec<Place>, ServerFnError> {
    crate::server::board::geocode(&query).await.map_err(err)
}

#[server]
pub async fn app_override_get(id: String) -> Result<AppOverride, ServerFnError> {
    Ok(crate::server::board::override_get(&id))
}

#[server]
pub async fn app_override_set(id: String, value: AppOverride) -> Result<(), ServerFnError> {
    crate::server::board::override_set(&id, value).map_err(err)
}
