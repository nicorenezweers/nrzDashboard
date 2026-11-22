//! Everything the dashboard shows is loaded in the background and kept in memory, so pages and
//! server functions answer instantly: umbrelOS (system, resources, apps), nrzMonitor, the App Store
//! list and the app icons. While someone uses the dashboard it refreshes every 3 s, otherwise every
//! 30 s – the first visit after a pause still shows data that is at most half a minute old and is
//! refreshed right away. CPU/RAM history is recorded here too, so charts are full when opened.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use tokio::sync::{Mutex as AsyncMutex, Notify};

use super::mcp;
use crate::api::home::{StoreApp, StoreDetails};
use crate::api::apps::AppTile;
use crate::model::{AppInfo, InitData, MonitorView, Overview, Resources, SystemInfo};

const ACTIVE: Duration = Duration::from_secs(3);
const IDLE: Duration = Duration::from_secs(30);
/// Points of CPU/RAM history (at 3 s: three minutes).
pub const HISTORY: usize = 60;

/// One cached value with single-flight refresh.
pub struct Slot<T> {
    val: Mutex<Option<(Instant, T)>>,
    busy: AsyncMutex<()>,
}

impl<T: Clone + Send + 'static> Slot<T> {
    fn new() -> Self {
        Self { val: Mutex::new(None), busy: AsyncMutex::new(()) }
    }

    pub fn peek(&self) -> Option<(Instant, T)> {
        self.val.lock().ok().and_then(|v| v.clone())
    }

    fn put(&self, v: T) {
        if let Ok(mut s) = self.val.lock() {
            *s = Some((Instant::now(), v));
        }
    }

    pub fn clear(&self) {
        if let Ok(mut s) = self.val.lock() {
            *s = None;
        }
    }

    /// Loads a fresh value; parallel callers wait for the same load.
    async fn refresh<Fut: Future<Output = T>>(&self, f: impl FnOnce() -> Fut) -> T {
        let started = Instant::now();
        let _guard = self.busy.lock().await;
        if let Some((t, v)) = self.peek() {
            if t >= started {
                return v;
            }
        }
        let v = f().await;
        self.put(v.clone());
        v
    }

    /// Cached value (refreshed in the background when older than `max_age`); waits only when
    /// nothing is cached yet.
    async fn get<Fut>(&'static self, max_age: Duration, f: fn() -> Fut) -> T
    where
        Fut: Future<Output = T> + Send + 'static,
    {
        match self.peek() {
            Some((t, v)) => {
                if t.elapsed() > max_age {
                    tokio::spawn(async move {
                        self.refresh(f).await;
                    });
                }
                v
            }
            None => self.refresh(f).await,
        }
    }
}

fn slot<T: Clone + Send + 'static>(cell: &'static OnceLock<Slot<T>>) -> &'static Slot<T> {
    cell.get_or_init(Slot::new)
}

static INFO: OnceLock<Slot<Result<SystemInfo, String>>> = OnceLock::new();
static RESOURCES: OnceLock<Slot<Result<Resources, String>>> = OnceLock::new();
static APPS: OnceLock<Slot<Result<Vec<AppInfo>, String>>> = OnceLock::new();
static MONITOR: OnceLock<Slot<Result<MonitorView, String>>> = OnceLock::new();
static TILES: OnceLock<Slot<Result<Vec<AppTile>, String>>> = OnceLock::new();
static STORE: OnceLock<Slot<Result<Vec<StoreApp>, String>>> = OnceLock::new();

fn history() -> &'static Mutex<VecDeque<[f64; 2]>> {
    static H: OnceLock<Mutex<VecDeque<[f64; 2]>>> = OnceLock::new();
    H.get_or_init(Default::default)
}

fn wake() -> &'static Notify {
    static N: OnceLock<Notify> = OnceLock::new();
    N.get_or_init(Notify::new)
}

static LAST_SEEN: AtomicU64 = AtomicU64::new(0);

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

