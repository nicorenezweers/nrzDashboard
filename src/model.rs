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
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct AppDetails {
    pub url: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub cpu: f64,
    pub memory: f64,
    pub disk: f64,
    pub tagline: String,
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

/// Tools that delete data or reboot – the UI asks twice.
#[cfg_attr(not(feature = "server"), allow(dead_code))]
pub const DESTRUCTIVE_TOOLS: &[&str] = &[
    "uninstall_app",
    "reset_app_data",
    "move_app_data",
    "trash",
    "install_os_update",
    "restart_device",
    "set_hostname",
    "set_release_channel",
    "delete_machine",
    "force_stop_machine",
    "stop_machine",
    "stop_app",
];

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct UmbrelSettings {
    pub url: String,
    /// Masked token (`umbrelmcp_54a6…18e1`), `None` when no token is set.
    pub token_hint: Option<String>,
}
