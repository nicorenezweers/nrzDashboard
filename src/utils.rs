//! Formatting helpers and small client utilities.

use dioxus::prelude::ServerFnError;

pub fn fmt_bytes(b: f64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = b.max(0.0);
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{v:.0} {}", UNITS[0])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

pub fn fmt_uptime(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    let (d, h, m) = (s / 86_400, (s % 86_400) / 3600, (s % 3600) / 60);
    if d > 0 {
        format!("{d} T {h} Std")
    } else if h > 0 {
        format!("{h} Std {m} Min")
    } else {
        format!("{m} Min")
    }
}

pub fn pct(used: f64, total: f64) -> f64 {
    if total <= 0.0 {
        0.0
    } else {
        (used / total * 100.0).clamp(0.0, 100.0)
    }
}

/// Formats a unix timestamp in milliseconds as local `TT.MM.JJJJ HH:MM`.
pub fn fmt_time_ms(ms: f64) -> String {
    #[cfg(feature = "web")]
    {
        let d = js_sys::Date::new(&wasm_bindgen_value(ms));
        format!(
            "{:02}.{:02}.{} {:02}:{:02}",
            d.get_date(),
            d.get_month() + 1,
            d.get_full_year(),
            d.get_hours(),
            d.get_minutes()
        )
    }
    #[cfg(not(feature = "web"))]
    {
        // UTC fallback (server render).
        let secs = (ms / 1000.0) as i64;
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        format!(
            "{d:02}.{m:02}.{y} {:02}:{:02}",
            rem / 3600,
            (rem % 3600) / 60
        )
    }
}

#[cfg(feature = "web")]
fn wasm_bindgen_value(ms: f64) -> js_sys::wasm_bindgen::JsValue {
    js_sys::wasm_bindgen::JsValue::from_f64(ms)
}

#[cfg(not(feature = "web"))]
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Sleeps on the client; never resolves on the server so polling loops don't run during SSR.
pub async fn sleep_ms(ms: u32) {
    #[cfg(feature = "web")]
    gloo_timers::future::TimeoutFuture::new(ms).await;
    #[cfg(not(feature = "web"))]
    {
        let _ = ms;
        std::future::pending::<()>().await;
    }
}

/// Turns a server function error into a message; a lost session sends the user to the login page.
pub fn err_msg(e: &ServerFnError) -> String {
    let text = e.to_string();
    if text.contains("401") || text.contains("Unauthorized") {
        #[cfg(feature = "web")]
        if let Some(w) = web_sys::window() {
            let _ = w.location().set_href("/login");
        }
        return "Sitzung abgelaufen – bitte neu anmelden".into();
    }
    text.trim_start_matches("error running server function:")
        .replace(" (details: None)", "")
        .trim()
        .to_string()
}

pub fn enc(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

/// Markdown → HTML; raw HTML is shown as text, links open in a new tab.
pub fn markdown(text: &str) -> String {
    use pulldown_cmark::{Event, Options, Parser, html};
    let parser = Parser::new_ext(
        text,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES,
    )
    .map(|e| match e {
        Event::Html(s) | Event::InlineHtml(s) => Event::Text(s),
        other => other,
    });
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out.replace("<a href=\"javascript:", "<a data-blocked=\"")
        .replace(
            "<a href=",
            "<a target=\"_blank\" rel=\"noopener noreferrer\" href=",
        )
}