/// Marks the dashboard as in use (faster background refresh).
fn seen() {
    let before = LAST_SEEN.swap(now_secs(), Ordering::Relaxed);
    if now_secs().saturating_sub(before) > IDLE.as_secs() {
        wake().notify_one();
    }
}

fn active() -> bool {
    now_secs().saturating_sub(LAST_SEEN.load(Ordering::Relaxed)) < 120
}

// ---------- Loaders ----------

async fn load_info() -> Result<SystemInfo, String> {
    mcp::call_as("get_system_info", json!({})).await
}

async fn load_resources() -> Result<Resources, String> {
    let r: Result<Resources, String> = mcp::call_as("get_system_resources", json!({})).await;
    if let Ok(r) = &r {
        let mem = if r.memory.size > 0.0 { r.memory.total_used / r.memory.size * 100.0 } else { 0.0 };
        if let Ok(mut h) = history().lock() {
            h.push_back([r.cpu.total_used.clamp(0.0, 100.0), mem.clamp(0.0, 100.0)]);
            while h.len() > HISTORY {
                h.pop_front();
            }
        }
    }
    r
}

async fn load_apps() -> Result<Vec<AppInfo>, String> {
    mcp::call_as("list_apps", json!({})).await
}

async fn load_monitor() -> Result<MonitorView, String> {
    super::monitor::view().await
}

async fn load_store() -> Result<Vec<StoreApp>, String> {
    mcp::call_as("search_app_store", json!({ "query": "", "limit": 50 })).await
}

// ---------- Public getters (instant after the first load) ----------

fn build_overview(info: Result<SystemInfo, String>, res: Result<Resources, String>, apps: Result<Vec<AppInfo>, String>) -> Overview {
    let error = [info.as_ref().err(), res.as_ref().err(), apps.as_ref().err()].into_iter().flatten().next().cloned();
    let history = history().lock().map(|h| h.iter().copied().collect()).unwrap_or_default();
    Overview { info: info.ok(), resources: res.ok(), apps: apps.unwrap_or_default(), error, history }
}

/// What is in memory right now (no waiting) – written into the first HTML of the home screen.
pub fn snapshot() -> InitData {
    seen();
    let overview = match (slot(&INFO).peek(), slot(&RESOURCES).peek(), slot(&APPS).peek()) {
        (Some((_, i)), Some((_, r)), Some((_, a))) => Some(build_overview(i, r, a)),
        _ => None,
    };
    InitData {
        overview,
        tiles: slot(&TILES).peek().and_then(|(_, t)| t.ok()),
        settings: Some(crate::api::settings::settings_view()),
        monitor: slot(&MONITOR).peek().map(|(_, m)| m),
    }
}

async fn load_tiles() -> Result<Vec<AppTile>, String> {
    super::apps::tiles().await
}

pub async fn tiles() -> Result<Vec<AppTile>, String> {
    seen();
    slot(&TILES).get(Duration::from_secs(10), load_tiles).await
}

/// Names, links or hidden apps changed: rebuild the tiles now.
pub async fn refresh_tiles() {
    let _ = slot(&TILES).refresh(load_tiles).await;
}

pub async fn overview() -> Overview {
    seen();
    let (info, res, apps) = tokio::join!(
        slot(&INFO).get(Duration::from_secs(60), load_info),
        slot(&RESOURCES).get(ACTIVE, load_resources),
        slot(&APPS).get(Duration::from_secs(10), load_apps),
    );
    build_overview(info, res, apps)
}

/// Installed apps without marking the dashboard as in use (background work).
pub async fn apps_quiet() -> Result<Vec<AppInfo>, String> {
    slot(&APPS).get(Duration::from_secs(10), load_apps).await
}


pub async fn monitor() -> Result<MonitorView, String> {
    seen();
    slot(&MONITOR).get(ACTIVE, load_monitor).await
}

