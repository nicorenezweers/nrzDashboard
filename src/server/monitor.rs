//! Monitoring engine (Uptime-Kuma-like).
//!
//! * Definitions: `$DATA_DIR/monitors.json` (migrated once from the old `targets.json`).
//! * History: `$DATA_DIR/heartbeats/<id>.jsonl` (appended, trimmed to 30 days / [`MAX_BEATS`] at startup).
//! * One tokio task per active monitor; push monitors are fed by `/monitors/push/<token>` (public route).

mod calc;
pub mod notify;
mod probe;

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, Once, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::{config, tracing_warn};
use crate::api::monitor::{Heartbeat, Monitor, MonitorDetail, MonitorKind, MonitorSummary, Status};

/// Maximum heartbeats kept per monitor (memory and file).
const MAX_BEATS: usize = 45_000;
/// Keep 30 days of history.
const KEEP_SECS: u64 = 30 * 86_400;
const MIN_INTERVAL: u32 = 20;
const MAX_INTERVAL: u32 = 86_400;
/// Beats per monitor in the overview.
const OVERVIEW_BEATS: usize = 50;
const HTTP_METHODS: [&str; 7] = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];

#[derive(Default)]
struct Runtime {
    beats: Vec<Heartbeat>,
    fails: u32,
    /// Status of the last beat.
    status: Status,
    /// Last confirmed up/down state and since when.
    important: Option<(Status, u64)>,
    cert_expiry: Option<u64>,
    last_push: Option<u64>,
    file_lines: usize,
    task: Option<tokio::task::AbortHandle>,
}

#[derive(Default)]
struct State {
    monitors: Vec<Monitor>,
    rt: HashMap<String, Runtime>,
}

fn state() -> MutexGuard<'static, State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State::default()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------- Files ----------

fn monitors_file() -> PathBuf {
    config().data_dir.join("monitors.json")
}

fn beats_file(id: &str) -> PathBuf {
    config()
        .data_dir
        .join("heartbeats")
        .join(format!("{id}.jsonl"))
}

