//! Monitoring (Uptime-Kuma-artig): gemeinsame Typen für Client und Server plus Server-Funktionen.
//!
//! Board-Schnittstelle: [`monitor_overview`] liefert je Monitor eine [`MonitorSummary`]
//! (Status, letzte Beats, Uptime 24 h, Latenz); die Leiste dazu ist
//! `crate::pages::monitors::HeartbeatBar`.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use super::err;
#[cfg(feature = "server")]
use crate::server::monitor as engine;

// ---------- Typen ----------

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub enum MonitorKind {
    /// HTTP(S)-Anfrage, Statuscode/Keyword prüfen.
    #[default]
    Http,
    /// TCP-Verbindung zu `host:port`.
    Tcp,
    /// ICMP-Echo.
    Ping,
    /// DNS-Auflösung (A/AAAA).
    Dns,
    /// Umbrel-App-Status über MCP.
    App,
    /// Push/Heartbeat: der Dienst ruft eine geheime URL auf.
    Push,
}

impl MonitorKind {
    pub const ALL: [Self; 6] = [
        Self::Http,
        Self::Tcp,
        Self::Ping,
        Self::Dns,
        Self::App,
        Self::Push,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Http => "HTTP(S)",
            Self::Tcp => "TCP-Port",
            Self::Ping => "Ping",
            Self::Dns => "DNS",
            Self::App => "Umbrel-App",
            Self::Push => "Push",
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Tcp => "tcp",
            Self::Ping => "ping",
            Self::Dns => "dns",
            Self::App => "app",
            Self::Push => "push",
        }
    }

    pub fn parse(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|k| k.key() == s)
            .unwrap_or_default()
    }

    pub const fn target_label(self) -> &'static str {
        match self {
            Self::Http => "URL",
            Self::Tcp => "Host:Port",
            Self::Ping => "Host",
            Self::Dns => "Hostname",
            Self::App => "App-ID",
            Self::Push => "",
        }
    }

    pub const fn placeholder(self) -> &'static str {
        match self {
            Self::Http => "https://example.com",
            Self::Tcp => "1.1.1.1:53",
            Self::Ping => "8.8.8.8",
            Self::Dns => "example.com",
            Self::App => "z.B. nextcloud",
            Self::Push => "",
        }
    }
}

/// Zustand eines Heartbeats bzw. Monitors.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Up,
    Down,
    /// Fehlgeschlagen, aber Wiederholungen noch nicht aufgebraucht.
    Pending,
    Paused,
    /// Noch keine Messung.
    #[default]
    Unknown,
}

impl Status {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Pending => "Ausstehend",
            Self::Paused => "Pausiert",
            Self::Unknown => "Unbekannt",
        }
    }

    /// Theme-Hintergrundklasse (Balken, Punkte).
    pub const fn bg(self) -> &'static str {
        match self {
            Self::Up => "bg-success",
            Self::Down => "bg-destructive",
            Self::Pending => "bg-warning",
            Self::Paused | Self::Unknown => "bg-muted-foreground/40",
        }
    }

    pub const fn text(self) -> &'static str {
        match self {
            Self::Up => "text-success",
            Self::Down => "text-destructive",
            Self::Pending => "text-warning",
            Self::Paused | Self::Unknown => "text-muted-foreground",
        }
    }
}

/// Monitor-Definition (gespeichert in `$DATA_DIR/monitors.json`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Monitor {
    pub id: String,
    pub name: String,
    pub kind: MonitorKind,
    /// URL, `host:port`, Host, DNS-Name oder App-ID (je nach Typ; bei Push leer).
    pub target: String,
    /// HTTP-Methode (GET, HEAD, POST …).
    pub method: String,
    /// Erwartete Statuscodes, z.B. `200-299, 301`.
    pub accepted_codes: String,
    /// Optionales Keyword im Antworttext.
    pub keyword: String,
    /// `true`: Keyword darf NICHT enthalten sein.
    pub keyword_invert: bool,
    /// TLS-Zertifikatsfehler ignorieren.
    pub ignore_tls: bool,
    /// DNS: `A`, `AAAA` oder leer (beides).
    pub dns_record: String,
    /// DNS: optional erwartete IP.
    pub dns_expected: String,
    /// Push: geheimes Token für `/monitors/push/<token>`.
    pub push_token: String,
    /// Prüfintervall in Sekunden.
    pub interval: u32,
    /// Fehlversuche, bevor der Monitor als down gilt.
    pub retries: u32,
    /// Intervall zwischen Wiederholungen (Sekunden).
    pub retry_interval: u32,
    /// Zeitlimit in Sekunden.
    pub timeout: u32,
    pub paused: bool,
    pub group: String,
    pub description: String,
    /// Benachrichtigungen bei Statuswechsel.
    pub notify: bool,
}

