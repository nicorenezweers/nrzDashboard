//! App Store window: browse/search the umbrelOS App Store, details, install, updates, installed apps.

use std::collections::HashMap;

use dioxus::prelude::*;
use icons::{ArrowLeft, ArrowUp, Check, Download, ExternalLink, Play, RotateCw, Search, Square, Trash2};

use super::{HomeCtx, HomeWindow};
use crate::api::home::{StoreApp, store_details, store_install, store_search, store_uninstall, update_all_apps};
use crate::api::app_action;
use crate::components::widgets::ConfirmButton;
use crate::model::AppInfo;
use crate::pages::board::widgets::{AppIcon, browser_url};
use crate::utils::{err_msg, fmt_bytes, sleep_ms};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Discover,
    Updates,
    Installed,
}

const PILL: &str = "rounded-full px-4 py-1.5 text-sm transition";
const PILL_ON: &str = "bg-white text-black font-medium";
const PILL_OFF: &str = "bg-white/10 text-white/80 hover:bg-white/15";
const BTN: &str = "inline-flex items-center justify-center gap-1.5 rounded-full px-4 py-1.5 text-sm font-medium transition disabled:opacity-50 disabled:pointer-events-none";
const BTN_PRIMARY: &str = "bg-sky-500 text-white hover:bg-sky-400";
const BTN_GHOST: &str = "bg-white/10 text-white hover:bg-white/20";

/// Installed apps by id (from the shared overview).
fn installed_map(ctx: &HomeCtx) -> HashMap<String, AppInfo> {
    match &*ctx.overview.read() {
        Some(Ok(o)) => o.apps.iter().map(|a| (a.id.clone(), a.clone())).collect(),
        _ => HashMap::new(),
    }
}

fn state_label(state: &str) -> &str {
    match state {
        "ready" | "running" => "Läuft",
        "stopped" => "Gestoppt",
        "installing" => "Wird installiert…",
        "updating" => "Wird aktualisiert…",
        "starting" => "Startet…",
        "stopping" => "Stoppt…",
        "restarting" => "Startet neu…",
        "uninstalling" => "Wird entfernt…",
        other => other,
    }
}

/// Reloads overview + tiles a few times so progress shows up.
fn follow(ctx: HomeCtx) {
    spawn(async move {
        for _ in 0..6 {
            ctx.reload();
            sleep_ms(3_000).await;
        }
    });
}

#[component]
pub fn StoreWindow(updates: bool) -> Element {
    let mut tab = use_signal(move || if updates { Tab::Updates } else { Tab::Discover });
    let mut selected = use_signal(|| None::<String>);
    let notice = use_signal(|| None::<Result<String, String>>);
    let ctx = use_context::<HomeCtx>();
    let n_updates = installed_map(&ctx).values().filter(|a| a.update_available).count();

    rsx! {
        HomeWindow { title: "App Store",
            if let Some(id) = selected() {
                Details { id, notice, onback: move |_| selected.set(None), onopen: move |id| selected.set(Some(id)) }
            } else {
                div { class: "sticky top-0 z-10 -mx-1 flex flex-wrap items-center gap-2 bg-transparent px-1 pb-4",
                    button { class: if tab() == Tab::Discover { "{PILL} {PILL_ON}" } else { "{PILL} {PILL_OFF}" }, onclick: move |_| tab.set(Tab::Discover), "Entdecken" }
                    button { class: if tab() == Tab::Updates { "{PILL} {PILL_ON}" } else { "{PILL} {PILL_OFF}" }, onclick: move |_| tab.set(Tab::Updates),
                        "Updates"
                        if n_updates > 0 { span { class: "ml-1.5 rounded-full bg-sky-500 px-1.5 text-xs text-white", "{n_updates}" } }
                    }
                    button { class: if tab() == Tab::Installed { "{PILL} {PILL_ON}" } else { "{PILL} {PILL_OFF}" }, onclick: move |_| tab.set(Tab::Installed), "Installiert" }
                }
                NoticeLine { notice }
                match tab() {
                    Tab::Discover => rsx! { Discover { onopen: move |id| selected.set(Some(id)) } },
                    Tab::Updates => rsx! { Updates { notice } },
                    Tab::Installed => rsx! { Installed { notice, onopen: move |id| selected.set(Some(id)) } },
                }
            }
        }
    }
}