/// Writes atomically (temp file + rename) with mode 0600.
pub(crate) fn write_private(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let res = opts
        .open(&tmp)
        .and_then(|mut f| f.write_all(data))
        .and_then(|()| std::fs::rename(&tmp, path));
    res.map_err(|e| format!("Konnte {} nicht schreiben: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn save_monitors(list: &[Monitor]) {
    match serde_json::to_string_pretty(list) {
        Ok(json) => {
            if let Err(e) = write_private(&monitors_file(), json.as_bytes()) {
                tracing_warn(&e);
            }
        }
        Err(e) => tracing_warn(&e.to_string()),
    }
}

fn append_beat(id: &str, beat: &Heartbeat) {
    let path = beats_file(id);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(line) = serde_json::to_string(beat) else {
        return;
    };
    let res = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(format!("{line}\n").as_bytes()));
    if let Err(e) = res {
        tracing_warn(&format!("Konnte {} nicht schreiben: {e}", path.display()));
    }
}

fn rewrite_beats(id: &str, beats: &[Heartbeat]) {
    let mut out = String::with_capacity(beats.len() * 48);
    for b in beats {
        if let Ok(line) = serde_json::to_string(b) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    if let Err(e) = write_private(&beats_file(id), out.as_bytes()) {
        tracing_warn(&e);
    }
}

/// Reads a history file, drops old/broken lines; returns (beats, lines in file).
fn load_beats(id: &str, now: u64) -> (Vec<Heartbeat>, usize) {
    let Ok(text) = std::fs::read_to_string(beats_file(id)) else {
        return (Vec::new(), 0);
    };
    let lines = text.lines().count();
    (
        trim_beats(
            text.lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect(),
            now,
        ),
        lines,
    )
}

fn trim_beats(mut beats: Vec<Heartbeat>, now: u64) -> Vec<Heartbeat> {
    beats.sort_by_key(|b| b.ts);
    let cutoff = now.saturating_sub(KEEP_SECS);
    let mut start = beats.partition_point(|b| b.ts < cutoff);
    start = start.max(beats.len().saturating_sub(MAX_BEATS));
    beats.drain(..start);
    beats
}

// ---------- Definitions ----------

#[derive(serde::Deserialize)]
struct OldTarget {
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    target: String,
}

fn default_monitors() -> Vec<Monitor> {
    let umbrel = super::settings::umbrel().url;
    vec![
        Monitor {
            name: "Umbrel".into(),
            kind: MonitorKind::Http,
            target: umbrel,
            ..Monitor::default()
        },
        Monitor {
            name: "Cloudflare DNS".into(),
            kind: MonitorKind::Tcp,
            target: "1.1.1.1:53".into(),
            ..Monitor::default()
        },
        Monitor {
            name: "Google".into(),
            kind: MonitorKind::Ping,
            target: "8.8.8.8".into(),
            ..Monitor::default()
        },
    ]
}

/// Converts the old ping targets (`targets.json`) into monitors.
fn migrate_targets(json: &str) -> Option<Vec<Monitor>> {
    let old: Vec<OldTarget> = serde_json::from_str(json).ok()?;
    Some(
        old.into_iter()
            .map(|t| {
                let kind = match t.kind.as_str() {
                    "http" => MonitorKind::Http,
                    "icmp" => MonitorKind::Ping,
                    _ => MonitorKind::Tcp,
                };
                Monitor {
                    name: if t.name.is_empty() {
                        t.target.clone()
                    } else {
                        t.name
                    },
                    kind,
                    target: t.target,
                    // The old HTTP check accepted everything below 500.
                    accepted_codes: if kind == MonitorKind::Http {
                        "200-499".into()
                    } else {
                        "200-299".into()
                    },
                    ..Monitor::default()
                }
            })
            .collect(),
    )
}

fn load_monitors() -> Vec<Monitor> {
    if let Ok(text) = std::fs::read_to_string(monitors_file()) {
        match serde_json::from_str::<Vec<Monitor>>(&text) {
            Ok(list) => return list,
            Err(e) => tracing_warn(&format!(
                "monitors.json ist ungültig ({e}) – Standard-Monitore werden verwendet"
            )),
        }
    }
    let old = config().data_dir.join("targets.json");
    let mut list = std::fs::read_to_string(&old)
        .ok()
        .and_then(|s| migrate_targets(&s))
        .filter(|l| !l.is_empty())
        .unwrap_or_else(default_monitors);
    for m in &mut list {
        if let Err(e) = normalize(m) {
            tracing_warn(&format!("Monitor „{}“: {e}", m.name));
        }
        m.id = super::auth::random_token(6);
    }
    if old.exists() {
        let _ = std::fs::rename(&old, old.with_extension("json.migrated"));
    }
    save_monitors(&list);
    list
}

fn valid_host(h: &str) -> bool {
    !h.is_empty() && !h.starts_with('-') && !h.contains(char::is_whitespace) && !h.contains('/')
}

/// Validates and normalizes a monitor definition (trims, clamps intervals, fills defaults).
fn normalize(m: &mut Monitor) -> Result<(), String> {
    for f in [
        &mut m.name,
        &mut m.target,
        &mut m.keyword,
        &mut m.group,
        &mut m.description,
        &mut m.dns_expected,
    ] {
        *f = f.trim().to_string();
    }
    m.method = m.method.trim().to_ascii_uppercase();
    m.interval = m.interval.clamp(MIN_INTERVAL, MAX_INTERVAL);
    m.retry_interval = m.retry_interval.clamp(MIN_INTERVAL, MAX_INTERVAL);
    m.retries = m.retries.min(10);
    m.timeout = m.timeout.clamp(1, 120).min(m.interval);
    match m.kind {
        MonitorKind::Http => {
            if !(m.target.starts_with("http://") || m.target.starts_with("https://")) {
                return Err("URL muss mit http:// oder https:// beginnen".into());
            }
            if m.method.is_empty() {
                m.method = "GET".into();
            }
            if !HTTP_METHODS.contains(&m.method.as_str()) {
                return Err("Ungültige HTTP-Methode".into());
            }
            calc::parse_codes(&m.accepted_codes)?;
        }
        MonitorKind::Tcp => {
            let (host, port) = m
                .target
                .rsplit_once(':')
                .ok_or("Format host:port erwartet")?;
            let host = host.trim_start_matches('[').trim_end_matches(']');
            if !valid_host(host) || port.parse::<u16>().map_or(true, |p| p == 0) {
                return Err("Format host:port erwartet".into());
            }
        }
        MonitorKind::Ping => {
            if !valid_host(&m.target) {
                return Err("Ungültiger Host".into());
            }
        }
        MonitorKind::Dns => {
            if !valid_host(&m.target) {
                return Err("Ungültiger Hostname".into());
            }
            if !matches!(m.dns_record.as_str(), "" | "A" | "AAAA") {
                return Err("Eintragstyp muss A oder AAAA sein".into());
            }
            if !m.dns_expected.is_empty() && m.dns_expected.parse::<std::net::IpAddr>().is_err() {
                return Err("Erwartete IP ist ungültig".into());
            }
        }
        MonitorKind::App => {
            if m.target.is_empty()
                || !m
                    .target
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err("Ungültige App-ID".into());
            }
        }
        MonitorKind::Push => {
            m.target.clear();
            if m.push_token.len() < 16 {
                m.push_token = super::auth::random_token(16);
            }
        }
    }
    if m.name.is_empty() {
        m.name = if m.target.is_empty() {
            m.kind.label().to_string()
        } else {
            m.target.clone()
        };
    }
    Ok(())
}

// ---------- Scheduler ----------

pub fn routes() -> axum::Router {
    use axum::routing::get;
    axum::Router::new().route(
        "/monitors/push/{token}",
        get(push_handler).post(push_handler),
    )
}

/// Loads monitors and history and starts one task per active monitor (called once at startup).
pub fn start() {
    static START: Once = Once::new();
    START.call_once(|| {
        let monitors = load_monitors();
        let t = now();
        let mut st = state();
        for m in &monitors {
            let (beats, lines) = load_beats(&m.id, t);
            if beats.len() != lines {
                rewrite_beats(&m.id, &beats);
            }
            let rt = runtime_from(beats);
            st.rt.insert(m.id.clone(), rt);
        }
        st.monitors = monitors;
        let ids: Vec<String> = st
            .monitors
            .iter()
            .filter(|m| !m.paused)
            .map(|m| m.id.clone())
            .collect();
        for id in ids {
            spawn(&mut st, &id);
        }
    });
}

fn runtime_from(beats: Vec<Heartbeat>) -> Runtime {
    let status = beats.last().map_or(Status::Unknown, |b| b.status);
    let fails = beats
        .iter()
        .rev()
        .take_while(|b| b.status != Status::Up)
        .count() as u32;
    let important = calc::events(&beats, now(), 1)
        .first()
        .map(|e| (e.status, e.ts));
    let file_lines = beats.len();
    Runtime {
        beats,
        fails,
        status,
        important,
        file_lines,
        ..Runtime::default()
    }
}

/// (Re)starts the task of monitor `id`.
fn spawn(st: &mut State, id: &str) {
    let rt = st.rt.entry(id.to_string()).or_default();
    if let Some(t) = rt.task.take() {
        t.abort();
    }
    // Push monitors get one interval of grace after (re)start.
    rt.last_push = Some(rt.last_push.unwrap_or(0).max(now()));
    let id = id.to_string();
    let handle = tokio::spawn(async move {
        // Spread the first checks a little.
        tokio::time::sleep(Duration::from_millis(rand::random::<u64>() % 3000)).await;
        loop {
            let Some(m) = definition(&id).filter(|m| !m.paused) else {
                break;
            };
            let wait = if m.kind == MonitorKind::Push {
                push_check(&m)
            } else {
                let res = probe::probe(&m).await;
                record(&m.id, res)
            };
            tokio::time::sleep(wait).await;
        }
    });
    rt.task = Some(handle.abort_handle());
}

fn definition(id: &str) -> Option<Monitor> {
    state().monitors.iter().find(|m| m.id == id).cloned()
}

fn push_check(m: &Monitor) -> Duration {
    let interval = u64::from(m.interval);
    let last = state().rt.get(&m.id).and_then(|r| r.last_push).unwrap_or(0);
    let age = now().saturating_sub(last);
    if age < interval {
        return Duration::from_secs(interval - age + 1);
    }
    record(&m.id, Err(format!("Kein Push innerhalb von {interval} s")))
}

/// Stores the result of a check; returns the delay until the next one.
fn record(id: &str, res: Result<probe::ProbeOk, String>) -> Duration {
    let ts = now();
    let mut st = state();
    let Some(m) = st.monitors.iter().find(|m| m.id == id).cloned() else {
        return Duration::from_secs(60);
    };
    let rt = st.rt.entry(id.to_string()).or_default();
    let (status, fails) = calc::next_status(rt.status, res.is_ok(), rt.fails, m.retries);
    let beat = match res {
        Ok(ok) => {
            if ok.cert_expiry.is_some() {
                rt.cert_expiry = ok.cert_expiry;
            }
            Heartbeat {
                ts,
                status,
                ms: Some(ok.ms),
                msg: ok.msg,
            }
        }
        Err(e) => {
            let msg = if status == Status::Pending {
                format!("{e} (Versuch {fails}/{})", m.retries + 1)
            } else {
                e
            };
            Heartbeat {
                ts,
                status,
                ms: None,
                msg,
            }
        }
    };
    rt.status = status;
    rt.fails = fails;

    // Status change up↔down → notification.
    let mut notify_prev: Option<Option<u64>> = None;
    if matches!(status, Status::Up | Status::Down) && rt.important.map(|i| i.0) != Some(status) {
        if let Some((_, since)) = rt.important {
            notify_prev = Some(Some(since));
        }
        rt.important = Some((status, ts));
    }

    rt.beats.push(beat.clone());
    rt.file_lines += 1;
    let cutoff = ts.saturating_sub(KEEP_SECS);
    if rt.beats.len() > MAX_BEATS + 500 || rt.beats.first().is_some_and(|b| b.ts + 3600 < cutoff) {
        rt.beats = trim_beats(std::mem::take(&mut rt.beats), ts);
    }
    let compact = rt.file_lines > MAX_BEATS + 5000;
    if compact {
        rt.file_lines = rt.beats.len();
        rewrite_beats(id, &rt.beats);
    }
    let next = if status == Status::Pending {
        m.retry_interval
    } else {
        m.interval
    };
    drop(st);

    if !compact {
        append_beat(id, &beat);
    }
    if let Some(prev) = notify_prev
        && m.notify
    {
        notify::status_changed(&m, &beat, prev);
    }
    Duration::from_secs(u64::from(next))
}

async fn push_handler(
    axum::extract::Path(token): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let found = {
        let st = state();
        st.monitors
            .iter()
            .find(|m| {
                m.kind == MonitorKind::Push && !m.push_token.is_empty() && m.push_token == token
            })
            .map(|m| (m.id.clone(), m.paused))
    };
    let Some((id, paused)) = found else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(json!({ "ok": false, "msg": "Monitor nicht gefunden" })),
        )
            .into_response();
    };
    if paused {
        return axum::Json(json!({ "ok": false, "msg": "Monitor ist pausiert" })).into_response();
    }
    if let Some(rt) = state().rt.get_mut(&id) {
        rt.last_push = Some(now());
    }
    let msg: String = q
        .get("msg")
        .map(|s| s.chars().take(200).collect())
        .unwrap_or_default();
    let down = q
        .get("status")
        .is_some_and(|s| s.eq_ignore_ascii_case("down"));
    let res = if down {
        Err(if msg.is_empty() {
            "Push meldet Fehler".into()
        } else {
            msg
        })
    } else {
        let ms = q
            .get("ping")
            .and_then(|p| p.parse::<f64>().ok())
            .filter(|p| p.is_finite() && *p >= 0.0);
        Ok(probe::ProbeOk {
            ms: ms.unwrap_or(0.0),
            msg: if msg.is_empty() {
                "Push empfangen".into()
            } else {
                msg
            },
            cert_expiry: None,
        })
    };
    record(&id, res);
    axum::Json(json!({ "ok": true })).into_response()
}

