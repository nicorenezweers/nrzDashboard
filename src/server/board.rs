//! Boards: persistence in `$DATA_DIR/boards.json`, app tiles, URL pings, weather (Open-Meteo).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::api::board::{AppOverride, AppTile, Board, BoardMeta, Place, Weather, Widget, WidgetKind};
use crate::model::AppInfo;

use super::{config, http, mcp, settings, tracing_warn};

/// Extra HTTP routes of this feature (merged into the main router, behind the auth middleware).
pub fn routes() -> axum::Router {
    axum::Router::new()
}

fn file() -> std::path::PathBuf {
    config().data_dir.join("boards.json")
}

fn store() -> &'static Mutex<Vec<Board>> {
    static S: OnceLock<Mutex<Vec<Board>>> = OnceLock::new();
    S.get_or_init(|| {
        let boards: Vec<Board> = std::fs::read_to_string(file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .filter(|b: &Vec<Board>| !b.is_empty())
            .unwrap_or_else(|| vec![default_board()]);
        Mutex::new(boards)
    })
}

fn persist(boards: &[Board]) -> Result<(), String> {
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::to_string_pretty(boards).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("Konnte {} nicht schreiben: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

fn new_id() -> String {
    super::auth::random_token(6)
}

fn widget(kind: WidgetKind, x: i32, y: i32, w: i32, h: i32, title: &str, options: Value) -> Widget {
    Widget { id: new_id(), kind, x, y, w, h, title: title.into(), options }
}

fn default_board() -> Board {
    Board {
        id: "home".into(),
        name: "Home".into(),
        widgets: vec![
            widget(WidgetKind::Search, 0, 0, 8, 1, "", json!({ "engine": "duckduckgo" })),
            widget(WidgetKind::Clock, 8, 0, 4, 2, "", json!({ "seconds": false, "date": true })),
            widget(WidgetKind::Apps, 0, 1, 8, 4, "Apps", json!({})),
            widget(WidgetKind::System, 8, 2, 4, 3, "System", json!({})),
            widget(WidgetKind::Monitors, 0, 5, 6, 4, "Monitore", json!({})),
            widget(
                WidgetKind::Weather,
                6,
                5,
                3,
                2,
                "",
                json!({ "city": "Zürich", "lat": 47.3769, "lon": 8.5417 }),
            ),
            widget(
                WidgetKind::Bookmarks,
                9,
                5,
                3,
                4,
                "Lesezeichen",
                json!({ "items": "nrzGit | http://umbrel.local:8390\numbrelOS | http://umbrel.local" }),
            ),
            widget(
                WidgetKind::Notes,
                6,
                7,
                3,
                2,
                "Notizen",
                json!({ "text": "**Willkommen!** Klick oben auf *Bearbeiten*, um Widgets hinzuzufügen, zu verschieben und ihre Grösse zu ändern." }),
            ),
        ],
    }
}

pub fn list() -> Vec<BoardMeta> {
    store()
        .lock()
        .map(|b| b.iter().map(|b| BoardMeta { id: b.id.clone(), name: b.name.clone() }).collect())
        .unwrap_or_default()
}

pub fn get(id: &str) -> Result<Board, String> {
    let boards = store().lock().map_err(|_| "Interner Fehler")?;
    boards
        .iter()
        .find(|b| b.id == id)
        .or_else(|| boards.first())
        .cloned()
        .ok_or_else(|| "Board nicht gefunden".into())
}

fn sanitize(mut board: Board) -> Board {
    board.name = board.name.trim().chars().take(60).collect();
    if board.name.is_empty() {
        board.name = "Board".into();
    }
    board.widgets.truncate(100);
    for w in &mut board.widgets {
        let (mw, mh) = w.kind.min_size();
        w.w = w.w.clamp(mw, crate::api::board::COLS);
        w.h = w.h.clamp(mh, 24);
        w.x = w.x.clamp(0, crate::api::board::COLS - w.w);
        w.y = w.y.clamp(0, 500);
    }
    board
}

pub fn save(board: Board) -> Result<(), String> {
    let board = sanitize(board);
    let mut boards = store().lock().map_err(|_| "Interner Fehler")?;
    match boards.iter_mut().find(|b| b.id == board.id) {
        Some(b) => *b = board,
        None => boards.push(board),
    }
    persist(&boards)
}

pub fn create(name: &str) -> Result<Board, String> {
    let board = sanitize(Board { id: new_id(), name: name.to_string(), widgets: Vec::new() });
    let mut boards = store().lock().map_err(|_| "Interner Fehler")?;
    boards.push(board.clone());
    persist(&boards)?;
    Ok(board)
}

pub fn delete(id: &str) -> Result<(), String> {
    let mut boards = store().lock().map_err(|_| "Interner Fehler")?;
    if boards.len() <= 1 {
        return Err("Das letzte Board kann nicht gelöscht werden".into());
    }
    boards.retain(|b| b.id != id);
    persist(&boards)
}

// ---------- App tiles ----------

/// App URLs rarely change: cache `get_app_details` per app for 10 minutes.
fn url_cache() -> &'static Mutex<HashMap<String, (Instant, Option<String>)>> {
    static C: OnceLock<Mutex<HashMap<String, (Instant, Option<String>)>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Cached URL, or `None` while it is still being fetched in the background (tiles show up at once;
/// the URLs arrive with the next poll).
fn app_url(id: &str) -> Option<String> {
    let cached = url_cache().lock().ok().and_then(|c| c.get(id).cloned());
    match cached {
        Some((t, url)) if t.elapsed() < Duration::from_secs(600) => url,
        stale => {
            if in_flight().lock().map(|mut f| f.insert(id.to_string())).unwrap_or(false) {
                let id = id.to_string();
                tokio::spawn(async move {
                    fetch_app_url(&id).await;
                    if let Ok(mut f) = in_flight().lock() {
                        f.remove(&id);
                    }
                });
            }
            stale.and_then(|(_, url)| url)
        }
    }
}

fn in_flight() -> &'static Mutex<std::collections::HashSet<String>> {
    static F: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

async fn fetch_app_url(id: &str) -> Option<String> {
    let url = mcp::call("get_app_details", json!({ "appId": id }))
        .await
        .ok()
        .and_then(|v| v.get("url").and_then(Value::as_str).map(str::to_string))
        .filter(|u| !u.is_empty());
    if let Ok(mut c) = url_cache().lock() {
        c.insert(id.to_string(), (Instant::now(), url.clone()));
    }
    url
}

pub async fn app_tiles() -> Result<Vec<AppTile>, String> {
    let apps: Vec<AppInfo> = mcp::call_as("list_apps", json!({})).await?;
    let overrides = overrides().lock().map(|o| o.clone()).unwrap_or_default();
    let nonempty = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
    let mut tiles: Vec<AppTile> = apps
        .into_iter()
        .map(|a| {
            let ov = overrides.get(&a.id).cloned().unwrap_or_default();
            let url = nonempty(&ov.url).or_else(|| app_url(&a.id));
            AppTile {
                name: nonempty(&ov.name).unwrap_or_else(|| a.name.clone()),
                original_name: a.name,
                url,
                icon: nonempty(&ov.icon),
                hidden: ov.hidden,
                state: a.state,
                update_available: a.update_available,
                id: a.id,
            }
        })
        .collect();
    tiles.sort_by_key(|t| t.name.to_lowercase());
    Ok(tiles)
}

// ---------- App overrides (`$DATA_DIR/app_overrides.json`) ----------

fn overrides_file() -> std::path::PathBuf {
    config().data_dir.join("app_overrides.json")
}

fn overrides() -> &'static Mutex<HashMap<String, AppOverride>> {
    static O: OnceLock<Mutex<HashMap<String, AppOverride>>> = OnceLock::new();
    O.get_or_init(|| {
        Mutex::new(
            std::fs::read_to_string(overrides_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
        )
    })
}

pub fn override_get(id: &str) -> AppOverride {
    overrides().lock().ok().and_then(|o| o.get(id).cloned()).unwrap_or_default()
}

pub fn override_set(id: &str, mut value: AppOverride) -> Result<(), String> {
    value.name = value.name.trim().chars().take(60).collect();
    value.icon = value.icon.trim().to_string();
    value.url = value.url.trim().to_string();
    for u in [&value.icon, &value.url] {
        if !u.is_empty() && !(u.starts_with("http://") || u.starts_with("https://")) {
            return Err("URLs müssen mit http:// oder https:// beginnen".into());
        }
    }
    let mut o = overrides().lock().map_err(|_| "Interner Fehler")?;
    if value.is_empty() {
        o.remove(id);
    } else {
        o.insert(id.to_string(), value);
    }
    let path = overrides_file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::to_string_pretty(&*o).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("Konnte {} nicht schreiben: {e}", path.display()))
}

// ---------- URL ping ----------

fn ping_cache() -> &'static Mutex<HashMap<String, (Instant, Option<f64>)>> {
    static C: OnceLock<Mutex<HashMap<String, (Instant, Option<f64>)>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// From inside the container `umbrel.local` does not resolve: use the configured Umbrel host.
fn server_side_url(url: &str) -> String {
    let base = settings::umbrel().url;
    let host = base.split("://").nth(1).unwrap_or(&base).split(['/', ':']).next().unwrap_or("");
    if host.is_empty() { url.to_string() } else { url.replace("umbrel.local", host) }
}

pub async fn ping_url(url: &str) -> Option<f64> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    if let Ok(c) = ping_cache().lock() {
        if let Some((t, v)) = c.get(url) {
            if t.elapsed() < Duration::from_secs(20) {
                return *v;
            }
        }
    }
    let start = Instant::now();
    let res = http().get(server_side_url(url)).timeout(Duration::from_secs(5)).send().await;
    let v = match res {
        Ok(r) if !r.status().is_server_error() => Some(start.elapsed().as_secs_f64() * 1000.0),
        _ => None,
    };
    if let Ok(mut c) = ping_cache().lock() {
        if c.len() > 500 {
            c.clear();
        }
        c.insert(url.to_string(), (Instant::now(), v));
    }
    v
}

// ---------- Weather (Open-Meteo, no API key) ----------

fn weather_cache() -> &'static Mutex<HashMap<String, (Instant, Weather)>> {
    static C: OnceLock<Mutex<HashMap<String, (Instant, Weather)>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

pub async fn weather(lat: f64, lon: f64) -> Result<Weather, String> {
    let key = format!("{lat:.3},{lon:.3}");
    if let Ok(c) = weather_cache().lock() {
        if let Some((t, w)) = c.get(&key) {
            if t.elapsed() < Duration::from_secs(600) {
                return Ok(w.clone());
            }
        }
    }
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day\
         &daily=weather_code,temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=6"
    );
    let v: Value = http()
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Wetterdienst nicht erreichbar: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let cur = v.get("current").cloned().unwrap_or(Value::Null);
    let f = |o: &Value, k: &str| o.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let daily = v.get("daily").cloned().unwrap_or(Value::Null);
    let arr = |k: &str| daily.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
    let (dates, codes, maxs, mins) = (arr("time"), arr("weather_code"), arr("temperature_2m_max"), arr("temperature_2m_min"));
    let days = dates
        .iter()
        .enumerate()
        .map(|(i, d)| {
            (
                d.as_str().unwrap_or_default().to_string(),
                codes.get(i).and_then(Value::as_i64).unwrap_or(0) as i32,
                maxs.get(i).and_then(Value::as_f64).unwrap_or(0.0),
                mins.get(i).and_then(Value::as_f64).unwrap_or(0.0),
            )
        })
        .collect();
    let w = Weather {
        temperature: f(&cur, "temperature_2m"),
        apparent: f(&cur, "apparent_temperature"),
        code: cur.get("weather_code").and_then(Value::as_i64).unwrap_or(0) as i32,
        wind: f(&cur, "wind_speed_10m"),
        humidity: f(&cur, "relative_humidity_2m"),
        is_day: cur.get("is_day").and_then(Value::as_i64).unwrap_or(1) == 1,
        daily: days,
    };
    if let Ok(mut c) = weather_cache().lock() {
        c.insert(key, (Instant::now(), w.clone()));
    }
    Ok(w)
}

pub async fn geocode(query: &str) -> Result<Vec<Place>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=6&language=de&format=json",
        urlencoding::encode(q)
    );
    let v: Value = http()
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| {
            tracing_warn(&format!("Geocoding: {e}"));
            format!("Ortssuche nicht erreichbar: {e}")
        })?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(v.get("results")
        .and_then(Value::as_array)
        .map(|r| {
            r.iter()
                .map(|p| Place {
                    name: p.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                    region: [p.get("admin1"), p.get("country")]
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", "),
                    lat: p.get("latitude").and_then(Value::as_f64).unwrap_or(0.0),
                    lon: p.get("longitude").and_then(Value::as_f64).unwrap_or(0.0),
                })
                .collect()
        })
        .unwrap_or_default())
}