#[component]
fn NoticeLine(notice: Signal<Option<Result<String, String>>>) -> Element {
    rsx! {
        match notice() {
            Some(Ok(m)) => rsx! { div { class: "mb-4 rounded-xl border border-emerald-400/20 bg-emerald-500/10 px-4 py-2.5 text-sm text-emerald-100", "{m}" } },
            Some(Err(m)) => rsx! { div { class: "mb-4 rounded-xl border border-red-400/20 bg-red-500/10 px-4 py-2.5 text-sm text-red-100", "{m}" } },
            None => rsx! {},
        }
    }
}

#[component]
fn Discover(onopen: EventHandler<String>) -> Element {
    let mut query = use_signal(String::new);
    let mut category = use_signal(|| None::<String>);
    let results = use_resource(move || async move {
        let q = query();
        if !q.trim().is_empty() {
            sleep_ms(250).await;
        }
        store_search(q, 50).await
    });
    let ctx = use_context::<HomeCtx>();
    let installed = installed_map(&ctx);

    let list = results.read().clone();
    let mut cats: Vec<String> = match &list {
        Some(Ok(l)) => l.iter().map(|a| a.category.clone()).filter(|c| !c.is_empty()).collect(),
        _ => vec![],
    };
    cats.sort();
    cats.dedup();

    rsx! {
        div { class: "relative mb-4",
            Search { class: "pointer-events-none absolute left-4 top-1/2 size-4 -translate-y-1/2 text-white/50" }
            input {
                class: "h-11 w-full rounded-full border border-white/10 bg-white/[0.07] pl-11 pr-4 text-[15px] text-white placeholder:text-white/40 outline-none focus:border-white/30",
                placeholder: "Apps im umbrelOS App Store suchen",
                value: "{query}",
                oninput: move |e| {
                    query.set(e.value());
                    category.set(None);
                },
            }
        }
        if cats.len() > 1 {
            div { class: "mb-5 flex flex-wrap gap-2",
                button { class: if category().is_none() { "{PILL} {PILL_ON}" } else { "{PILL} {PILL_OFF}" }, onclick: move |_| category.set(None), "Alle" }
                for c in cats {
                    button {
                        key: "{c}",
                        class: if category().as_deref() == Some(c.as_str()) { "{PILL} {PILL_ON} capitalize" } else { "{PILL} {PILL_OFF} capitalize" },
                        onclick: {
                            let c = c.clone();
                            move |_| category.set(Some(c.clone()))
                        },
                        "{c}"
                    }
                }
            }
        }
        match list {
            None => rsx! {
                div { class: "grid gap-3 sm:grid-cols-2 lg:grid-cols-3",
                    for i in 0..9 { div { key: "{i}", class: "h-24 animate-pulse rounded-2xl bg-white/[0.06]" } }
                }
            },
            Some(Err(e)) => rsx! { p { class: "text-sm text-red-300", {err_msg(&e)} } },
            Some(Ok(l)) => {
                let l: Vec<StoreApp> = l.into_iter().filter(|a| category().is_none_or(|c| a.category == c)).collect();
                rsx! {
                    if l.is_empty() {
                        p { class: "py-10 text-center text-sm text-white/50", "Keine Apps gefunden." }
                    }
                    div { class: "grid gap-3 sm:grid-cols-2 lg:grid-cols-3",
                        for a in l {
                            StoreCard { key: "{a.id}", state: installed.get(&a.id).map(|i| i.state.clone()), app: a, onopen }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StoreCard(app: StoreApp, state: Option<String>, onopen: EventHandler<String>) -> Element {
    let id = app.id.clone();
    rsx! {
        button {
            class: "flex items-center gap-4 rounded-2xl border border-white/[0.06] bg-white/[0.05] p-3.5 text-left transition hover:bg-white/[0.09]",
            onclick: move |_| onopen.call(id.clone()),
            AppIcon { id: app.id.clone(), name: app.name.clone(), class: "size-14 shrink-0" }
            div { class: "min-w-0 flex-1",
                div { class: "truncate font-medium", "{app.name}" }
                div { class: "line-clamp-2 text-xs text-white/55", "{app.tagline}" }
                div { class: "mt-1.5 flex items-center gap-2 text-[11px] text-white/40",
                    span { class: "capitalize", "{app.category}" }
                    if let Some(s) = &state {
                        span { class: "flex items-center gap-1 text-emerald-300", Check { class: "size-3" } {state_label(s).to_string()} }
                    } else if !app.compatible {
                        span { class: "text-amber-300", "Nicht kompatibel" }
                    }
                }
            }
        }
    }
}

#[component]
fn Details(id: String, notice: Signal<Option<Result<String, String>>>, onback: EventHandler<()>, onopen: EventHandler<String>) -> Element {
    let ctx = use_context::<HomeCtx>();
    let id_r = id.clone();
    let details = use_resource(use_reactive!(|id_r| store_details(id_r)));
    let installed = installed_map(&ctx);
    let mut busy = use_signal(|| false);
    let mut notice = notice;
    let state = installed.get(&id).map(|a| a.state.clone());
    let url = match &*ctx.tiles.read() {
        Some(Ok(t)) => t.iter().find(|t| t.id == id).and_then(|t| t.url.clone()).map(|u| browser_url(&u)),
        _ => None,
    };

    let install = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            busy.set(true);
            spawn(async move {
                let r = store_install(id).await.map_err(|e| err_msg(&e));
                notice.set(Some(r.map(|_| "Installation gestartet – die App erscheint gleich auf dem Startbildschirm.".into())));
                busy.set(false);
                follow(ctx);
            });
        }
    };
    let uninstall = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            spawn(async move {
                let r = store_uninstall(id).await.map_err(|e| err_msg(&e));
                notice.set(Some(r.map(|_| "App wird deinstalliert.".into())));
                follow(ctx);
            });
        }
    };

    rsx! {
        button { class: "mb-4 inline-flex items-center gap-1.5 text-sm text-white/60 hover:text-white", onclick: move |_| onback.call(()),
            ArrowLeft { class: "size-4" }
            "Zurück"
        }
        NoticeLine { notice }
        match details.read().clone() {
            None => rsx! { div { class: "h-40 animate-pulse rounded-2xl bg-white/[0.05]" } },
            Some(Err(e)) => rsx! { p { class: "text-sm text-red-300", {err_msg(&e)} } },
            Some(Ok(d)) => {
                let missing: Vec<String> = d.dependencies.iter().filter(|dep| !installed.contains_key(*dep)).cloned().collect();
                rsx! {
                    div { class: "flex flex-col gap-5 sm:flex-row sm:items-center",
                        AppIcon { id: d.id.clone(), name: d.name.clone(), class: "size-24 shrink-0 shadow-xl" }
                        div { class: "min-w-0 flex-1",
                            h3 { class: "text-2xl font-semibold", "{d.name}" }
                            p { class: "text-white/60", "{d.tagline}" }
                            div { class: "mt-1 flex flex-wrap gap-x-3 text-xs text-white/40",
                                span { "Version {d.version}" }
                                span { class: "capitalize", "{d.category}" }
                                if let Some(s) = d.install_size { span { "{fmt_bytes(s)}" } }
                            }
                        }
                        div { class: "flex flex-wrap items-center gap-2",
                            if let Some(s) = &state {
                                if let Some(u) = &url {
                                    a { class: "{BTN} {BTN_PRIMARY}", href: "{u}", target: "_blank", rel: "noopener noreferrer", ExternalLink { class: "size-4" } "Öffnen" }
                                } else {
                                    span { class: "{BTN} {BTN_GHOST}", {state_label(s).to_string()} }
                                }
                                ConfirmButton { label: "Deinstallieren", confirm_label: "Daten werden gelöscht – sicher?", onconfirm: uninstall, Trash2 { class: "size-4" } }
                            } else {
                                button {
                                    class: "{BTN} {BTN_PRIMARY}",
                                    disabled: busy() || !d.compatible || !missing.is_empty(),
                                    onclick: install,
                                    Download { class: "size-4" }
                                    if busy() { "Wird gestartet…" } else { "Installieren" }
                                }
                            }
                        }
                    }
                    if !d.compatible {
                        p { class: "mt-4 rounded-xl bg-amber-500/10 px-4 py-2.5 text-sm text-amber-200", "Diese App ist mit deinem Gerät nicht kompatibel." }
                    }
                    if !d.dependencies.is_empty() {
                        div { class: "mt-5 rounded-2xl bg-white/[0.05] p-4",
                            h4 { class: "mb-2 text-sm font-medium text-white/70", "Benötigt" }
                            div { class: "flex flex-wrap gap-2",
                                for dep in d.dependencies.clone() {
                                    button {
                                        key: "{dep}",
                                        class: "flex items-center gap-2 rounded-full bg-white/10 py-1 pl-1 pr-3 text-sm hover:bg-white/15",
                                        onclick: {
                                            let dep = dep.clone();
                                            move |_| onopen.call(dep.clone())
                                        },
                                        AppIcon { id: dep.clone(), name: dep.clone(), class: "size-6" }
                                        "{dep}"
                                        if installed.contains_key(&dep) { Check { class: "size-3.5 text-emerald-300" } } else { span { class: "text-xs text-amber-300", "fehlt" } }
                                    }
                                }
                            }
                            if !missing.is_empty() {
                                p { class: "mt-2 text-xs text-white/50", "Installiere zuerst die fehlenden Apps." }
                            }
                        }
                    }
                    div { class: "mt-6 grid gap-6 lg:grid-cols-[2fr_1fr]",
                        div {
                            h4 { class: "mb-2 font-medium", "Über die App" }
                            p { class: "whitespace-pre-line text-sm leading-relaxed text-white/70", "{d.description}" }
                        }
                        if !d.release_notes.is_empty() {
                            div {
                                h4 { class: "mb-2 font-medium", "Neu in {d.version}" }
                                p { class: "whitespace-pre-line text-sm leading-relaxed text-white/60", "{d.release_notes}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Updates(notice: Signal<Option<Result<String, String>>>) -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut notice = notice;
    let mut busy = use_signal(|| false);
    let mut list: Vec<AppInfo> = installed_map(&ctx).into_values().filter(|a| a.update_available || a.state == "updating").collect();
    list.sort_by_key(|a| a.name.to_lowercase());
    let any = list.iter().any(|a| a.update_available);

    rsx! {
        div { class: "mb-4 flex items-center justify-between gap-3",
            p { class: "text-sm text-white/60",
                if list.is_empty() { "Alle Apps sind aktuell." } else { "{list.len()} App(s) mit Update" }
            }
            div { class: "flex gap-2",
                button { class: "{BTN} {BTN_GHOST}", onclick: move |_| ctx.reload(), RotateCw { class: "size-4" } "Prüfen" }
                if any {
                    button {
                        class: "{BTN} {BTN_PRIMARY}",
                        disabled: busy(),
                        onclick: move |_| {
                            busy.set(true);
                            spawn(async move {
                                let r = update_all_apps().await.map_err(|e| err_msg(&e));
                                notice.set(Some(r.map(|names| format!("Update gestartet: {}", names.join(", ")))));
                                busy.set(false);
                                follow(ctx);
                            });
                        },
                        ArrowUp { class: "size-4" }
                        "Alle aktualisieren"
                    }
                }
            }
        }
        div { class: "flex flex-col gap-2",
            for a in list {
                div { key: "{a.id}", class: "flex items-center gap-4 rounded-2xl bg-white/[0.05] p-3",
                    AppIcon { id: a.id.clone(), name: a.name.clone(), class: "size-12" }
                    div { class: "min-w-0 flex-1",
                        div { class: "truncate font-medium", "{a.name}" }
                        div { class: "text-xs text-white/50", "Installiert: {a.version}" }
                    }
                    if a.state == "updating" {
                        span { class: "text-sm text-sky-300", "Wird aktualisiert…" }
                    } else {
                        button {
                            class: "{BTN} {BTN_PRIMARY}",
                            onclick: {
                                let id = a.id.clone();
                                move |_| {
                                    let id = id.clone();
                                    spawn(async move {
                                        let r = app_action(id, "update".into()).await.map_err(|e| err_msg(&e));
                                        notice.set(Some(r.map(|_| "Update gestartet.".into())));
                                        follow(ctx);
                                    });
                                }
                            },
                            "Aktualisieren"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Installed(notice: Signal<Option<Result<String, String>>>, onopen: EventHandler<String>) -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut notice = notice;
    let mut list: Vec<AppInfo> = installed_map(&ctx).into_values().collect();
    list.sort_by_key(|a| a.name.to_lowercase());
    let act = move |id: String, action: &'static str| {
        spawn(async move {
            let r = app_action(id, action.into()).await.map_err(|e| err_msg(&e));
            notice.set(Some(r.map(|_| match action {
                "start" => "Wird gestartet…".into(),
                "stop" => "Wird gestoppt…".into(),
                _ => "Wird neu gestartet…".into(),
            })));
            follow(ctx);
        });
    };
    rsx! {
        p { class: "mb-4 text-sm text-white/60", "{list.len()} Apps installiert" }
        div { class: "grid gap-2 lg:grid-cols-2",
            for a in list {
                {
                    let running = matches!(a.state.as_str(), "ready" | "running");
                    let id = a.id.clone();
                    rsx! {
                        div { key: "{a.id}", class: "flex items-center gap-3 rounded-2xl bg-white/[0.05] p-3",
                            button { class: "flex min-w-0 flex-1 items-center gap-3 text-left", onclick: {
                                    let id = id.clone();
                                    move |_| onopen.call(id.clone())
                                },
                                AppIcon { id: a.id.clone(), name: a.name.clone(), class: "size-11" }
                                div { class: "min-w-0",
                                    div { class: "truncate font-medium", "{a.name}" }
                                    div { class: if running { "text-xs text-emerald-300" } else { "text-xs text-white/50" }, "{state_label(&a.state)} · {a.version}" }
                                }
                            }
                            if running {
                                button { class: "rounded-full p-2 text-white/60 hover:bg-white/10 hover:text-white", title: "Neu starten",
                                    onclick: { let id = id.clone(); move |_| act(id.clone(), "restart") },
                                    RotateCw { class: "size-4" }
                                }
                                button { class: "rounded-full p-2 text-white/60 hover:bg-white/10 hover:text-white", title: "Stoppen",
                                    onclick: { let id = id.clone(); move |_| act(id.clone(), "stop") },
                                    Square { class: "size-4" }
                                }
                            } else if a.state == "stopped" {
                                button { class: "rounded-full p-2 text-white/60 hover:bg-white/10 hover:text-white", title: "Starten",
                                    onclick: { let id = id.clone(); move |_| act(id.clone(), "start") },
                                    Play { class: "size-4" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