// ---------- API ----------

fn current_status(m: &Monitor, rt: Option<&Runtime>) -> Status {
    if m.paused {
        Status::Paused
    } else {
        rt.map_or(Status::Unknown, |r| r.status)
    }
}

pub fn overview() -> Vec<MonitorSummary> {
    let st = state();
    let day = now().saturating_sub(86_400);
    st.monitors
        .iter()
        .map(|m| {
            let rt = st.rt.get(&m.id);
            let beats = rt.map(|r| r.beats.as_slice()).unwrap_or_default();
            let last = beats.last();
            MonitorSummary {
                id: m.id.clone(),
                name: m.name.clone(),
                kind: m.kind,
                target: m.target_display(),
                group: m.group.clone(),
                status: current_status(m, rt),
                beats: beats
                    .get(beats.len().saturating_sub(OVERVIEW_BEATS)..)
                    .unwrap_or_default()
                    .to_vec(),
                uptime_24h: calc::uptime(beats, day),
                latency_ms: last.and_then(|b| b.ms),
                avg_24h: calc::avg_ms(beats, day),
                msg: last.map(|b| b.msg.clone()).unwrap_or_default(),
            }
        })
        .collect()
}

pub fn detail(id: &str, range_secs: u64) -> Option<MonitorDetail> {
    let st = state();
    let m = st.monitors.iter().find(|m| m.id == id)?;
    let rt = st.rt.get(id);
    let beats = rt.map(|r| r.beats.as_slice()).unwrap_or_default();
    let t = now();
    let range = range_secs.clamp(3600, KEEP_SECS);
    let day = t.saturating_sub(86_400);
    Some(MonitorDetail {
        monitor: m.clone(),
        status: current_status(m, rt),
        beats: beats
            .get(beats.len().saturating_sub(100)..)
            .unwrap_or_default()
            .to_vec(),
        latency_ms: beats.last().and_then(|b| b.ms),
        avg_24h: calc::avg_ms(beats, day),
        uptime_24h: calc::uptime(beats, day),
        uptime_7d: calc::uptime(beats, t.saturating_sub(7 * 86_400)),
        uptime_30d: calc::uptime(beats, t.saturating_sub(KEEP_SECS)),
        cert_expiry: rt.and_then(|r| r.cert_expiry),
        chart: calc::chart(beats, t.saturating_sub(range), t, 240),
        events: calc::events(beats, t, 100),
        now: t,
    })
}