pub async fn store() -> Result<Vec<StoreApp>, String> {
    seen();
    slot(&STORE).get(Duration::from_secs(600), load_store).await
}

/// After app actions and setting changes: reload right away (and once more shortly after,
/// when umbrelOS has finished the change).
pub fn kick() {
    if let Ok(mut c) = details().lock() {
        c.clear();
    }
    tokio::spawn(async {
        let _ = slot(&STORE).refresh(load_store).await;
    });
    tokio::spawn(async {
        for wait in [0u64, 2, 6] {
            tokio::time::sleep(Duration::from_secs(wait)).await;
            let _ = slot(&APPS).refresh(load_apps).await;
            let _ = slot(&TILES).refresh(load_tiles).await;
        }
    });
    wake().notify_one();
}

fn details() -> &'static Mutex<std::collections::HashMap<String, (Instant, StoreDetails)>> {
    static D: OnceLock<Mutex<std::collections::HashMap<String, (Instant, StoreDetails)>>> = OnceLock::new();
    D.get_or_init(Default::default)
}

async fn load_details(id: &str) -> Result<StoreDetails, String> {
    let d: StoreDetails = mcp::call_as("get_app_store_details", json!({ "appId": id })).await?;
    if let Ok(mut c) = details().lock() {
        c.insert(id.to_string(), (Instant::now(), d.clone()));
    }
    Ok(d)
}

/// App Store details (preloaded for all listed apps, kept for 30 minutes).
pub async fn store_details(id: &str) -> Result<StoreDetails, String> {
    seen();
    let hit = details().lock().ok().and_then(|c| c.get(id).cloned());
    match hit {
        Some((t, d)) if t.elapsed() < Duration::from_secs(1800) => Ok(d),
        _ => load_details(id).await,
    }
}

/// New nrzMonitor connection: drop the old result.
pub fn reset_monitor() {
    slot(&MONITOR).clear();
}

// ---------- Background loop ----------

/// Starts the background refresh (once per process).
pub fn start() {
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        tokio::spawn(async {
            let mut round = 0u64;
            loop {
                let started = Instant::now();
                let slow = round % 10 == 0;
                let _ = tokio::join!(
                    slot(&RESOURCES).refresh(load_resources),
                    slot(&MONITOR).refresh(load_monitor),
                    async {
                        if slow || active() {
                            let _ = slot(&APPS).refresh(load_apps).await;
                        }
                    },
                    async {
                        if round % 20 == 0 {
                            let _ = slot(&INFO).refresh(load_info).await;
                        }
                    },
                );
                if round % 100 == 0 || slot(&STORE).peek().is_none_or(|(_, s)| s.is_err()) {
                    let list = slot(&STORE).refresh(load_store).await.unwrap_or_default();
                    // Details in the background, one after the other (gentle on umbrelOS).
                    tokio::spawn(async move {
                        for app in list {
                            let fresh = details().lock().ok().and_then(|c| c.get(&app.id).map(|(t, _)| t.elapsed() < Duration::from_secs(1500))).unwrap_or(false);
                            if !fresh {
                                let _ = load_details(&app.id).await;
                            }
                        }
                    });
                }
                // Domains, ports and icons for the home screen and the App Store.
                if slow {
                    let mut ids: Vec<String> = slot(&APPS).peek().and_then(|(_, a)| a.ok()).unwrap_or_default().into_iter().map(|a| a.id).collect();
                    let _ = slot(&TILES).refresh(load_tiles).await;
                    ids.extend(slot(&STORE).peek().and_then(|(_, s)| s.ok()).unwrap_or_default().into_iter().map(|a| a.id));
                    super::apps::prefetch_icons(ids).await;
                }
                round += 1;
                let pause = if active() { ACTIVE } else { IDLE };
                let _ = tokio::time::timeout(pause.saturating_sub(started.elapsed()), wake().notified()).await;
            }
        });
    });
}
