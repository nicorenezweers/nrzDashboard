//! App menu (right click / long press): open as user or admin, edit links, start/stop, update, hide.

use dioxus::prelude::*;
use icons::{ArrowUp, EyeOff, Globe, Pencil, Play, RotateCw, ShieldCheck, Square};

use super::{AppIcon, HomeCtx, open_url};
use crate::api::app_action;
use crate::api::apps::{AppTile, app_set_hidden, browser_url};
use crate::utils::{err_msg, sleep_ms};

#[derive(Clone, PartialEq)]
pub struct MenuReq {
    pub tile: AppTile,
    pub x: f64,
    pub y: f64,
}

fn viewport() -> (f64, f64) {
    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    if let Some(w) = web_sys::window() {
        let wd = w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1200.0);
        let ht = w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(800.0);
        return (wd, ht);
    }
    (1200.0, 800.0)
}

fn host(url: &str) -> String {
    url.split_once("://").map_or(url, |(_, r)| r).trim_end_matches('/').to_string()
}

#[component]
pub fn AppMenu() -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut menu = ctx.menu;
    let mut toast = use_signal(|| None::<Result<String, String>>);

    let mut act = move |id: String, action: &'static str| {
        menu.set(None);
        spawn(async move {
            let r = app_action(id, action.to_string()).await.map_err(|e| err_msg(&e));
            toast.set(Some(r.map(|_| match action {
                "start" => "Wird gestartet…".to_string(),
                "stop" => "Wird gestoppt…".to_string(),
                "restart" => "Wird neu gestartet…".to_string(),
                _ => "Update läuft…".to_string(),
            })));
            for _ in 0..4 {
                sleep_ms(2_500).await;
                ctx.reload();
            }
            toast.set(None);
        });
    };

    rsx! {
        if let Some(r) = menu() {
            {
                let (vw, vh) = viewport();
                let x = r.x.min(vw - 268.0).max(8.0);
                let y = r.y.min(vh - 330.0).max(8.0);
                let t = r.tile.clone();
                let running = t.is_running();
                let fallback = (t.user_url.is_none() && t.admin_url.is_none()).then(|| t.fallback_url.as_deref().map(browser_url)).flatten();
                rsx! {
                    div { class: "fixed inset-0 z-[80]", onclick: move |_| menu.set(None), oncontextmenu: move |e| { e.prevent_default(); menu.set(None); } }
                    div {
                        class: "fixed z-[81] w-64 rounded-2xl border border-white/10 bg-[#1c1c1e]/90 p-1.5 text-sm text-white shadow-2xl backdrop-blur-2xl",
                        style: "left: {x}px; top: {y}px;",
                        div { class: "flex items-center gap-3 px-2.5 py-2",
                            AppIcon { id: t.id.clone(), name: t.name.clone(), class: "size-9" }
                            div { class: "min-w-0",
                                div { class: "truncate font-medium", "{t.name}" }
                                div { class: "text-xs text-white/50", if running { "Läuft" } else { "Gestoppt" } }
                            }
                        }
                        div { class: "my-1 h-px bg-white/10" }
                        if running {
                            if let Some(u) = t.user_url.clone() {
                                Item { onclick: move |_| { menu.set(None); open_url(&u); }, Globe { class: "size-4" }
                                    span { class: "flex min-w-0 flex-col", "Öffnen" span { class: "truncate text-xs text-white/45", {host(t.user_url.as_deref().unwrap_or_default())} } }
                                }
                            }
                            if let Some(u) = t.admin_url.clone() {
                                Item { onclick: move |_| { menu.set(None); open_url(&u); }, ShieldCheck { class: "size-4" }
                                    span { class: "flex min-w-0 flex-col", "Als Admin öffnen" span { class: "truncate text-xs text-white/45", {host(t.admin_url.as_deref().unwrap_or_default())} } }
                                }
                            }
                            if let Some(u) = fallback {
                                Item { onclick: move |_| { menu.set(None); open_url(&u); }, Globe { class: "size-4" } "Öffnen" }
                            }
                            div { class: "my-1 h-px bg-white/10" }
                            Item { onclick: { let id = t.id.clone(); move |_| act(id.clone(), "restart") }, RotateCw { class: "size-4" } "Neu starten" }
                            Item { onclick: { let id = t.id.clone(); move |_| act(id.clone(), "stop") }, Square { class: "size-4" } "Stoppen" }
                        } else if t.state == "stopped" {
                            Item { onclick: { let id = t.id.clone(); move |_| act(id.clone(), "start") }, Play { class: "size-4" } "Starten" }
                        }
                        Item {
                            onclick: {
                                let t = t.clone();
                                move |_| {
                                    menu.set(None);
                                    let mut editing = ctx.editing;
                                    editing.set(Some(t.clone()));
                                }
                            },
                            Pencil { class: "size-4" }
                            "Bearbeiten"
                        }
                        if t.update_available {
                            Item { onclick: { let id = t.id.clone(); move |_| act(id.clone(), "update") }, ArrowUp { class: "size-4" } "Aktualisieren" }
                        }
                        Item {
                            onclick: {
                                let id = t.id.clone();
                                move |_| {
                                    menu.set(None);
                                    let id = id.clone();
                                    spawn(async move {
                                        if let Err(e) = app_set_hidden(id, true).await {
                                            toast.set(Some(Err(err_msg(&e))));
                                        }
                                        ctx.reload();
                                    });
                                }
                            },
                            EyeOff { class: "size-4" }
                            "Ausblenden"
                        }
                    }
                }
            }
        }
        if let Some(t) = toast() {
            div { class: "fixed bottom-28 left-1/2 z-[90] -translate-x-1/2 rounded-full border border-white/10 bg-black/70 px-4 py-2 text-sm text-white backdrop-blur-xl",
                match t { Ok(m) => m, Err(m) => m }
            }
        }
    }
}

#[component]
fn Item(onclick: EventHandler<MouseEvent>, children: Element) -> Element {
    rsx! {
        button {
            class: "flex w-full items-center gap-3 rounded-xl px-2.5 py-2 text-left hover:bg-white/10 [&>svg]:shrink-0 [&>svg]:text-white/70",
            onclick: move |e| onclick.call(e),
            {children}
        }
    }
}
