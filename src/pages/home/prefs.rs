//! Personal settings of the home screen, stored in the browser (localStorage).

use serde::{Deserialize, Serialize};

#[cfg_attr(not(feature = "web"), allow(dead_code))]
const KEY: &str = "nrz.home.prefs";

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Prefs {
    /// Name in the greeting ("Guten Morgen, nrz.").
    pub name: String,
    /// "default" (built-in wallpaper), "none" (plain dark) or "custom".
    pub wallpaper: String,
    /// Image URL for `wallpaper == "custom"`.
    pub custom_url: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { name: "nrz".into(), wallpaper: "default".into(), custom_url: String::new() }
    }
}

impl Prefs {
    pub fn load() -> Self {
        #[cfg(feature = "web")]
        {
            web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item(KEY).ok().flatten())
                .and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or_default()
        }
        #[cfg(not(feature = "web"))]
        Self::default()
    }

    pub fn save(&self) {
        #[cfg(feature = "web")]
        if let (Some(s), Ok(v)) = (web_sys::window().and_then(|w| w.local_storage().ok().flatten()), serde_json::to_string(self)) {
            let _ = s.set_item(KEY, &v);
        }
    }
}

/// CSS background for the chosen wallpaper.
pub fn wallpaper_style(p: &Prefs) -> String {
    let url = match p.wallpaper.as_str() {
        "none" => return "background: #111;".into(),
        "custom" if p.custom_url.starts_with("http") || p.custom_url.starts_with('/') => p.custom_url.replace(['"', '\'', ')', '('], ""),
        _ => super::WALLPAPER.to_string(),
    };
    format!("background: #111 url('{url}') center / cover no-repeat;")
}
