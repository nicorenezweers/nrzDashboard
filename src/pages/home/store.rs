//! App Store: featured app, categories, search, details, install, updates and installed apps.

use std::collections::HashMap;

use dioxus::prelude::*;
use icons::{ArrowLeft, ArrowUp, Check, Download, Play, RotateCw, Search, Square, Trash2};

use super::{AppIcon, HomeCtx, HomeWindow, open_url};
use crate::api::app_action;
use crate::api::home::{StoreApp, StoreDetails, store_details, store_install, store_search, store_uninstall, update_all_apps};
use crate::components::widgets::ConfirmButton;
use crate::model::AppInfo;
use crate::utils::{err_msg, fmt_bytes, sleep_ms};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Discover,
    Updates,
    Installed,
}

type Notice = Signal<Option<Result<String, String>>>;

const PILL_BLUE: &str = "inline-flex h-8 items-center justify-center gap-1.5 rounded-full bg-sky-500 px-4 text-[13px] font-semibold text-white transition hover:bg-sky-400 disabled:opacity-40 disabled:pointer-events-none";
const PILL_GREY: &str = "inline-flex h-8 items-center justify-center gap-1.5 rounded-full bg-white/10 px-4 text-[13px] font-semibold text-sky-300 transition hover:bg-white/15";
const BIG_BLUE: &str = "inline-flex h-10 items-center justify-center gap-2 rounded-full bg-sky-500 px-6 text-sm font-semibold text-white transition hover:bg-sky-400 disabled:opacity-40 disabled:pointer-events-none";
const BIG_GREY: &str = "inline-flex h-10 items-center justify-center gap-2 rounded-full bg-white/10 px-6 text-sm font-semibold text-white transition hover:bg-white/20";

fn category_label(c: &str) -> String {
    match c {
        "files" => "Dateien".into(),
        "media" => "Medien".into(),
        "automation" => "Smart Home".into(),
        "networking" => "Netzwerk".into(),
        "social" => "Soziales".into(),
        "developer" => "Entwickler".into(),
        "finance" => "Finanzen".into(),
        "bitcoin" => "Bitcoin".into(),
        "crypto" => "Krypto".into(),
        "ai" => "KI".into(),
        "gaming" => "Spiele".into(),
        "productivity" => "Produktivität".into(),
        other => {
            let mut ch = other.chars();
            ch.next().map(|f| f.to_uppercase().collect::<String>() + ch.as_str()).unwrap_or_default()
        }
    }
}

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

/// Reloads a few times so progress shows up on the home screen.
fn follow(ctx: HomeCtx) {
    spawn(async move {
        for _ in 0..8 {
            ctx.reload();
            sleep_ms(3_000).await;
        }
    });
}

fn install(ctx: HomeCtx, mut notice: Notice, id: String, name: String) {
    spawn(async move {
        let r = store_install(id).await.map_err(|e| err_msg(&e));
        notice.set(Some(r.map(|_| format!("{name} wird installiert – erscheint gleich auf dem Startbildschirm."))));
        follow(ctx);
    });
}

/// Address of an installed app for the chosen link mode.
fn app_url(ctx: &HomeCtx, id: &str) -> Option<String> {
    let mode = ctx.link_mode();
    match &*ctx.tiles.read() {
        Some(Ok(t)) => t.iter().find(|t| t.id == id).and_then(|t| t.url(&mode)),
        _ => None,
    }
}

#[component]
pub fn StoreWindow(updates: bool) -> Element {
    let mut tab = use_signal(move || if updates { Tab::Updates } else { Tab::Discover });
    let mut selected = use_signal(|| None::<String>);
    let notice: Notice = use_signal(|| None);
    let ctx = use_context::<HomeCtx>();
    let n_updates = installed_map(&ctx).values().filter(|a| a.update_available).count();
    let seg = |t: Tab| if tab() == t { "rounded-full bg-white px-4 py-1.5 text-sm font-medium text-black shadow" } else { "rounded-full px-4 py-1.5 text-sm text-white/70 hover:text-white" };

    rsx! {
        HomeWindow { title: "App Store",
            if let Some(id) = selected() {
                Details { key: "{id}", id, notice, onback: move |_| selected.set(None), onopen: move |id| selected.set(Some(id)) }
            } else {
                div { class: "mb-5 flex justify-center",
                    div { class: "inline-flex gap-1 rounded-full bg-white/[0.07] p-1",
                        button { class: seg(Tab::Discover), onclick: move |_| tab.set(Tab::Discover), "Entdecken" }
                        button { class: seg(Tab::Updates), onclick: move |_| tab.set(Tab::Updates),
                            "Updates"
                            if n_updates > 0 { span { class: "ml-1.5 rounded-full bg-sky-500 px-1.5 text-xs text-white", "{n_updates}" } }
                        }
                        button { class: seg(Tab::Installed), onclick: move |_| tab.set(Tab::Installed), "Installiert" }
                    }
                }
                NoticeLine { notice }
                match tab() {
                    Tab::Discover => rsx! { Discover { notice, onopen: move |id| selected.set(Some(id)) } },
                    Tab::Updates => rsx! { Updates { notice } },
                    Tab::Installed => rsx! { Installed { notice, onopen: move |id| selected.set(Some(id)) } },
                }
            }
        }
    }
}

