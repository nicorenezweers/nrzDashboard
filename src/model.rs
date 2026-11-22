//! Types shared between server and client.

use serde::{Deserialize, Serialize};

// ---------- umbrelOS ----------

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SystemInfo {
    pub version_name: String,
    pub status: String,
    pub release_channel: String,
    pub hostname: String,
    pub ip_addresses: Vec<String>,
    pub uptime_seconds: f64,
    pub manufacturer: String,
    pub product_name: String,
    pub cpu: String,
    pub memory_size: f64,
    pub memory_type: String,
    pub storage_size: f64,
    pub storage_type: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AppUse {
    pub id: String,
    pub used: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CpuRes {
    pub threads: f64,
    pub total_used: f64,
    pub apps: Vec<AppUse>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MemRes {
    pub size: f64,
    pub total_used: f64,
    pub apps: Vec<AppUse>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct StorageRes {
    pub size: f64,
    pub total_used: f64,
    pub available: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TempRes {
    pub warning: String,
    pub temperature: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Resources {
    pub cpu: CpuRes,
    pub memory: MemRes,
    pub storage: StorageRes,
    pub temperature: TempRes,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AppInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub state: String,
    pub progress: f64,
    pub update_available: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Overview {
    pub info: Option<SystemInfo>,
    pub resources: Option<Resources>,
    pub apps: Vec<AppInfo>,
    pub error: Option<String>,
    /// CPU and RAM in percent, oldest first (recorded by the server).
    #[serde(default)]
    pub history: Vec<[f64; 2]>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub size: Option<f64>,
    pub modified: Option<f64>,
}

impl FileEntry {
    pub fn is_dir(&self) -> bool {
        self.kind == "directory"
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DirListing {
    pub path: String,
    pub files: Vec<FileEntry>,
    pub total_files: f64,
    pub has_more: bool,
}

// ---------- nrzMonitor ----------

/// Docker container as reported by nrzMonitor (`docker.containers`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Container {
    pub name: String,
    pub app: String,
    pub service: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub health: String,
    /// Percent, 100 = one core.
    pub cpu: f64,
    pub mem: f64,
    pub mem_limit: f64,
    /// Bit/s.
    pub net_rx: f64,
    pub net_tx: f64,
    pub restarts: f64,
    pub oom_killed: bool,
    pub started: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Filesystem {
    pub mount: String,
    pub device: String,
    pub fstype: String,
    pub size: f64,
    pub used: f64,
    pub avail: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Probe {
    pub name: String,
    pub target: String,
    pub up: bool,
    pub pending: bool,
    pub latency_ms: Option<f64>,
    pub uptime_pct: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Sensor {
    pub chip_label: String,
    pub label: String,
    pub kind: String,
    pub value: Option<f64>,
    pub crit: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct MonEvent {
    /// Unix milliseconds.
    pub ts: f64,
    pub severity: String,
    pub title: String,
    pub subject: String,
}

/// Everything the activity window shows from nrzMonitor.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct MonitorView {
    pub containers: Vec<Container>,
    pub filesystems: Vec<Filesystem>,
    pub probes: Vec<Probe>,
    pub sensors: Vec<Sensor>,
    /// Active alerts (title).
    pub alerts: Vec<String>,
    pub events: Vec<MonEvent>,
}

/// Everything the home screen needs, written into the first HTML by the server (from its cache)
/// so the page is complete before the WASM bundle has loaded.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct InitData {
    pub overview: Option<Overview>,
    pub tiles: Option<Vec<crate::api::apps::AppTile>>,
    pub settings: Option<crate::api::settings::SettingsView>,
    pub monitor: Option<Result<MonitorView, String>>,
}
