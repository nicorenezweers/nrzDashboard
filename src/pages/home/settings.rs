//! Settings window: greeting/wallpaper, device (hostname, release channel, restart),
//! umbrelOS updates, network, notifications – all through the umbrelOS MCP server.

use dioxus::prelude::*;
use icons::{ArrowUp, Bell, Network, Power, RefreshCw, Sparkles, Terminal, User};

use super::{HomeCtx, HomeWindow, Win, open_win};
use crate::api::home::mcp_run;
use crate::app::Route;
use crate::components::widgets::ConfirmButton;
use crate::utils::{err_msg, fmt_bytes, fmt_uptime};

const INPUT: &str = "h-10 w-full rounded-xl border border-white/10 bg-white/[0.06] px-3 text-sm text-white placeholder:text-white/35 outline-none focus:border-white/30";
const BTN: &str = "inline-flex items-center justify-center gap-1.5 rounded-full bg-white/10 px-4 py-1.5 text-sm text-white transition hover:bg-white/20 disabled:opacity-50";

fn run(name: &'static str, args: String) -> impl std::future::Future<Output = Result<String, String>> {
    async move { mcp_run(name.into(), args).await.map_err(|e| err_msg(&e)) }
}

#[component]
pub fn SettingsWindow() -> Element {
    let ctx = use_context::<HomeCtx>();
    let info = match &*ctx.overview.read() {
        Some(Ok(o)) => o.info.clone(),
        _ => None,
    };

    rsx! {
        HomeWindow { title: "Einstellungen",
            // Device header
            if let Some(i) = &info {
                div { class: "mb-6 flex flex-col gap-1 rounded-3xl bg-gradient-to-br from-white/[0.09] to-white/[0.03] p-5",
                    div { class: "text-2xl font-semibold", "{i.hostname}" }
                    div { class: "text-sm text-white/60", "{i.manufacturer} {i.product_name} · umbrelOS {i.version_name}" }
                    div { class: "mt-2 flex flex-wrap gap-x-5 gap-y-1 text-xs text-white/50",
                        span { "Laufzeit {fmt_uptime(i.uptime_seconds)}" }
                        span { "RAM {fmt_bytes(i.memory_size)}" }
                        span { "Speicher {fmt_bytes(i.storage_size)}" }
                        span { "{i.ip_addresses.join(\", \")}" }
                        span { class: "capitalize", "Kanal: {i.release_channel}" }
                    }
                }
            }
            div { class: "grid gap-4 lg:grid-cols-2",
                PersonalCard {}
                DeviceCard { hostname: info.as_ref().map(|i| i.hostname.clone()).unwrap_or_default(), channel: info.as_ref().map(|i| i.release_channel.clone()).unwrap_or_default() }
                UpdateCard {}
                TextCard { icon: rsx! { Network {} }, title: "Netzwerk", tool: "get_network_settings" }
                TextCard { icon: rsx! { Bell {} }, title: "Benachrichtigungen", tool: "get_notifications" }
                Card { icon: rsx! { Terminal {} }, title: "Mehr",
                    p { class: "text-sm text-white/60", "Jedes Werkzeug des umbrelOS-MCP-Servers lässt sich in der MCP-Konsole direkt ausführen. Umbrel-Token, nrzNews und Integrationen stellst du unter System ein." }
                    div { class: "mt-3 flex flex-wrap gap-2",
                        button { class: BTN, onclick: move |_| open_win(Win::Console), "MCP-Konsole" }
                        Link { class: BTN, to: Route::SystemPage {}, "System & Verbindung" }
                        Link { class: BTN, to: Route::BoardPage {}, "Board" }
                    }
                }
            }
        }
    }
}

#[component]
fn Card(icon: Element, title: String, children: Element) -> Element {
    rsx! {
        section { class: "rounded-3xl bg-white/[0.05] p-5",
            h3 { class: "mb-4 flex items-center gap-2 font-medium [&_svg]:size-4 [&_svg]:text-white/60", {icon} "{title}" }
            {children}
        }
    }
}