#[component]
fn NoticeLine(notice: Notice) -> Element {
    rsx! {
        match notice() {
            Some(Ok(m)) => rsx! { div { class: "mb-4 rounded-2xl bg-emerald-500/15 px-4 py-3 text-sm text-emerald-100", "{m}" } },
            Some(Err(m)) => rsx! { div { class: "mb-4 rounded-2xl bg-red-500/15 px-4 py-3 text-sm text-red-100", "{m}" } },
            None => rsx! {},
        }
    }
}

#[component]
fn Discover(notice: Notice, onopen: EventHandler<String>) -> Element {
    let mut query = use_signal(String::new);
    let mut category = use_signal(|| None::<String>);
    let ctx = use_context::<HomeCtx>();
    let results = use_resource(move || async move {
        let q = query();
        if q.trim().is_empty() {
            return None;
        }
        sleep_ms(250).await;
        Some(store_search(q, 50).await)
    });
    let installed = installed_map(&ctx);
    // Without a search the preloaded list is shown right away.
    let list = if query().trim().is_empty() { ctx.store.read().clone() } else { results.read().clone().flatten() };
    let mut cats: Vec<String> = match &list {
        Some(Ok(l)) => l.iter().map(|a| a.category.clone()).filter(|c| !c.is_empty()).collect(),
        _ => vec![],
    };
    cats.sort();
    cats.dedup();
    let browsing = query().trim().is_empty() && category().is_none();

    rsx! {
        div { class: "relative mb-5",
            Search { class: "pointer-events-none absolute left-4 top-1/2 size-4 -translate-y-1/2 text-white/40" }
            input {
                class: "h-12 w-full rounded-2xl bg-white/[0.07] pl-11 pr-4 text-[15px] text-white placeholder:text-white/40 outline-none ring-1 ring-white/10 focus:ring-white/30",
                placeholder: "Spiele, Medien, Smart Home …",
                value: "{query}",
                oninput: move |e| {
                    query.set(e.value());
                    category.set(None);
                },
            }
        }
        match list {
            None => rsx! {
                div { class: "mb-6 h-56 animate-pulse rounded-3xl bg-white/[0.05]" }
                div { class: "grid gap-3 sm:grid-cols-2 lg:grid-cols-3",
                    for i in 0..6 { div { key: "{i}", class: "h-24 animate-pulse rounded-2xl bg-white/[0.05]" } }
                }
            },
            Some(Err(e)) => rsx! { p { class: "text-sm text-red-300", {err_msg(&e)} } },
            Some(Ok(l)) => {
                let featured = l.iter().find(|a| a.compatible && !installed.contains_key(&a.id)).cloned();
                rsx! {
                    if browsing {
                        if let Some(f) = featured {
                            Hero { app: f, onopen }
                        }
                    }
                    if cats.len() > 1 && query().trim().is_empty() {
                        div { class: "-mx-1 mb-6 flex gap-2 overflow-x-auto px-1 pb-1",
                            button {
                                class: if category().is_none() { "shrink-0 rounded-full bg-white px-4 py-2 text-sm font-medium text-black" } else { "shrink-0 rounded-full bg-white/[0.07] px-4 py-2 text-sm text-white/80 hover:bg-white/15" },
                                onclick: move |_| category.set(None),
                                "Alle"
                            }
                            for c in cats.clone() {
                                button {
                                    key: "{c}",
                                    class: if category().as_deref() == Some(c.as_str()) { "shrink-0 rounded-full bg-white px-4 py-2 text-sm font-medium text-black" } else { "shrink-0 rounded-full bg-white/[0.07] px-4 py-2 text-sm text-white/80 hover:bg-white/15" },
                                    onclick: { let c = c.clone(); move |_| category.set(Some(c.clone())) },
                                    {category_label(&c)}
                                }
                            }
                        }
                    }
                    if browsing {
                        for c in cats {
                            {
                                let apps: Vec<StoreApp> = l.iter().filter(|a| a.category == c).cloned().collect();
                                rsx! {
                                    section { key: "{c}", class: "mb-7",
                                        div { class: "mb-3 flex items-baseline justify-between",
                                            h3 { class: "text-lg font-semibold", {category_label(&c)} }
                                            if apps.len() > 6 {
                                                button { class: "text-sm text-sky-300 hover:text-sky-200", onclick: { let c = c.clone(); move |_| category.set(Some(c.clone())) }, "Alle anzeigen" }
                                            }
                                        }
                                        div { class: "grid gap-x-6 gap-y-1 sm:grid-cols-2 lg:grid-cols-3",
                                            for a in apps.into_iter().take(6) {
                                                StoreRow { key: "{a.id}", state: installed.get(&a.id).map(|i| i.state.clone()), app: a, notice, onopen }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        {
                            let l: Vec<StoreApp> = l.into_iter().filter(|a| category().is_none_or(|c| a.category == c)).collect();
                            rsx! {
                                if l.is_empty() {
                                    p { class: "py-12 text-center text-white/50", "Keine Apps gefunden." }
                                }
                                div { class: "grid gap-x-6 gap-y-1 sm:grid-cols-2 lg:grid-cols-3",
                                    for a in l {
                                        StoreRow { key: "{a.id}", state: installed.get(&a.id).map(|i| i.state.clone()), app: a, notice, onopen }
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
fn Hero(app: StoreApp, onopen: EventHandler<String>) -> Element {
    let id = app.id.clone();
    rsx! {
        button {
            class: "relative mb-6 flex w-full items-end overflow-hidden rounded-3xl bg-[#1c1c1e] p-6 text-left md:h-64 md:p-8",
            onclick: move |_| onopen.call(id.clone()),
            img { class: "pointer-events-none absolute inset-0 h-full w-full scale-150 object-cover opacity-50 blur-3xl", src: "/appicon/{app.id}", alt: "" }
            div { class: "absolute inset-0 bg-gradient-to-t from-black/70 via-black/20 to-transparent" }
            div { class: "relative flex w-full flex-col gap-4 md:flex-row md:items-end",
                AppIcon { id: app.id.clone(), name: app.name.clone(), class: "size-20 shadow-2xl md:size-24" }
                div { class: "min-w-0 flex-1",
                    div { class: "text-xs font-semibold uppercase tracking-wider text-white/60", "Empfohlen · {category_label(&app.category)}" }
                    div { class: "text-2xl font-bold md:text-3xl", "{app.name}" }
                    div { class: "line-clamp-2 text-white/75", "{app.tagline}" }
                }
                span { class: BIG_BLUE, "Ansehen" }
            }
        }
    }
}

#[component]
fn StoreRow(app: StoreApp, state: Option<String>, notice: Notice, onopen: EventHandler<String>) -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut busy = use_signal(|| false);
    let id = app.id.clone();
    let can_install = app.compatible && app.dependencies.is_empty();
    let url = state.as_ref().and_then(|_| app_url(&ctx, &app.id));
    rsx! {
        div { class: "flex items-center gap-4 border-b border-white/[0.06] py-3",
            button { class: "flex min-w-0 flex-1 items-center gap-4 text-left", onclick: move |_| onopen.call(id.clone()),
                AppIcon { id: app.id.clone(), name: app.name.clone(), class: "size-16 shrink-0 shadow-lg" }
                div { class: "min-w-0",
                    div { class: "truncate font-semibold", "{app.name}" }
                    div { class: "line-clamp-2 text-sm text-white/55", "{app.tagline}" }
                }
            }
            if let Some(s) = &state {
                if let Some(u) = url {
                    button { class: PILL_GREY, onclick: move |_| open_url(&u), "Öffnen" }
                } else {
                    span { class: "text-xs text-white/50", {state_label(s).to_string()} }
                }
            } else if busy() {
                span { class: "text-xs text-white/50", "Startet…" }
            } else if can_install {
                button {
                    class: PILL_GREY,
                    onclick: {
                        let (id, name) = (app.id.clone(), app.name.clone());
                        move |_| {
                            busy.set(true);
                            install(ctx, notice, id.clone(), name.clone());
                        }
                    },
                    "Laden"
                }
            } else {
                button { class: PILL_GREY, onclick: { let id = app.id.clone(); move |_| onopen.call(id.clone()) }, "Ansehen" }
            }
        }
    }
}

#[component]
fn Details(id: String, notice: Notice, onback: EventHandler<()>, onopen: EventHandler<String>) -> Element {
    let ctx = use_context::<HomeCtx>();
    let id_r = id.clone();
    let details = use_resource(move || store_details(id_r.clone()));
    let installed = installed_map(&ctx);
    let mut busy = use_signal(|| false);
    let state = installed.get(&id).map(|a| a.state.clone());
    let url = app_url(&ctx, &id);

    rsx! {
        button { class: "mb-4 inline-flex items-center gap-1.5 text-sm text-sky-300 hover:text-sky-200", onclick: move |_| onback.call(()),
            ArrowLeft { class: "size-4" }
            "App Store"
        }
        NoticeLine { notice }
        match details.read().clone() {
            None => rsx! { div { class: "h-56 animate-pulse rounded-3xl bg-white/[0.05]" } },
            Some(Err(e)) => rsx! { p { class: "text-sm text-red-300", {err_msg(&e)} } },
            Some(Ok(d)) => {
                let missing: Vec<String> = d.dependencies.iter().filter(|dep| !installed.contains_key(*dep)).cloned().collect();
                let d2 = d.clone();
                rsx! {
                    div { class: "relative overflow-hidden rounded-3xl bg-[#1c1c1e] p-6 md:p-8",
                        img { class: "pointer-events-none absolute inset-0 h-full w-full scale-150 object-cover opacity-40 blur-3xl", src: "/appicon/{d.id}", alt: "" }
                        div { class: "absolute inset-0 bg-gradient-to-b from-transparent to-black/60" }
                        div { class: "relative flex flex-col gap-5 sm:flex-row sm:items-center",
                            AppIcon { id: d.id.clone(), name: d.name.clone(), class: "size-28 shrink-0 shadow-2xl" }
                            div { class: "min-w-0 flex-1",
                                h3 { class: "text-3xl font-bold", "{d.name}" }
                                p { class: "text-white/70", "{d.tagline}" }
                                div { class: "mt-4 flex flex-wrap items-center gap-2",
                                    if let Some(s) = &state {
                                        if let Some(u) = url.clone() {
                                            button { class: BIG_BLUE, onclick: move |_| open_url(&u), "Öffnen" }
                                        } else {
                                            span { class: BIG_GREY, {state_label(s).to_string()} }
                                        }
                                        ConfirmButton {
                                            label: "Entfernen",
                                            confirm_label: "Daten werden gelöscht – sicher?",
                                            onconfirm: {
                                                let id = d.id.clone();
                                                move |_| {
                                                    let id = id.clone();
                                                    let mut notice = notice;
                                                    spawn(async move {
                                                        let r = store_uninstall(id).await.map_err(|e| err_msg(&e));
                                                        notice.set(Some(r.map(|_| "App wird entfernt.".into())));
                                                        follow(ctx);
                                                    });
                                                }
                                            },
                                            Trash2 { class: "size-4" }
                                        }
                                    } else {
                                        button {
                                            class: BIG_BLUE,
                                            disabled: busy() || !d.compatible || !missing.is_empty(),
                                            onclick: move |_| {
                                                busy.set(true);
                                                install(ctx, notice, d2.id.clone(), d2.name.clone());
                                            },
                                            Download { class: "size-4" }
                                            if busy() { "Wird gestartet…" } else { "Installieren" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    InfoStrip { d: d.clone() }
                    if !d.compatible {
                        p { class: "mt-4 rounded-2xl bg-amber-500/10 px-4 py-3 text-sm text-amber-200", "Diese App läuft nicht auf deinem Gerät." }
                    }
                    if !d.dependencies.is_empty() {
                        div { class: "mt-5",
                            h4 { class: "mb-2 font-semibold", "Benötigt" }
                            div { class: "flex flex-wrap gap-2",
                                for dep in d.dependencies.clone() {
                                    button {
                                        key: "{dep}",
                                        class: "flex items-center gap-2 rounded-full bg-white/[0.07] py-1 pl-1 pr-3 text-sm hover:bg-white/15",
                                        onclick: { let dep = dep.clone(); move |_| onopen.call(dep.clone()) },
                                        AppIcon { id: dep.clone(), name: dep.clone(), class: "size-7" }
                                        "{dep}"
                                        if installed.contains_key(&dep) { Check { class: "size-3.5 text-emerald-300" } } else { span { class: "text-xs text-amber-300", "zuerst installieren" } }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "mt-6 grid gap-8 lg:grid-cols-[3fr_2fr]",
                        div {
                            h4 { class: "mb-2 text-lg font-semibold", "Über die App" }
                            p { class: "whitespace-pre-line leading-relaxed text-white/70", "{d.description}" }
                        }
                        if !d.release_notes.is_empty() {
                            div {
                                h4 { class: "mb-2 text-lg font-semibold", "Neu in Version {d.version}" }
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
fn InfoStrip(d: StoreDetails) -> Element {
    let items: Vec<(&str, String)> = [
        Some(("Version", d.version.clone())),
        Some(("Kategorie", category_label(&d.category))),
        d.install_size.map(|s| ("Größe", fmt_bytes(s))),
        Some(("Kompatibel", if d.compatible { "Ja".to_string() } else { "Nein".to_string() })),
    ]
    .into_iter()
    .flatten()
    .collect();
    rsx! {
        div { class: "mt-4 grid grid-cols-2 divide-white/10 rounded-2xl bg-white/[0.04] sm:grid-cols-4 sm:divide-x",
            for (k, v) in items {
                div { key: "{k}", class: "px-4 py-3 text-center",
                    div { class: "text-[11px] uppercase tracking-wider text-white/40", "{k}" }
                    div { class: "mt-0.5 truncate font-semibold", "{v}" }
                }
            }
        }
    }
}

#[component]
fn Updates(notice: Notice) -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut notice = notice;
    let mut busy = use_signal(|| false);
    let mut list: Vec<AppInfo> = installed_map(&ctx).into_values().filter(|a| a.update_available || a.state == "updating").collect();
    list.sort_by_key(|a| a.name.to_lowercase());
    let any = list.iter().any(|a| a.update_available);

    rsx! {
        div { class: "mb-4 flex items-center justify-between gap-3",
            h3 { class: "text-lg font-semibold", if list.is_empty() { "Alles aktuell" } else { "Verfügbare Updates" } }
            div { class: "flex gap-2",
                button { class: BIG_GREY, onclick: move |_| ctx.reload(), RotateCw { class: "size-4" } "Prüfen" }
                if any {
                    button {
                        class: BIG_BLUE,
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
        if list.is_empty() {
            div { class: "flex flex-col items-center gap-2 py-16 text-white/50",
                Check { class: "size-10 text-emerald-300" }
                "Alle Apps sind auf dem neuesten Stand."
            }
        }
        div { class: "flex flex-col",
            for a in list {
                div { key: "{a.id}", class: "flex items-center gap-4 border-b border-white/[0.06] py-3",
                    AppIcon { id: a.id.clone(), name: a.name.clone(), class: "size-14" }
                    div { class: "min-w-0 flex-1",
                        div { class: "truncate font-semibold", "{a.name}" }
                        div { class: "text-sm text-white/50", "Installiert: {a.version}" }
                    }
                    if a.state == "updating" {
                        span { class: "text-sm text-sky-300", "Wird aktualisiert…" }
                    } else {
                        button {
                            class: PILL_BLUE,
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
fn Installed(notice: Notice, onopen: EventHandler<String>) -> Element {
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
        h3 { class: "mb-3 text-lg font-semibold", "{list.len()} Apps installiert" }
        div { class: "grid gap-x-6 lg:grid-cols-2",
            for a in list {
                {
                    let running = matches!(a.state.as_str(), "ready" | "running");
                    let id = a.id.clone();
                    rsx! {
                        div { key: "{a.id}", class: "flex items-center gap-3 border-b border-white/[0.06] py-3",
                            button { class: "flex min-w-0 flex-1 items-center gap-3 text-left", onclick: { let id = id.clone(); move |_| onopen.call(id.clone()) },
                                AppIcon { id: a.id.clone(), name: a.name.clone(), class: "size-12" }
                                div { class: "min-w-0",
                                    div { class: "truncate font-semibold", "{a.name}" }
                                    div { class: if running { "text-sm text-emerald-300" } else { "text-sm text-white/50" }, "{state_label(&a.state)} · {a.version}" }
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