pub fn save(mut m: Monitor) -> Result<String, String> {
    normalize(&mut m)?;
    let mut st = state();
    if m.id.is_empty() {
        m.id = super::auth::random_token(6);
        st.monitors.push(m.clone());
    } else {
        let slot = st
            .monitors
            .iter_mut()
            .find(|x| x.id == m.id)
            .ok_or("Monitor nicht gefunden")?;
        if slot.kind == MonitorKind::Push && m.kind == MonitorKind::Push && m.push_token.is_empty()
        {
            m.push_token.clone_from(&slot.push_token);
        }
        *slot = m.clone();
    }
    save_monitors(&st.monitors);
    if m.paused {
        if let Some(t) = st.rt.get_mut(&m.id).and_then(|r| r.task.take()) {
            t.abort();
        }
    } else {
        // Re-check right away with fresh retry state.
        if let Some(rt) = st.rt.get_mut(&m.id) {
            rt.fails = 0;
        }
        spawn(&mut st, &m.id);
    }
    Ok(m.id)
}

pub fn delete(id: &str) -> Result<(), String> {
    let mut st = state();
    let before = st.monitors.len();
    st.monitors.retain(|m| m.id != id);
    if st.monitors.len() == before {
        return Err("Monitor nicht gefunden".into());
    }
    if let Some(t) = st.rt.remove(id).and_then(|r| r.task) {
        t.abort();
    }
    save_monitors(&st.monitors);
    drop(st);
    let _ = std::fs::remove_file(beats_file(id));
    Ok(())
}

