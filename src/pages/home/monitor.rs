//! nrzMonitor part of the activity window: Docker containers, file systems, reachability,
//! temperatures, alerts and recent events.

use std::collections::HashMap;

use dioxus::prelude::*;
use icons::{
    Bell, Container as ContainerIcon, HardDrive, Network, Search, Thermometer, TriangleAlert,
};

use super::AppIcon;
use crate::api::home::monitor_view;
use crate::model::{Container, MonitorView};
use crate::utils::{err_msg, fmt_bytes, fmt_time_ms, pct, sleep_ms};

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Running,
    Problems,
}

fn is_problem(c: &Container) -> bool {
    c.health == "unhealthy"
        || c.oom_killed
        || c.state == "restarting"
        || c.state == "dead"
        || (c.state == "exited" && c.status.contains("(1"))
}

fn fmt_bits(b: f64) -> String {
    match b {
        b if b >= 1e9 => format!("{:.1} Gbit/s", b / 1e9),
        b if b >= 1e6 => format!("{:.1} Mbit/s", b / 1e6),
        b if b >= 1e3 => format!("{:.0} kbit/s", b / 1e3),
        b => format!("{b:.0} bit/s"),
    }
}

#[component]
pub fn MonitorPanel(live: Signal<bool>, names: HashMap<String, String>) -> Element {
    let mut data = use_resource(monitor_view);
    use_future(move || async move {
        loop {
            sleep_ms(5_000).await;
            if live() {
                data.restart();
            }
        }
    });
    // Keep the last good data while reloading.
    let mut last = use_signal(|| None::<MonitorView>);
    let current = data.read().clone();
    if let Some(Ok(v)) = &current {
        if last.peek().as_ref() != Some(v) {
            last.set(Some(v.clone()));
        }
    }
    let shown = last();

    rsx! {
        match (shown, current) {
            (Some(v), _) => rsx! { MonitorBody { v, names } },
            (None, Some(Err(e))) => rsx! {
                div { class: "mt-4 rounded-3xl bg-white/[0.05] p-5 text-sm",
                    div { class: "flex items-center gap-2 text-white/60", ContainerIcon { class: "size-4" } "Container (nrzMonitor)" }
                    p { class: "mt-2 text-red-300", {err_msg(&e)} }
                    p { class: "mt-1 text-white/45", "Adresse und Schlüssel unter Einstellungen → Verbindungen → nrzMonitor." }
                }
            },
            (None, _) => rsx! { div { class: "mt-4 h-64 animate-pulse rounded-3xl bg-white/[0.05]" } },
        }
    }
}

