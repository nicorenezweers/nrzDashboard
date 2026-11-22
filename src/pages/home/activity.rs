//! Activity window: live CPU, RAM, storage and temperature with history and per-app breakdown.

use std::collections::HashMap;

use dioxus::prelude::*;
use icons::{Cpu, HardDrive, MemoryStick, RefreshCw, Thermometer};

use super::{AppIcon, HomeCtx, HomeWindow};
use crate::model::AppUse;
use crate::utils::{err_msg, fmt_bytes, fmt_uptime, pct};

#[component]
pub fn ActivityWindow() -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut live = use_signal(|| true);
    // Pausing freezes the view; the home screen keeps refreshing underneath.
    let mut frozen = use_signal(|| None::<Option<Result<crate::model::Overview, ServerFnError>>>);
    let data = match frozen() {
        Some(d) => d,
        None => ctx.overview.read().clone(),
    };
    let history = match &data {
        Some(Ok(o)) => o.history.clone(),
        _ => Vec::new(),
    };
    let cpu_hist: Vec<f64> = history.iter().map(|h| h[0]).collect();
    let mem_hist: Vec<f64> = history.iter().map(|h| h[1]).collect();
    let toggle = move |_| {
        if live() {
            frozen.set(Some(ctx.overview.read().clone()));
        } else {
            frozen.set(None);
        }
        live.toggle();
    };

    rsx! {
        HomeWindow { title: "Aktivität",
            div { class: "mb-5 flex flex-wrap items-center gap-2",
                button {
                    class: if live() { "flex items-center gap-2 rounded-full bg-emerald-500/20 px-4 py-1.5 text-sm text-emerald-200" } else { "flex items-center gap-2 rounded-full bg-white/10 px-4 py-1.5 text-sm text-white/70" },
                    onclick: toggle,
                    span { class: if live() { "size-2 rounded-full bg-emerald-400 animate-pulse" } else { "size-2 rounded-full bg-white/40" } }
                    if live() { "Live" } else { "Pausiert" }
                }
                button { class: "flex items-center gap-2 rounded-full bg-white/10 px-4 py-1.5 text-sm hover:bg-white/20", onclick: move |_| ctx.reload(),
                    RefreshCw { class: "size-4" }
                    "Aktualisieren"
                }
            }
            match data {
                None => rsx! { div { class: "grid gap-4 md:grid-cols-3", for i in 0..3 { div { key: "{i}", class: "h-44 animate-pulse rounded-3xl bg-white/[0.05]" } } } },
                Some(Err(e)) => rsx! { p { class: "text-sm text-red-300", {err_msg(&e)} } },
                Some(Ok(o)) => {
                    let names: HashMap<String, String> = o.apps.iter().map(|a| (a.id.clone(), a.name.clone())).collect();
                    match o.resources {
                        None => rsx! {
                            p { class: "text-sm text-red-300", {o.error.clone().unwrap_or_else(|| "Keine Daten".into())} }
                            super::monitor::MonitorPanel { live, names: names.clone() }
                        },
                        Some(r) => {
                            let mem_p = pct(r.memory.total_used, r.memory.size);
                            let disk_p = pct(r.storage.total_used, r.storage.size);
                            rsx! {
                                div { class: "grid gap-4 md:grid-cols-3",
                                    BigCard {
                                        icon: rsx! { Cpu {} }, label: "CPU", value: r.cpu.total_used.clamp(0.0, 100.0),
                                        main: format!("{:.1} %", r.cpu.total_used),
                                        sub: format!("{} Threads", r.cpu.threads),
                                        history: cpu_hist.clone(), color: "#34d399",
                                    }
                                    BigCard {
                                        icon: rsx! { MemoryStick {} }, label: "Arbeitsspeicher", value: mem_p,
                                        main: fmt_bytes(r.memory.total_used),
                                        sub: format!("von {} · {:.0} %", fmt_bytes(r.memory.size), mem_p),
                                        history: mem_hist.clone(), color: "#60a5fa",
                                    }
                                    BigCard {
                                        icon: rsx! { HardDrive {} }, label: "Speicher", value: disk_p,
                                        main: fmt_bytes(r.storage.total_used),
                                        sub: format!("{} frei von {}", fmt_bytes(r.storage.available), fmt_bytes(r.storage.size)),
                                        history: Vec::new(), color: "#f472b6",
                                    }
                                }
                                div { class: "mt-4 grid gap-4 md:grid-cols-[1fr_1fr]",
                                    UsageList { title: "CPU nach App", items: r.cpu.apps.clone(), names: names.clone(), cpu: true }
                                    UsageList { title: "RAM nach App", items: r.memory.apps.clone(), names: names.clone(), cpu: false }
                                }
                                div { class: "mt-4 grid gap-4 md:grid-cols-2",
                                    div { class: "rounded-3xl bg-white/[0.05] p-5",
                                        div { class: "mb-3 flex items-center gap-2 text-sm text-white/60", Thermometer { class: "size-4" } "Temperatur" }
                                        div { class: "text-3xl font-semibold tabular-nums", "{r.temperature.temperature:.0} °C" }
                                        if !r.temperature.warning.is_empty() && r.temperature.warning != "normal" {
                                            div { class: "mt-1 text-sm text-amber-300", "{r.temperature.warning}" }
                                        } else {
                                            div { class: "mt-1 text-sm text-emerald-300", "Normal" }
                                        }
                                    }
                                    if let Some(i) = o.info {
                                        div { class: "rounded-3xl bg-white/[0.05] p-5 text-sm",
                                            div { class: "mb-3 text-white/60", "Gerät" }
                                            dl { class: "grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5",
                                                dt { class: "text-white/50", "Name" } dd { class: "truncate", "{i.hostname}" }
                                                dt { class: "text-white/50", "Modell" } dd { class: "truncate", "{i.manufacturer} {i.product_name}" }
                                                dt { class: "text-white/50", "Prozessor" } dd { class: "truncate", "{i.cpu}" }
                                                dt { class: "text-white/50", "umbrelOS" } dd { class: "truncate", "{i.version_name}" }
                                                dt { class: "text-white/50", "Laufzeit" } dd { "{fmt_uptime(i.uptime_seconds)}" }
                                                dt { class: "text-white/50", "IP" } dd { class: "truncate", "{i.ip_addresses.join(\", \")}" }
                                            }
                                        }
                                    }
                                }
                                super::monitor::MonitorPanel { live, names: names.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BigCard(icon: Element, label: String, value: f64, main: String, sub: String, history: Vec<f64>, color: String) -> Element {
    let points: String = if history.len() > 1 {
        let n = (history.len() - 1) as f64;
        history
            .iter()
            .enumerate()
            .map(|(i, v)| format!("{:.1},{:.1}", i as f64 / n * 100.0, 40.0 - v.clamp(0.0, 100.0) * 0.38))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        String::new()
    };
    let area = if points.is_empty() { String::new() } else { format!("0,40 {points} 100,40") };
    rsx! {
        div { class: "relative overflow-hidden rounded-3xl bg-white/[0.05] p-5",
            div { class: "flex items-center gap-2 text-sm text-white/60 [&_svg]:size-4", {icon} "{label}" }
            div { class: "mt-3 text-3xl font-semibold tabular-nums", "{main}" }
            div { class: "text-sm text-white/50", "{sub}" }
            div { class: "mt-4 h-2 overflow-hidden rounded-full bg-white/10",
                div { class: "h-full rounded-full transition-all duration-700", style: "width: {value.clamp(0.0, 100.0)}%; background: {color};" }
            }
            if !points.is_empty() {
                svg { class: "mt-4 h-16 w-full", view_box: "0 0 100 40", preserve_aspect_ratio: "none",
                    polygon { points: "{area}", fill: "{color}", fill_opacity: "0.15" }
                    polyline { points: "{points}", fill: "none", stroke: "{color}", stroke_width: "1.2", vector_effect: "non-scaling-stroke" }
                }
            }
        }
    }
}

#[component]
fn UsageList(title: String, items: Vec<AppUse>, names: HashMap<String, String>, cpu: bool) -> Element {
    let mut items: Vec<AppUse> = items.into_iter().filter(|a| a.used > 0.0).collect();
    items.sort_by(|a, b| b.used.total_cmp(&a.used));
    items.truncate(12);
    let max = items.first().map(|a| a.used).unwrap_or(1.0).max(f64::EPSILON);
    rsx! {
        div { class: "rounded-3xl bg-white/[0.05] p-5",
            div { class: "mb-3 text-sm text-white/60", "{title}" }
            if items.is_empty() {
                p { class: "text-sm text-white/40", "Keine Daten" }
            }
            div { class: "flex flex-col gap-2.5",
                for a in items {
                    {
                        let name = names.get(&a.id).cloned().unwrap_or_else(|| a.id.clone());
                        let text = if cpu { format!("{:.1} %", a.used) } else { fmt_bytes(a.used) };
                        let share = a.used / max * 100.0;
                        rsx! {
                            div { key: "{a.id}", class: "flex items-center gap-3",
                                AppIcon { id: a.id.clone(), name: name.clone(), class: "size-7" }
                                div { class: "min-w-0 flex-1",
                                    div { class: "flex justify-between gap-2 text-sm",
                                        span { class: "truncate", "{name}" }
                                        span { class: "shrink-0 tabular-nums text-white/60", "{text}" }
                                    }
                                    div { class: "mt-1 h-1 overflow-hidden rounded-full bg-white/10",
                                        div { class: if cpu { "h-full rounded-full bg-emerald-400" } else { "h-full rounded-full bg-sky-400" }, style: "width: {share.clamp(1.0, 100.0)}%;" }
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