pub fn set_paused(id: &str, paused: bool) -> Result<(), String> {
    let mut st = state();
    let m = st
        .monitors
        .iter_mut()
        .find(|m| m.id == id)
        .ok_or("Monitor nicht gefunden")?;
    m.paused = paused;
    save_monitors(&st.monitors);
    if paused {
        if let Some(t) = st.rt.get_mut(id).and_then(|r| r.task.take()) {
            t.abort();
        }
    } else {
        if let Some(rt) = st.rt.get_mut(id) {
            rt.fails = 0;
            rt.last_push = None;
        }
        spawn(&mut st, id);
    }
    Ok(())
}

pub async fn app_choices() -> Result<Vec<(String, String)>, String> {
    let apps: Vec<crate::model::AppInfo> = super::mcp::call_as("list_apps", json!({})).await?;
    let mut out: Vec<(String, String)> = apps.into_iter().map(|a| (a.id, a.name)).collect();
    out.sort_by_key(|(_, n)| n.to_lowercase());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(kind: MonitorKind, target: &str) -> Monitor {
        Monitor {
            kind,
            target: target.into(),
            ..Monitor::default()
        }
    }

    #[test]
    fn normalize_validates_targets() {
        assert!(normalize(&mut mon(MonitorKind::Http, "https://example.com")).is_ok());
        assert!(normalize(&mut mon(MonitorKind::Http, "example.com")).is_err());
        assert!(normalize(&mut mon(MonitorKind::Tcp, "1.1.1.1:53")).is_ok());
        assert!(normalize(&mut mon(MonitorKind::Tcp, "[::1]:22")).is_ok());
        assert!(normalize(&mut mon(MonitorKind::Tcp, "1.1.1.1")).is_err());
        assert!(normalize(&mut mon(MonitorKind::Tcp, "host:0")).is_err());
        assert!(normalize(&mut mon(MonitorKind::Ping, "-c 9 x")).is_err());
        assert!(normalize(&mut mon(MonitorKind::App, "nextcloud")).is_ok());
        assert!(normalize(&mut mon(MonitorKind::App, "../x")).is_err());
        let mut dns = mon(MonitorKind::Dns, "example.com");
        dns.dns_expected = "1.2.3.x".into();
        assert!(normalize(&mut dns).is_err());
        let mut bad = mon(MonitorKind::Http, "https://x");
        bad.accepted_codes = "foo".into();
        assert!(normalize(&mut bad).is_err());
    }

    #[test]
    fn normalize_clamps_and_fills() {
        let mut m = mon(MonitorKind::Push, "ignored");
        m.interval = 1;
        m.timeout = 500;
        m.retries = 99;
        normalize(&mut m).unwrap();
        assert_eq!(
            (m.interval, m.timeout, m.retries),
            (MIN_INTERVAL, MIN_INTERVAL, 10)
        );
        assert!(m.target.is_empty());
        assert_eq!(m.push_token.len(), 32);
        assert_eq!(m.name, "Push");
        let mut h = mon(MonitorKind::Http, " https://a.b ");
        h.method = "post".into();
        normalize(&mut h).unwrap();
        assert_eq!(
            (h.method.as_str(), h.name.as_str()),
            ("POST", "https://a.b")
        );
    }

    #[test]
    fn migrates_old_targets() {
        let json = r#"[{"id":"x","name":"Umbrel","kind":"http","target":"http://umbrel.local"},
                       {"id":"y","name":"","kind":"icmp","target":"8.8.8.8"},
                       {"id":"z","name":"DNS","kind":"tcp","target":"1.1.1.1:53"}]"#;
        let list = migrate_targets(json).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(
            (list[0].kind, list[0].accepted_codes.as_str()),
            (MonitorKind::Http, "200-499")
        );
        assert_eq!(
            (list[1].kind, list[1].name.as_str()),
            (MonitorKind::Ping, "8.8.8.8")
        );
        assert_eq!(list[2].kind, MonitorKind::Tcp);
        assert!(migrate_targets("nope").is_none());
    }

    #[test]
    fn trims_history() {
        let t = 100 * 86_400;
        let beats: Vec<Heartbeat> = [t - 31 * 86_400, t - 10, t - 20, t - 29 * 86_400]
            .into_iter()
            .map(|ts| Heartbeat {
                ts,
                status: Status::Up,
                ms: None,
                msg: String::new(),
            })
            .collect();
        let kept = trim_beats(beats, t);
        assert_eq!(
            kept.iter().map(|b| b.ts).collect::<Vec<_>>(),
            vec![t - 29 * 86_400, t - 20, t - 10]
        );
    }

    #[test]
    fn runtime_restores_state() {
        let b = |ts, status| Heartbeat {
            ts,
            status,
            ms: None,
            msg: String::new(),
        };
        let rt = runtime_from(vec![
            b(1, Status::Up),
            b(2, Status::Down),
            b(3, Status::Down),
        ]);
        assert_eq!((rt.status, rt.fails), (Status::Down, 2));
        assert_eq!(rt.important, Some((Status::Down, 2)));
    }
}