impl Default for Monitor {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: MonitorKind::Http,
            target: String::new(),
            method: "GET".into(),
            accepted_codes: "200-299".into(),
            keyword: String::new(),
            keyword_invert: false,
            ignore_tls: false,
            dns_record: String::new(),
            dns_expected: String::new(),
            push_token: String::new(),
            interval: 60,
            retries: 1,
            retry_interval: 30,
            timeout: 10,
            paused: false,
            group: String::new(),
            description: String::new(),
            notify: true,
        }
    }
}

impl Monitor {
    /// Kurzbeschreibung des Ziels für Listen.
    pub fn target_display(&self) -> String {
        match self.kind {
            MonitorKind::Push => "Push-Heartbeat".into(),
            MonitorKind::Http if self.method != "GET" && !self.method.is_empty() => {
                format!("{} {}", self.method, self.target)
            }
            _ => self.target.clone(),
        }
    }
}

/// Ein Messpunkt (eine Zeile in `heartbeats/<id>.jsonl`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Heartbeat {
    /// Unix-Sekunden.
    #[serde(rename = "t")]
    pub ts: u64,
    #[serde(rename = "s")]
    pub status: Status,
    /// Antwortzeit in ms.
    #[serde(rename = "ms", default, skip_serializing_if = "Option::is_none")]
    pub ms: Option<f64>,
    #[serde(rename = "m", default, skip_serializing_if = "String::is_empty")]
    pub msg: String,
}

/// Kurzfassung eines Monitors (Liste links, Board).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct MonitorSummary {
    pub id: String,
    pub name: String,
    pub kind: MonitorKind,
    pub target: String,
    pub group: String,
    /// Aktueller Zustand (`Paused` wenn pausiert).
    pub status: Status,
    /// Die letzten ~50 Heartbeats (älteste zuerst).
    pub beats: Vec<Heartbeat>,
    /// Uptime der letzten 24 h in Prozent.
    pub uptime_24h: Option<f64>,
    /// Letzte Antwortzeit in ms.
    pub latency_ms: Option<f64>,
    /// Ø-Antwortzeit 24 h in ms.
    pub avg_24h: Option<f64>,
    /// Meldung des letzten Heartbeats.
    pub msg: String,
}

/// Punkt im Antwortzeit-Diagramm (ggf. gemittelt).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct ChartPoint {
    pub ts: u64,
    pub ms: Option<f64>,
    /// Mindestens ein Fehlschlag in diesem Abschnitt.
    pub down: bool,
}

/// Statuswechsel up↔down.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct MonitorEvent {
    pub ts: u64,
    pub status: Status,
    /// Wie lange dieser Zustand anhielt (bis zum nächsten Wechsel bzw. bis jetzt).
    pub duration_secs: u64,
    pub msg: String,
    /// Zustand hält noch an.
    pub ongoing: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct MonitorDetail {
    pub monitor: Monitor,
    pub status: Status,
    /// Die letzten ~100 Heartbeats.
    pub beats: Vec<Heartbeat>,
    pub latency_ms: Option<f64>,
    pub avg_24h: Option<f64>,
    pub uptime_24h: Option<f64>,
    pub uptime_7d: Option<f64>,
    pub uptime_30d: Option<f64>,
    /// Ablauf des TLS-Zertifikats (Unix-Sekunden), nur HTTPS.
    pub cert_expiry: Option<u64>,
    pub chart: Vec<ChartPoint>,
    pub events: Vec<MonitorEvent>,
    /// Server-Zeit (Unix-Sekunden).
    pub now: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum NotifyKind {
    #[default]
    Ntfy,
    Discord,
    Telegram,
    Webhook,
}

impl NotifyKind {
    pub const ALL: [Self; 4] = [Self::Ntfy, Self::Discord, Self::Telegram, Self::Webhook];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ntfy => "ntfy",
            Self::Discord => "Discord",
            Self::Telegram => "Telegram",
            Self::Webhook => "Webhook",
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Ntfy => "ntfy",
            Self::Discord => "discord",
            Self::Telegram => "telegram",
            Self::Webhook => "webhook",
        }
    }

    pub fn parse(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|k| k.key() == s)
            .unwrap_or_default()
    }
}

