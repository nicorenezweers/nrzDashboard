//! Spotlight search (⌘K): installed apps, actions, App Store and files.

use dioxus::prelude::*;
use icons::{FileText, Folder, Search, ShoppingBag};

use super::{HomeCtx, Win, open_win};
use crate::api::files::search_files;
use crate::api::home::store_search;
use crate::app::Route;
use crate::pages::board::widgets::{AppIcon, browser_url};
use crate::utils::sleep_ms;

#[derive(Clone, PartialEq)]
enum Action {
    Url(String),
    Win(Win),
    Route(Route),
}

#[derive(Clone, PartialEq)]
struct Item {
    title: String,
    subtitle: String,
    /// App id for the icon (installed or App Store app).
    app: Option<String>,
    kind: &'static str,
    action: Action,
}

fn actions() -> Vec<(&'static str, &'static str, Action)> {
    vec![
        ("App Store", "Apps entdecken und installieren", Action::Win(Win::Store)),
        ("Updates", "App-Updates installieren", Action::Win(Win::Updates)),
        ("Aktivität", "CPU, RAM und Speicher", Action::Win(Win::Activity)),
        ("Einstellungen", "Gerät, Updates, Netzwerk", Action::Win(Win::Settings)),
        ("MCP-Konsole", "Alle umbrelOS-Werkzeuge ausführen", Action::Win(Win::Console)),
        ("Dateien", "Dateimanager", Action::Route(Route::FilesPage { path: "/Home".into(), msg: String::new() })),
        ("Board", "Widgets und Dashboards", Action::Route(Route::BoardPage {})),
        ("Monitore", "Uptime-Überwachung", Action::Route(Route::MonitorsPage {})),
        ("Agent", "KI-Agent für dein Umbrel", Action::Route(Route::AgentPage {})),
        ("System", "Verbindung, Logs, Integrationen", Action::Route(Route::SystemPage {})),
    ]
}

fn parent(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((p, _)) if !p.is_empty() => p.to_string(),
        _ => "/Home".into(),
    }
}

fn run(item: &Item, onclose: EventHandler<()>) {
    onclose.call(());
    match &item.action {
        Action::Url(u) => {
            #[cfg(feature = "web")]
            if let Some(w) = web_sys::window() {
                let _ = w.open_with_url_and_target(u, "_blank");
            }
            #[cfg(not(feature = "web"))]
            let _ = u;
        }
        Action::Win(w) => open_win(*w),
        Action::Route(r) => {
            navigator().push(r.clone());
        }
    }
}

