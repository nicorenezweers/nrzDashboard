//! Shared building blocks of the monitor pages (also used by the board).

use dioxus::prelude::*;
use icons::{Boxes, Globe, HeartPulse, Network, Radio, Server, X};

use crate::api::monitor::{Heartbeat, MonitorKind, Status};

/// Heartbeat bar: the last `max` beats as narrow colored bars (green/red/yellow/grey), oldest left.
/// Missing beats are drawn as empty slots. Tooltip: time, response time, message.
#[component]
pub fn HeartbeatBar(
    beats: Vec<Heartbeat>,
    #[props(default = 30)] max: usize,
    /// Extra classes, e.g. the height (`h-5` default).
    #[props(into, optional)]
    class: Option<String>,
) -> Element {
    let max = max.max(1);
    let shown = beats
        .get(beats.len().saturating_sub(max)..)
        .unwrap_or_default();
    let empty = max - shown.len();
    let class = class.unwrap_or_else(|| "h-5".into());
    rsx! {
        div { class: "flex w-full min-w-0 items-stretch gap-[2px] {class}",
            for _ in 0..empty {
                div { class: "min-w-0 flex-1 rounded-[2px] bg-muted/50" }
            }
            for b in shown.iter() {
                div {
                    class: "min-w-0 flex-1 rounded-[2px] {b.status.bg()} transition-opacity hover:opacity-70",
                    title: beat_title(b),
                }
            }
        }
    }
}

pub fn beat_title(b: &Heartbeat) -> String {
    let mut s = format!("{} · {}", fmt_ts(b.ts), b.status.label());
    if let Some(ms) = b.ms {
        s.push_str(&format!(" · {}", fmt_ms(ms)));
    }
    if !b.msg.is_empty() {
        s.push_str(&format!("\n{}", b.msg));
    }
    s
}

/// Colored status dot.
#[component]
pub fn StatusPip(status: Status) -> Element {
    let ring = match status {
        Status::Up => "shadow-[0_0_0_3px] shadow-success/20",
        Status::Down => "shadow-[0_0_0_3px] shadow-destructive/25 animate-pulse",
        Status::Pending => "shadow-[0_0_0_3px] shadow-warning/20",
        _ => "",
    };
    rsx! { span { class: "inline-block size-2.5 shrink-0 rounded-full {status.bg()} {ring}" } }
}

/// Status pill (`Up`, `Down` …).
#[component]
pub fn StatusPill(status: Status) -> Element {
    let class = match status {
        Status::Up => "bg-success/15 text-success",
        Status::Down => "bg-destructive/15 text-destructive",
        Status::Pending => "bg-warning/15 text-warning",
        _ => "bg-muted text-muted-foreground",
    };
    rsx! {
        span { class: "inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-semibold {class}",
            "{status.label()}"
        }
    }
}

/// Uptime percentage pill, colored by value.
#[component]
pub fn UptimePill(pct: Option<f64>, status: Status) -> Element {
    let class = match (status, pct) {
        (Status::Paused, _) | (_, None) => "bg-muted text-muted-foreground",
        (_, Some(p)) if p >= 99.0 => "bg-success/15 text-success",
        (_, Some(p)) if p >= 90.0 => "bg-warning/15 text-warning",
        _ => "bg-destructive/15 text-destructive",
    };
    rsx! {
        span { class: "inline-flex w-14 shrink-0 justify-center rounded-full px-1.5 py-0.5 text-[11px] font-semibold tabular-nums {class}",
            "{fmt_pct(pct)}"
        }
    }
}

#[component]
pub fn KindIcon(
    kind: MonitorKind,
    #[props(into, default = "size-4".to_string())] class: String,
) -> Element {
    match kind {
        MonitorKind::Http => rsx! { Globe { class } },
        MonitorKind::Tcp => rsx! { Network { class } },
        MonitorKind::Ping => rsx! { Radio { class } },
        MonitorKind::Dns => rsx! { Server { class } },
        MonitorKind::App => rsx! { Boxes { class } },
        MonitorKind::Push => rsx! { HeartPulse { class } },
    }
}