/// Präfix maskierter Geheimnisse; beim Speichern bleibt der alte Wert erhalten.
#[cfg_attr(not(feature = "server"), allow(dead_code))]
pub const MASK: &str = "••••";

/// Benachrichtigungskanal (`$DATA_DIR/notifications.json`, Datei 0600).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct NotifyChannel {
    pub id: String,
    pub name: String,
    pub kind: NotifyKind,
    pub enabled: bool,
    /// ntfy: Server-URL; Discord/Webhook: Webhook-URL.
    pub url: String,
    /// ntfy: Topic.
    pub topic: String,
    /// ntfy: Zugriffstoken (optional); Telegram: Bot-Token.
    pub token: String,
    /// Telegram: Chat-ID.
    pub chat_id: String,
}

// ---------- Server-Funktionen ----------

/// Alle Monitore in Kurzform (auch für das Board).
#[server]
pub async fn monitor_overview() -> Result<Vec<MonitorSummary>, ServerFnError> {
    Ok(engine::overview())
}

/// Details eines Monitors; `range_secs` bestimmt den Zeitraum des Diagramms.
#[server]
pub async fn monitor_detail(id: String, range_secs: u64) -> Result<MonitorDetail, ServerFnError> {
    engine::detail(&id, range_secs).ok_or_else(|| err("Monitor nicht gefunden"))
}

/// Legt einen Monitor an (leere `id`) oder ändert ihn. Gibt die ID zurück.
#[server]
pub async fn save_monitor(monitor: Monitor) -> Result<String, ServerFnError> {
    engine::save(monitor).map_err(err)
}

#[server]
pub async fn delete_monitor(id: String) -> Result<(), ServerFnError> {
    engine::delete(&id).map_err(err)
}

#[server]
pub async fn set_monitor_paused(id: String, paused: bool) -> Result<(), ServerFnError> {
    engine::set_paused(&id, paused).map_err(err)
}

/// Installierte Umbrel-Apps (ID, Name) für das Formular.
#[server]
pub async fn monitor_app_choices() -> Result<Vec<(String, String)>, ServerFnError> {
    engine::app_choices().await.map_err(err)
}

/// Benachrichtigungskanäle (Geheimnisse maskiert).
#[server]
pub async fn notify_channels() -> Result<Vec<NotifyChannel>, ServerFnError> {
    Ok(engine::notify::channels_masked())
}

#[server]
pub async fn save_notify_channel(channel: NotifyChannel) -> Result<String, ServerFnError> {
    engine::notify::save(channel).map_err(err)
}

#[server]
pub async fn delete_notify_channel(id: String) -> Result<(), ServerFnError> {
    engine::notify::delete(&id).map_err(err)
}

/// Sendet eine Testnachricht (maskierte Felder werden aus dem gespeicherten Kanal ergänzt).
#[server]
pub async fn test_notify_channel(channel: NotifyChannel) -> Result<String, ServerFnError> {
    engine::notify::test(channel).await.map_err(err)
}

// ---------- Formatierung ----------

/// Dauer kurz und deutsch, z.B. `3 T 4 Std`, `12 Min`, `40 s`.
pub fn fmt_duration(secs: u64) -> String {
    let (d, h, m, s) = (
        secs / 86_400,
        (secs % 86_400) / 3600,
        (secs % 3600) / 60,
        secs % 60,
    );
    if d > 0 {
        format!("{d} T {h} Std")
    } else if h > 0 {
        format!("{h} Std {m} Min")
    } else if m > 0 {
        format!("{m} Min")
    } else {
        format!("{s} s")
    }
}