#[component]
fn MonitorBody(v: MonitorView, names: HashMap<String, String>) -> Element {
    let mut filter = use_signal(|| Filter::Running);
    let mut query = use_signal(String::new);

    let total = v.containers.len();
    let running = v.containers.iter().filter(|c| c.state == "running").count();
    let problems = v.containers.iter().filter(|c| is_problem(c)).count();
    let cpu: f64 = v.containers.iter().map(|c| c.cpu).sum();
    let mem: f64 = v.containers.iter().map(|c| c.mem).sum();
    let rx: f64 = v.containers.iter().map(|c| c.net_rx).sum();
    let tx: f64 = v.containers.iter().map(|c| c.net_tx).sum();

    let q = query().to_lowercase();
    let mut list: Vec<Container> = v
        .containers
        .iter()
        .filter(|c| match filter() {
            Filter::All => true,
            Filter::Running => c.state == "running",
            Filter::Problems => is_problem(c),
        })
        .filter(|c| {
            q.is_empty()
                || c.name.to_lowercase().contains(&q)
                || c.app.to_lowercase().contains(&q)
                || c.image.to_lowercase().contains(&q)
        })
        .cloned()
        .collect();
    list.sort_by(|a, b| {
        b.cpu
            .total_cmp(&a.cpu)
            .then(b.mem.total_cmp(&a.mem))
            .then(a.name.cmp(&b.name))
    });

    let chip = move |f: Filter, label: String| {
        let active = filter() == f;
        rsx! {
            button {
                class: if active { "rounded-full bg-white px-3 py-1 text-xs font-medium text-black" } else { "rounded-full bg-white/10 px-3 py-1 text-xs text-white/70 hover:bg-white/20" },
                onclick: move |_| filter.set(f),
                "{label}"
            }
        }
    };

    rsx! {
        if !v.alerts.is_empty() {
            div { class: "mt-4 rounded-3xl bg-red-500/15 p-4 text-sm text-red-100",
                div { class: "mb-1 flex items-center gap-2 font-medium", TriangleAlert { class: "size-4" } "Aktive Alarme" }
                for (i, a) in v.alerts.iter().enumerate() { div { key: "{i}", "{a}" } }
            }
        }
        // Container summary
        div { class: "mt-4 grid grid-cols-2 gap-3 md:grid-cols-4",
            Stat { label: "Container", value: format!("{running} / {total}"), sub: "laufen".to_string() }
            Stat { label: "CPU (Container)", value: format!("{cpu:.0} %"), sub: "100 % = ein Kern".to_string() }
            Stat { label: "RAM (Container)", value: fmt_bytes(mem), sub: if problems > 0 { format!("{problems} mit Problemen") } else { "alle gesund".to_string() } }
            Stat { label: "Netzwerk", value: format!("↓ {}", fmt_bits(rx)), sub: format!("↑ {}", fmt_bits(tx)) }
        }
        // Container list
        div { class: "mt-4 rounded-3xl bg-white/[0.05] p-5",
            div { class: "mb-3 flex flex-wrap items-center gap-2",
                div { class: "mr-auto flex items-center gap-2 text-sm text-white/60", ContainerIcon { class: "size-4" } "Docker-Container" }
                {chip(Filter::Running, format!("Laufend {running}"))}
                {chip(Filter::Problems, format!("Probleme {problems}"))}
                {chip(Filter::All, format!("Alle {total}"))}
                label { class: "flex h-8 items-center gap-2 rounded-full bg-white/10 px-3",
                    Search { class: "size-3.5 text-white/50" }
                    input { class: "w-28 bg-transparent text-xs outline-none placeholder:text-white/40 sm:w-40", placeholder: "Suchen", value: "{query}", oninput: move |e| query.set(e.value()) }
                }
            }
            if list.is_empty() {
                p { class: "py-6 text-center text-sm text-white/40", "Keine Container" }
            }
            div { class: "flex max-h-[28rem] flex-col divide-y divide-white/[0.06] overflow-y-auto",
                for c in list {
                    ContainerRow { key: "{c.name}", name: names.get(&c.app).cloned().unwrap_or_else(|| c.app.clone()), c }
                }
            }
        }
        div { class: "mt-4 grid gap-4 md:grid-cols-2",
            if !v.filesystems.is_empty() {
                div { class: "rounded-3xl bg-white/[0.05] p-5",
                    div { class: "mb-3 flex items-center gap-2 text-sm text-white/60", HardDrive { class: "size-4" } "Datenträger" }
                    div { class: "flex flex-col gap-3",
                        for f in v.filesystems.iter() {
                            {
                                let p = pct(f.used, f.size);
                                let color = if p > 90.0 { "bg-red-400" } else if p > 75.0 { "bg-amber-400" } else { "bg-pink-400" };
                                let short = f.mount.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or("/").to_string();
                                rsx! {
                                    div { key: "{f.mount}",
                                        div { class: "flex justify-between gap-2 text-sm",
                                            span { class: "truncate", title: "{f.mount} ({f.device}, {f.fstype})", "{short}" }
                                            span { class: "shrink-0 tabular-nums text-white/60", "{fmt_bytes(f.avail)} frei von {fmt_bytes(f.size)}" }
                                        }
                                        div { class: "mt-1 h-1.5 overflow-hidden rounded-full bg-white/10",
                                            div { class: "h-full rounded-full {color}", style: "width: {p.clamp(1.0, 100.0)}%;" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !v.probes.is_empty() {
                div { class: "rounded-3xl bg-white/[0.05] p-5",
                    div { class: "mb-3 flex items-center gap-2 text-sm text-white/60", Network { class: "size-4" } "Erreichbarkeit" }
                    div { class: "flex flex-col gap-2",
                        for (i, p) in v.probes.iter().enumerate() {
                            div { key: "{i}", class: "flex items-center gap-3 text-sm",
                                span { class: if p.pending { "size-2 shrink-0 rounded-full bg-white/40" } else if p.up { "size-2 shrink-0 rounded-full bg-emerald-400" } else { "size-2 shrink-0 rounded-full bg-red-400" } }
                                span { class: "min-w-0 flex-1 truncate", title: "{p.target}", "{p.name}" }
                                span { class: "shrink-0 tabular-nums text-white/60",
                                    if let Some(l) = p.latency_ms { "{l:.0} ms" } else if p.up { "" } else { "offline" }
                                }
                                if let Some(u) = p.uptime_pct {
                                    span { class: "w-16 shrink-0 text-right tabular-nums text-white/40", "{u:.1} %" }
                                }
                            }
                        }
                    }
                }
            }
            if !v.sensors.is_empty() {
                div { class: "rounded-3xl bg-white/[0.05] p-5",
                    div { class: "mb-3 flex items-center gap-2 text-sm text-white/60", Thermometer { class: "size-4" } "Temperaturen" }
                    div { class: "grid grid-cols-2 gap-2",
                        for (i, s) in v.sensors.iter().enumerate() {
                            {
                                let t = s.value.unwrap_or_default();
                                let hot = s.crit.is_some_and(|c| t >= c - 10.0) || t >= 80.0;
                                rsx! {
                                    div { key: "{i}", class: "rounded-2xl bg-white/[0.04] px-3 py-2",
                                        div { class: "truncate text-xs text-white/50", "{s.chip_label} · {s.label}" }
                                        div { class: if hot { "text-lg font-semibold tabular-nums text-amber-300" } else { "text-lg font-semibold tabular-nums" }, "{t:.0} °C" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !v.events.is_empty() {
                div { class: "rounded-3xl bg-white/[0.05] p-5",
                    div { class: "mb-3 flex items-center gap-2 text-sm text-white/60", Bell { class: "size-4" } "Ereignisse" }
                    div { class: "flex flex-col gap-2",
                        for (i, e) in v.events.iter().enumerate() {
                            div { key: "{i}", class: "flex items-start gap-3 text-sm",
                                span { class: match e.severity.as_str() {
                                    "critical" | "error" => "mt-1.5 size-2 shrink-0 rounded-full bg-red-400",
                                    "warning" => "mt-1.5 size-2 shrink-0 rounded-full bg-amber-400",
                                    "ok" => "mt-1.5 size-2 shrink-0 rounded-full bg-emerald-400",
                                    _ => "mt-1.5 size-2 shrink-0 rounded-full bg-sky-400",
                                } }
                                span { class: "min-w-0 flex-1", "{e.title}" }
                                span { class: "shrink-0 text-xs tabular-nums text-white/40", "{fmt_time_ms(e.ts)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Stat(label: String, value: String, sub: String) -> Element {
    rsx! {
        div { class: "rounded-3xl bg-white/[0.05] p-4",
            div { class: "text-xs text-white/50", "{label}" }
            div { class: "mt-1 truncate text-xl font-semibold tabular-nums", "{value}" }
            div { class: "truncate text-xs text-white/45", "{sub}" }
        }
    }
}

#[component]
fn ContainerRow(c: Container, name: String) -> Element {
    let running = c.state == "running";
    let (dot, badge) = match (c.state.as_str(), c.health.as_str()) {
        (_, "unhealthy") => ("bg-red-400", "ungesund"),
        ("running", "starting") => ("bg-amber-400", "startet"),
        ("running", _) => ("bg-emerald-400", "läuft"),
        ("restarting", _) => ("bg-amber-400", "Neustart"),
        ("paused", _) => ("bg-sky-400", "pausiert"),
        ("created", _) => ("bg-white/40", "erstellt"),
        ("exited", _) => ("bg-white/30", "gestoppt"),
        _ => ("bg-red-400", "Fehler"),
    };
    let mem_share = if c.mem_limit > 0.0 {
        format!(" / {}", fmt_bytes(c.mem_limit))
    } else {
        String::new()
    };
    let label = if c.service.is_empty() || c.service == "app" {
        name.clone()
    } else {
        format!("{name} · {}", c.service)
    };
    rsx! {
        div { class: "flex items-center gap-3 py-2.5",
            AppIcon { id: c.app.clone(), name: name.clone(), class: "size-8 shrink-0" }
            div { class: "min-w-0 flex-1",
                div { class: "flex items-center gap-2",
                    span { class: "truncate text-sm font-medium", title: "{c.name}", "{label}" }
                    span { class: "flex shrink-0 items-center gap-1 rounded-full bg-white/[0.06] px-2 py-0.5 text-[11px] text-white/60",
                        span { class: "size-1.5 rounded-full {dot}" }
                        "{badge}"
                    }
                    if c.restarts > 0.0 {
                        span { class: "shrink-0 rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] text-amber-200", "{c.restarts:.0}× neu gestartet" }
                    }
                    if c.oom_killed {
                        span { class: "shrink-0 rounded-full bg-red-500/15 px-2 py-0.5 text-[11px] text-red-200", "Speicher voll (OOM)" }
                    }
                }
                div { class: "truncate text-xs text-white/40", title: "{c.image}", "{c.image}" }
            }
            if running {
                div { class: "hidden w-20 shrink-0 text-right sm:block",
                    div { class: "text-sm tabular-nums", "{c.cpu:.1} %" }
                    div { class: "text-[11px] text-white/40", "CPU" }
                }
                div { class: "w-24 shrink-0 text-right sm:w-32",
                    div { class: "truncate text-sm tabular-nums", "{fmt_bytes(c.mem)}{mem_share}" }
                    div { class: "text-[11px] text-white/40 sm:hidden", "{c.cpu:.1} % CPU" }
                    div { class: "hidden text-[11px] text-white/40 sm:block", "RAM" }
                }
            } else {
                div { class: "w-24 shrink-0 truncate text-right text-xs text-white/40 sm:w-52", title: "{c.status}", "{c.status}" }
            }
        }
    }
}