/// Modal dialog (bottom sheet on phones).
#[component]
pub fn Dialog(title: String, onclose: EventHandler<()>, children: Element) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-end justify-center bg-background/80 backdrop-blur-sm sm:items-center sm:p-4",
            onclick: move |_| onclose.call(()),
            div {
                class: "flex max-h-[92dvh] w-full flex-col rounded-t-xl border bg-card text-card-foreground shadow-lg sm:max-w-2xl sm:rounded-xl",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between border-b px-4 py-3",
                    h2 { class: "font-semibold", "{title}" }
                    button {
                        class: "inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent",
                        "aria-label": "Schliessen",
                        onclick: move |_| onclose.call(()),
                        X { class: "size-4" }
                    }
                }
                div { class: "overflow-y-auto p-4 pb-[calc(1rem+env(safe-area-inset-bottom,0px))]", {children} }
            }
        }
    }
}

/// Labeled form field.
#[component]
pub fn Field(label: String, #[props(into, default)] hint: String, children: Element) -> Element {
    rsx! {
        label { class: "flex flex-col gap-1.5 text-sm",
            span { class: "font-medium", "{label}" }
            {children}
            if !hint.is_empty() {
                span { class: "text-xs text-muted-foreground", "{hint}" }
            }
        }
    }
}

/// Checkbox with label.
#[component]
pub fn Check(label: String, checked: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "flex cursor-pointer items-center gap-2 text-sm select-none",
            input {
                r#type: "checkbox",
                class: "size-4 accent-primary",
                checked,
                onchange: move |e| onchange.call(e.checked()),
            }
            span { "{label}" }
        }
    }
}

pub fn select_class() -> &'static str {
    "h-9 w-full rounded-md border border-input bg-background px-2 text-base md:text-sm text-foreground outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50"
}

pub fn fmt_ms(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.2} s", ms / 1000.0)
    } else {
        format!("{ms:.0} ms")
    }
}

pub fn fmt_ms_opt(ms: Option<f64>) -> String {
    ms.map_or_else(|| "–".into(), fmt_ms)
}

pub fn fmt_pct(p: Option<f64>) -> String {
    match p {
        None => "–".into(),
        Some(p) if p >= 99.995 => "100 %".into(),
        Some(p) if p >= 10.0 => format!("{p:.1} %"),
        Some(p) => format!("{p:.2} %"),
    }
}

/// Local date + time with seconds (`12.11. 14:03:05`).
pub fn fmt_ts(ts: u64) -> String {
    let (d, mo, _, h, mi, s) = local(ts);
    format!("{d:02}.{mo:02}. {h:02}:{mi:02}:{s:02}")
}

/// Local `HH:MM`, with date prefix when `with_date`.
pub fn fmt_clock(ts: u64, with_date: bool) -> String {
    let (d, mo, _, h, mi, _) = local(ts);
    if with_date {
        format!("{d:02}.{mo:02}. {h:02}:{mi:02}")
    } else {
        format!("{h:02}:{mi:02}")
    }
}

/// (day, month, year, hour, minute, second) in local time (UTC during server rendering).
fn local(ts: u64) -> (u32, u32, i64, u32, u32, u32) {
    #[cfg(feature = "web")]
    {
        let d = js_sys::Date::new(&js_sys::wasm_bindgen::JsValue::from_f64(ts as f64 * 1000.0));
        (
            d.get_date(),
            d.get_month() + 1,
            i64::from(d.get_full_year()),
            d.get_hours(),
            d.get_minutes(),
            d.get_seconds(),
        )
    }
    #[cfg(not(feature = "web"))]
    {
        let days = (ts / 86_400) as i64;
        let rem = ts % 86_400;
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let y = yoe + era * 400 + i64::from(m <= 2);
        (
            d,
            m,
            y,
            (rem / 3600) as u32,
            ((rem % 3600) / 60) as u32,
            (rem % 60) as u32,
        )
    }
}

/// Origin of the page (`https://host:port`) for push URLs.
pub fn origin() -> String {
    #[cfg(feature = "web")]
    {
        web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_default()
    }
    #[cfg(not(feature = "web"))]
    {
        String::new()
    }
}

pub fn copy_to_clipboard(text: &str) {
    #[cfg(feature = "web")]
    if let Some(w) = web_sys::window() {
        let _ = w.navigator().clipboard().write_text(text);
    }
    #[cfg(not(feature = "web"))]
    let _ = text;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        assert_eq!(fmt_pct(Some(100.0)), "100 %");
        assert_eq!(fmt_pct(Some(99.5)), "99.5 %");
        assert_eq!(fmt_pct(Some(5.123)), "5.12 %");
        assert_eq!(fmt_pct(None), "–");
        assert_eq!(fmt_ms(12.4), "12 ms");
        assert_eq!(fmt_ms(1500.0), "1.50 s");
        assert_eq!(fmt_ts(1_794_441_600 + 3661), "12.11. 01:01:01");
    }
}