#[component]
pub fn Spotlight(onclose: EventHandler<()>) -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut query = use_signal(String::new);
    let mut sel = use_signal(|| 0usize);

    let store = use_resource(move || async move {
        let q = query();
        if q.trim().len() < 2 {
            return Vec::new();
        }
        sleep_ms(250).await;
        store_search(q, 5).await.unwrap_or_default()
    });
    let files = use_resource(move || async move {
        let q = query();
        if q.trim().len() < 3 {
            return Vec::new();
        }
        sleep_ms(350).await;
        search_files(q).await.unwrap_or_default()
    });

    let q = query().trim().to_lowercase();
    let mut items: Vec<Item> = Vec::new();
    if let Some(Ok(tiles)) = &*ctx.tiles.read() {
        for t in tiles.iter().filter(|t| !t.hidden && (q.is_empty() || t.name.to_lowercase().contains(&q) || t.id.contains(&q))) {
            let action = match &t.url {
                Some(u) => Action::Url(browser_url(u)),
                None => Action::Win(Win::Store),
            };
            items.push(Item { title: t.name.clone(), subtitle: "App öffnen".into(), app: Some(t.id.clone()), kind: "Apps", action });
        }
    }
    if q.is_empty() {
        items.truncate(6);
    }
    for (title, sub, action) in actions() {
        if q.is_empty() || title.to_lowercase().contains(&q) || sub.to_lowercase().contains(&q) {
            items.push(Item { title: title.into(), subtitle: sub.into(), app: None, kind: "Aktionen", action });
        }
    }
    if let Some(list) = &*store.read() {
        for a in list.iter().filter(|a| !a.installed) {
            items.push(Item { title: a.name.clone(), subtitle: a.tagline.clone(), app: Some(a.id.clone()), kind: "App Store", action: Action::Win(Win::Store) });
        }
    }
    if let Some(list) = &*files.read() {
        for f in list.iter().take(6) {
            let dir = if f.is_dir() { f.path.clone() } else { parent(&f.path) };
            items.push(Item {
                title: f.name.clone(),
                subtitle: f.path.clone(),
                app: None,
                kind: if f.is_dir() { "Ordner" } else { "Dateien" },
                action: Action::Route(Route::FilesPage { path: dir, msg: String::new() }),
            });
        }
    }
    let count = items.len();
    let current = sel().min(count.saturating_sub(1));
    let items_key = items.clone();

    rsx! {
        div { class: "fixed inset-0 z-[60] flex items-start justify-center bg-black/40 px-3 pt-[12vh] backdrop-blur-sm", onclick: move |_| onclose.call(()),
            div {
                class: "w-full max-w-xl overflow-hidden rounded-3xl border border-white/10 bg-[#1a1a1a]/90 text-white shadow-2xl backdrop-blur-2xl",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center gap-3 border-b border-white/10 px-5",
                    Search { class: "size-5 text-white/50" }
                    input {
                        class: "h-14 flex-1 bg-transparent text-lg text-white placeholder:text-white/35 outline-none",
                        placeholder: "Apps, Aktionen, App Store, Dateien…",
                        value: "{query}",
                        onmounted: move |e| async move {
                            let _ = e.set_focus(true).await;
                        },
                        oninput: move |e| {
                            query.set(e.value());
                            sel.set(0);
                        },
                        onkeydown: move |e: KeyboardEvent| match e.key() {
                            Key::ArrowDown => {
                                e.prevent_default();
                                if count > 0 { sel.set((current + 1) % count); }
                            }
                            Key::ArrowUp => {
                                e.prevent_default();
                                if count > 0 { sel.set((current + count - 1) % count); }
                            }
                            Key::Enter => {
                                if let Some(it) = items_key.get(current) {
                                    run(it, onclose);
                                }
                            }
                            Key::Escape => onclose.call(()),
                            _ => {}
                        },
                    }
                }
                div { class: "max-h-[60vh] overflow-y-auto p-2",
                    if items.is_empty() {
                        p { class: "px-4 py-8 text-center text-sm text-white/40", "Keine Treffer" }
                    }
                    for (i, it) in items.into_iter().enumerate() {
                        {
                            let it2 = it.clone();
                            rsx! {
                                button {
                                    key: "{it.kind}-{i}",
                                    class: if i == current { "flex w-full items-center gap-3 rounded-xl bg-white/10 px-3 py-2 text-left" } else { "flex w-full items-center gap-3 rounded-xl px-3 py-2 text-left hover:bg-white/[0.06]" },
                                    onmouseenter: move |_| sel.set(i),
                                    onclick: move |_| run(&it2, onclose),
                                    if let Some(id) = &it.app {
                                        AppIcon { id: id.clone(), name: it.title.clone(), class: "size-8" }
                                    } else {
                                        span { class: "flex size-8 items-center justify-center rounded-lg bg-white/10 text-white/70",
                                            match it.kind {
                                                "Ordner" => rsx! { Folder { class: "size-4" } },
                                                "Dateien" => rsx! { FileText { class: "size-4" } },
                                                _ => rsx! { ShoppingBag { class: "size-4" } },
                                            }
                                        }
                                    }
                                    div { class: "min-w-0 flex-1",
                                        div { class: "truncate text-sm", "{it.title}" }
                                        div { class: "truncate text-xs text-white/45", "{it.subtitle}" }
                                    }
                                    span { class: "shrink-0 text-[11px] text-white/35", "{it.kind}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