#[component]
fn Msg(msg: Option<Result<String, String>>) -> Element {
    rsx! {
        match msg {
            Some(Ok(m)) => rsx! { pre { class: "mt-3 max-h-48 overflow-auto whitespace-pre-wrap rounded-xl bg-black/30 p-3 font-mono text-xs text-emerald-100", "{m}" } },
            Some(Err(m)) => rsx! { p { class: "mt-3 rounded-xl bg-red-500/10 p-3 text-sm text-red-200", "{m}" } },
            None => rsx! {},
        }
    }
}

#[component]
fn PersonalCard() -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut prefs = ctx.prefs;
    let mut saved = use_signal(|| false);
    let p = prefs.read().clone();
    let mut set = move |f: &dyn Fn(&mut super::Prefs)| {
        let mut v = prefs.read().clone();
        f(&mut v);
        v.save();
        prefs.set(v);
        saved.set(true);
    };
    rsx! {
        Card { icon: rsx! { User {} }, title: "Startbildschirm",
            label { class: "mb-1 block text-xs text-white/50", "Name in der Begrüßung" }
            input { class: INPUT, value: "{p.name}", placeholder: "z. B. nrz", oninput: move |e| set(&|v| v.name = e.value()) }
            label { class: "mb-1 mt-4 block text-xs text-white/50", "Hintergrund" }
            div { class: "flex flex-wrap gap-2",
                for (key, label) in [("default", "Standard"), ("none", "Schlicht"), ("custom", "Eigenes Bild")] {
                    button {
                        key: "{key}",
                        class: if p.wallpaper == key { "rounded-full bg-white px-4 py-1.5 text-sm font-medium text-black" } else { "rounded-full bg-white/10 px-4 py-1.5 text-sm hover:bg-white/20" },
                        onclick: move |_| set(&|v| v.wallpaper = key.to_string()),
                        "{label}"
                    }
                }
            }
            if p.wallpaper == "custom" {
                input { class: "{INPUT} mt-2", value: "{p.custom_url}", placeholder: "https://… Bild-URL", oninput: move |e| set(&|v| v.custom_url = e.value()) }
            }
            if saved() {
                p { class: "mt-3 text-xs text-white/40", "Gespeichert (in diesem Browser)." }
            }
        }
    }
}

