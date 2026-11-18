//! Detail view of one monitor: stats, big heartbeat bar, response-time chart, events.

use dioxus::prelude::*;
use icons::{ArrowLeft, Copy, Pause, Pencil, Play, Trash2};

use super::parts::{
    HeartbeatBar, KindIcon, StatusPill, copy_to_clipboard, fmt_clock, fmt_ms, fmt_ms_opt, fmt_pct,
    fmt_ts, origin,
};
use crate::api::monitor::{
    ChartPoint, Monitor, MonitorDetail, MonitorKind, delete_monitor, fmt_duration,
    set_monitor_paused,
};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::widgets::{ConfirmButton, ErrorBox};
use crate::utils::err_msg;

const RANGES: [(u64, &str); 4] = [
    (3600, "1 Std"),
    (3 * 3600, "3 Std"),
    (86_400, "24 Std"),
    (7 * 86_400, "7 Tage"),
];

#[component]
pub fn Detail(
    detail: MonitorDetail,
    range: Signal<u64>,
    onback: EventHandler<()>,
    onedit: EventHandler<Monitor>,
    onchanged: EventHandler<()>,
    ondeleted: EventHandler<()>,
) -> Element {
    let mut range = range;
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let d = detail;
    let m = d.monitor.clone();
    let id = m.id.clone();
    let paused = m.paused;
    let cert_days = d
        .cert_expiry
        .map(|t| (t as i64 - d.now as i64).div_euclid(86_400));
    let push_url = (m.kind == MonitorKind::Push)
        .then(|| format!("{}/monitors/push/{}", origin(), m.push_token));

    let toggle_pause = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            spawn(async move {
                busy.set(true);
                match set_monitor_paused(id, !paused).await {
                    Ok(()) => {
                        error.set(None);
                        onchanged.call(());
                    }
                    Err(e) => error.set(Some(err_msg(&e))),
                }
                busy.set(false);
            });
        }
    };
    let delete = {
        let id = id.clone();
        move |()| {
            let id = id.clone();
            spawn(async move {
                match delete_monitor(id).await {
                    Ok(()) => ondeleted.call(()),
                    Err(e) => error.set(Some(err_msg(&e))),
                }
            });
        }
    };
    let edit_monitor = m.clone();

    rsx! {
        div { class: "flex flex-col gap-4",
            Button {
                class: "lg:hidden -ml-2 self-start",
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Sm,
                onclick: move |_| onback.call(()),
                ArrowLeft { class: "size-4" }
                "Zurück"
            }

            // Header
            div { class: "flex flex-col gap-3 rounded-xl border bg-card p-4",
                div { class: "flex flex-wrap items-start justify-between gap-3",
                    div { class: "min-w-0",
                        div { class: "flex flex-wrap items-center gap-2",
                            h2 { class: "text-xl font-semibold tracking-tight break-words", "{m.name}" }
                            StatusPill { status: d.status }
                            if !m.group.is_empty() {
                                span { class: "rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground", "{m.group}" }
                            }
                        }
                        div { class: "mt-1 flex min-w-0 items-center gap-1.5 text-sm text-muted-foreground",
                            KindIcon { kind: m.kind, class: "size-3.5 shrink-0" }
                            span { class: "shrink-0", "{m.kind.label()}" }
                            if m.kind == MonitorKind::Http {
                                a {
                                    class: "min-w-0 truncate text-primary hover:underline",
                                    href: "{m.target}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    "{m.target_display()}"
                                }
                            } else if m.kind != MonitorKind::Push {
                                span { class: "min-w-0 truncate", "· {m.target_display()}" }
                            }
                        }
                        if !m.description.is_empty() {
                            p { class: "mt-2 text-sm text-muted-foreground whitespace-pre-line", "{m.description}" }
                        }
                    }
                    div { class: "flex flex-wrap gap-2",
                        Button {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            onclick: move |_| onedit.call(edit_monitor.clone()),
                            Pencil { class: "size-4" }
                            "Bearbeiten"
                        }
                        Button {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            disabled: busy(),
                            onclick: toggle_pause,
                            if paused {
                                Play { class: "size-4" }
                                "Fortsetzen"
                            } else {
                                Pause { class: "size-4" }
                                "Pausieren"
                            }
                        }
                        ConfirmButton { label: "Löschen", confirm_label: "Wirklich löschen?", onconfirm: delete,
                            Trash2 { class: "size-4" }
                        }
                    }
                }
                if let Some(e) = error() {
                    ErrorBox { message: e }
                }
                if let Some(url) = push_url {
                    PushInfo { url }
                }
                HeartbeatBar { beats: d.beats.clone(), max: 60, class: "h-8" }
                if let Some(b) = d.beats.last() {
                    p { class: "text-xs text-muted-foreground break-words",
                        "Zuletzt {fmt_ts(b.ts)}"
                        if !b.msg.is_empty() { " · {b.msg}" }
                    }
                }
            }

            // Stats
            div { class: "grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4",
                Stat { label: "Antwortzeit", sub: "aktuell", value: fmt_ms_opt(d.latency_ms) }
                Stat { label: "Ø Antwortzeit", sub: "24 Std", value: fmt_ms_opt(d.avg_24h) }
                Stat { label: "Uptime", sub: "24 Std · 7 Tage {fmt_pct(d.uptime_7d)}", value: fmt_pct(d.uptime_24h) }
                Stat { label: "Uptime", sub: "30 Tage", value: fmt_pct(d.uptime_30d) }
            }
            if let (Some(days), Some(exp)) = (cert_days, d.cert_expiry) {
                div { class: if days < 14 { "rounded-lg border border-warning/40 bg-warning/10 px-3 py-2 text-sm text-warning" } else { "rounded-lg border bg-card px-3 py-2 text-sm text-muted-foreground" },
                    if days < 0 {
                        "TLS-Zertifikat ist abgelaufen ({fmt_clock(exp, true)})"
                    } else {
                        "TLS-Zertifikat gültig bis {fmt_clock(exp, true)} – noch {days} Tage"
                    }
                }
            }

            // Chart
            div { class: "flex flex-col gap-3 rounded-xl border bg-card p-4",
                div { class: "flex flex-wrap items-center justify-between gap-2",
                    h3 { class: "font-medium", "Antwortzeit" }
                    div { class: "flex gap-1",
                        for (secs, label) in RANGES {
                            Button {
                                size: ButtonSize::Sm,
                                variant: if range() == secs { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                                onclick: move |_| range.set(secs),
                                "{label}"
                            }
                        }
                    }
                }
                ResponseChart { points: d.chart.clone(), since: d.now.saturating_sub(range()), now: d.now }
            }

            // Events
            div { class: "flex flex-col gap-3 rounded-xl border bg-card p-4",
                h3 { class: "font-medium", "Ereignisse" }
                if d.events.is_empty() {
                    p { class: "text-sm text-muted-foreground", "Noch keine Statuswechsel." }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "w-full text-sm",
                            thead {
                                tr { class: "border-b text-left text-xs text-muted-foreground",
                                    th { class: "py-2 pr-3 font-medium", "Status" }
                                    th { class: "py-2 pr-3 font-medium", "Zeit" }
                                    th { class: "py-2 pr-3 font-medium", "Dauer" }
                                    th { class: "py-2 font-medium", "Meldung" }
                                }
                            }
                            tbody {
                                for e in d.events.iter() {
                                    tr { class: "border-b last:border-0 align-top",
                                        td { class: "py-2 pr-3", StatusPill { status: e.status } }
                                        td { class: "py-2 pr-3 whitespace-nowrap tabular-nums", "{fmt_clock(e.ts, true)}" }
                                        td { class: "py-2 pr-3 whitespace-nowrap",
                                            "{fmt_duration(e.duration_secs)}"
                                            if e.ongoing { span { class: "text-muted-foreground", " (aktuell)" } }
                                        }
                                        td { class: "py-2 text-muted-foreground break-words", "{e.msg}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Stat(label: String, sub: String, value: String) -> Element {
    rsx! {
        div { class: "rounded-xl border bg-card px-4 py-3",
            div { class: "text-sm text-muted-foreground", "{label}" }
            div { class: "text-xs text-muted-foreground/70", "{sub}" }
            div { class: "mt-1 text-2xl font-semibold tabular-nums", "{value}" }
        }
    }
}

#[component]
fn PushInfo(url: String) -> Element {
    let mut copied = use_signal(|| false);
    let u = url.clone();
    rsx! {
        div { class: "flex flex-col gap-2 rounded-lg border bg-muted/30 p-3 text-sm",
            span { class: "text-muted-foreground",
                "Der Dienst muss diese URL innerhalb des Intervalls aufrufen (GET/POST, optional "
                code { "?status=down&msg=…&ping=…" }
                "):"
            }
            div { class: "flex items-center gap-2",
                code { class: "min-w-0 flex-1 truncate rounded bg-background px-2 py-1 text-xs", "{url}" }
                Button {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    onclick: move |_| {
                        copy_to_clipboard(&u);
                        copied.set(true);
                    },
                    Copy { class: "size-4" }
                    if copied() { "Kopiert" } else { "Kopieren" }
                }
            }
        }
    }
}

/// Response-time chart (SVG); failed sections are shaded red.
#[component]
pub fn ResponseChart(points: Vec<ChartPoint>, since: u64, now: u64) -> Element {
    const W: f64 = 600.0;
    const H: f64 = 160.0;
    if points.iter().all(|p| p.ms.is_none() && !p.down) {
        return rsx! {
            div { class: "flex h-40 items-center justify-center text-sm text-muted-foreground", "Keine Messwerte im Zeitraum" }
        };
    }
    let span = now.saturating_sub(since).max(1) as f64;
    let max_ms = points.iter().filter_map(|p| p.ms).fold(0.0_f64, f64::max);
    let top = nice_ceil(max_ms * 1.1);
    let x = |ts: u64| ((ts.saturating_sub(since)) as f64 / span * W).clamp(0.0, W);
    let y = |ms: f64| H - (ms / top) * (H - 4.0);
    // Width of one point slot for the red shading.
    let slot = (W / points.len().max(1) as f64).max(2.0);

    let mut segments: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut cur: Vec<(f64, f64)> = Vec::new();
    for p in &points {
        match p.ms {
            Some(ms) => cur.push((x(p.ts), y(ms))),
            None => {
                if !cur.is_empty() {
                    segments.push(std::mem::take(&mut cur));
                }
            }
        }
    }
    if !cur.is_empty() {
        segments.push(cur);
    }
    let lines: Vec<String> = segments
        .iter()
        .map(|s| {
            s.iter()
                .map(|(a, b)| format!("{a:.1},{b:.1}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    let areas: Vec<String> = segments
        .iter()
        .filter(|s| s.len() > 1)
        .map(|s| {
            let first = s.first().map_or(0.0, |p| p.0);
            let last = s.last().map_or(0.0, |p| p.0);
            let pts = s
                .iter()
                .map(|(a, b)| format!("L{a:.1},{b:.1}"))
                .collect::<String>();
            format!("M{first:.1},{H} {pts} L{last:.1},{H} Z")
        })
        .collect();
    let downs: Vec<f64> = points
        .iter()
        .filter(|p| p.down)
        .map(|p| x(p.ts) - slot / 2.0)
        .collect();
    let with_date = span > 86_400.0;
    let ticks: Vec<String> = (0..=4)
        .map(|i| fmt_clock(since + (span as u64) * i / 4, with_date))
        .collect();
    let singles: Vec<(f64, f64)> = segments
        .iter()
        .filter(|s| s.len() == 1)
        .filter_map(|s| s.first().copied())
        .collect();

    rsx! {
        div { class: "flex flex-col gap-1",
            div { class: "relative h-40 w-full",
                // Grid labels
                div { class: "pointer-events-none absolute inset-0 flex flex-col justify-between text-[10px] text-muted-foreground",
                    for i in 0..4 {
                        div { class: "flex items-start gap-1 border-t border-border/60",
                            span { class: "bg-card pr-1 tabular-nums", "{fmt_ms(top * f64::from(4 - i) / 4.0)}" }
                        }
                    }
                    div { class: "border-t border-border/60" }
                }
                svg {
                    class: "absolute inset-0 h-full w-full text-success",
                    view_box: "0 0 {W} {H}",
                    preserve_aspect_ratio: "none",
                    for dx in downs {
                        rect { x: "{dx:.1}", y: "0", width: "{slot:.1}", height: "{H}", fill: "var(--destructive)", opacity: "0.25" }
                    }
                    for a in areas {
                        path { d: "{a}", fill: "currentColor", opacity: "0.12" }
                    }
                    for l in lines {
                        polyline {
                            points: "{l}",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            stroke_linejoin: "round",
                            stroke_linecap: "round",
                            vector_effect: "non-scaling-stroke",
                        }
                    }
                    for (sx, sy) in singles {
                        line { x1: "{sx:.1}", x2: "{sx:.1}", y1: "{sy:.1}", y2: "{H}", stroke: "currentColor", stroke_width: "2", vector_effect: "non-scaling-stroke" }
                    }
                }
            }
            div { class: "flex justify-between text-[10px] text-muted-foreground tabular-nums",
                for t in ticks {
                    span { "{t}" }
                }
            }
        }
    }
}

/// Rounds up to 1/2/5 × 10^n.
fn nice_ceil(v: f64) -> f64 {
    if v <= 0.0 || !v.is_finite() {
        return 10.0;
    }
    let mag = 10f64.powf(v.log10().floor());
    let n = v / mag;
    let f = if n <= 1.0 {
        1.0
    } else if n <= 2.0 {
        2.0
    } else if n <= 5.0 {
        5.0
    } else {
        10.0
    };
    f * mag
}

#[cfg(test)]
mod tests {
    #[test]
    fn nice_ceil_rounds() {
        assert_eq!(super::nice_ceil(0.0), 10.0);
        assert_eq!(super::nice_ceil(13.0), 20.0);
        assert_eq!(super::nice_ceil(48.0), 50.0);
        assert_eq!(super::nice_ceil(101.0), 200.0);
    }
}