#[component]
fn DeviceCard(hostname: String, channel: String) -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut host = use_signal(|| hostname.clone());
    let mut msg = use_signal(|| None::<Result<String, String>>);
    let valid = {
        let h = host();
        !h.is_empty() && h.len() <= 63 && h.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && !h.starts_with('-') && !h.ends_with('-')
    };
    let set_channel = move |c: &'static str| {
        spawn(async move {
            msg.set(Some(run("set_release_channel", format!(r#"{{"channel":"{c}"}}"#)).await));
            ctx.reload();
        });
    };
    rsx! {
        Card { icon: rsx! { Power {} }, title: "Gerät",
            label { class: "mb-1 block text-xs text-white/50", "Gerätename (Hostname, .local-Adresse)" }
            div { class: "flex gap-2",
                input { class: INPUT, value: "{host}", oninput: move |e| host.set(e.value().to_lowercase()) }
                ConfirmButton {
                    label: "Ändern",
                    confirm_label: "Sicher?",
                    disabled: !valid || host() == hostname,
                    onconfirm: move |_| {
                        let h = host();
                        spawn(async move {
                            msg.set(Some(run("set_hostname", serde_json::json!({ "hostname": h }).to_string()).await));
                            ctx.reload();
                        });
                    },
                }
            }
            label { class: "mb-1 mt-4 block text-xs text-white/50", "Update-Kanal" }
            div { class: "flex gap-2",
                for (c, l) in [("stable", "Stabil"), ("beta", "Beta")] {
                    button {
                        key: "{c}",
                        class: if channel == c { "rounded-full bg-white px-4 py-1.5 text-sm font-medium text-black" } else { "rounded-full bg-white/10 px-4 py-1.5 text-sm hover:bg-white/20" },
                        onclick: move |_| set_channel(c),
                        "{l}"
                    }
                }
            }
            div { class: "mt-5 flex flex-wrap items-center gap-2",
                ConfirmButton {
                    label: "Umbrel neu starten",
                    confirm_label: "Jetzt neu starten?",
                    onconfirm: move |_| {
                        spawn(async move {
                            msg.set(Some(run("restart_device", "{}".into()).await));
                        });
                    },
                    Power { class: "size-4" }
                }
            }
            Msg { msg: msg() }
        }
    }
}

#[component]
fn UpdateCard() -> Element {
    let mut check = use_resource(|| run("check_os_update", "{}".into()));
    let mut status = use_resource(|| run("get_os_update_status", "{}".into()));
    let mut msg = use_signal(|| None::<Result<String, String>>);
    let available = matches!(&*check.read(), Some(Ok(t)) if update_available(t));
    rsx! {
        Card { icon: rsx! { Sparkles {} }, title: "umbrelOS-Update",
            match check.read().clone() {
                None => rsx! { p { class: "text-sm text-white/50", "Suche nach Updates…" } },
                Some(Err(e)) => rsx! { p { class: "text-sm text-red-300", "{e}" } },
                Some(Ok(t)) => rsx! {
                    p { class: if available { "text-sm text-sky-200" } else { "text-sm text-emerald-200" },
                        if available { "Ein Update ist verfügbar." } else { "umbrelOS ist aktuell." }
                    }
                    pre { class: "mt-2 max-h-40 overflow-auto whitespace-pre-wrap rounded-xl bg-black/30 p-3 font-sans text-xs text-white/70", "{update_text(&t)}" }
                },
            }
            if let Some(Ok(s)) = status.read().clone() {
                details { class: "mt-2 text-xs text-white/50",
                    summary { class: "cursor-pointer", "Update-Status" }
                    pre { class: "mt-1 whitespace-pre-wrap font-mono", "{s}" }
                }
            }
            div { class: "mt-3 flex flex-wrap gap-2",
                button { class: BTN, onclick: move |_| { check.restart(); status.restart(); }, RefreshCw { class: "size-4" } "Erneut prüfen" }
                if available {
                    ConfirmButton {
                        label: "Update installieren",
                        confirm_label: "Gerät startet neu – sicher?",
                        onconfirm: move |_| {
                            spawn(async move {
                                msg.set(Some(run("install_os_update", "{}".into()).await));
                                status.restart();
                            });
                        },
                        ArrowUp { class: "size-4" }
                    }
                }
            }
            Msg { msg: msg() }
        }
    }
}

/// Version name + release notes from the check_os_update answer.
fn update_text(text: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(v) => {
            let s = |k: &str| v.get(k).and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
            format!("{}\n\n{}", s("name"), s("releaseNotes")).trim().to_string()
        }
        Err(_) => text.to_string(),
    }
}

/// Looks for an "available: true"-style flag in the check_os_update answer.
fn update_available(text: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(v) => ["/available", "/updateAvailable", "/update/available"].iter().any(|p| v.pointer(p).and_then(serde_json::Value::as_bool) == Some(true)),
        Err(_) => {
            let t = text.to_lowercase();
            t.contains("available") && !t.contains("no update") && !t.contains("not available") && !t.contains("up to date")
        }
    }
}

#[component]
fn TextCard(icon: Element, title: String, tool: &'static str) -> Element {
    let mut data = use_resource(move || run(tool, "{}".into()));
    rsx! {
        Card { icon, title,
            match data.read().clone() {
                None => rsx! { div { class: "h-20 animate-pulse rounded-xl bg-white/[0.04]" } },
                Some(Err(e)) => rsx! { p { class: "text-sm text-red-300", "{e}" } },
                Some(Ok(t)) => rsx! { pre { class: "max-h-56 overflow-auto whitespace-pre-wrap rounded-xl bg-black/30 p-3 font-mono text-xs text-white/70", "{t}" } },
            }
            button { class: "{BTN} mt-3", onclick: move |_| data.restart(), RefreshCw { class: "size-4" } "Neu laden" }
        }
    }
}

