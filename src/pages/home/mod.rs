//! Home screen in the style of the umbrelOS desktop: wallpaper, greeting, paged app grid,
//! live CPU/RAM/storage, search (⌘K), dock and windows for App Store, activity and settings.

mod activity;
mod edit;
mod live;
mod menu;
mod monitor;
mod settings;
mod spotlight;
mod store;

use dioxus::prelude::*;
use icons::{Activity, ArrowUp, ChevronLeft, ChevronRight, Cpu, FolderOpen, HardDrive, LogOut, MemoryStick, RefreshCw, Settings, ShoppingBag, X};

use crate::api::apps::{AppTile, app_tiles};
use crate::api::get_overview;
use crate::api::home::monitor_view;
use crate::api::settings::{SettingsView, settings_get};
use crate::app::Route;
use crate::model::{MonitorView, Overview};
use live::{INIT_ID, Live, use_init, use_live};
use crate::utils::{fmt_bytes, pct, sleep_ms};

use menu::{AppMenu, MenuReq};

pub const WALLPAPER: Asset = asset!("/assets/wallpaper.jpg");

/// CSS background with the wallpaper.
pub fn wallpaper_style() -> String {
    format!("background: #111 url('{WALLPAPER}') center / cover no-repeat;")
}

/// Windows above the home screen (`/?w=…`, so the back button closes them).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Win {
    Store,
    Updates,
    Activity,
    Settings,
    Files,
}

impl Win {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Store => "store",
            Self::Updates => "updates",
            Self::Activity => "activity",
            Self::Settings => "settings",
            Self::Files => "files",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "store" => Self::Store,
            "updates" => Self::Updates,
            "activity" => Self::Activity,
            "settings" => Self::Settings,
            "files" => Self::Files,
            _ => return None,
        })
    }
}

pub fn open_win(win: Win) {
    let route = if win == Win::Files { Route::files("/Home") } else { Route::HomePage { w: win.key().to_string(), path: String::new(), msg: String::new() } };
    navigator().push(route);
}

pub fn close_win() {
    navigator().replace(Route::home());
}

/// Data shared by the home screen and its windows.
#[derive(Clone, Copy)]
pub struct HomeCtx {
    pub overview: Live<Result<Overview, ServerFnError>>,
    pub tiles: Live<Result<Vec<AppTile>, ServerFnError>>,
    pub settings: Live<Result<SettingsView, ServerFnError>>,
    pub monitor: Live<Result<MonitorView, ServerFnError>>,
    /// App Store list, loaded in the background right after the page (ready when the store opens).
    pub store: Resource<Result<Vec<crate::api::home::StoreApp>, ServerFnError>>,
    /// Bump to reload everything at once.
    pub refresh: Signal<u32>,
    pub menu: Signal<Option<MenuReq>>,
    /// App whose name/links are being edited.
    pub editing: Signal<Option<AppTile>>,
}

impl HomeCtx {
    pub fn reload(mut self) {
        self.refresh += 1;
    }

    /// `user` or `admin`.
    pub fn link_mode(&self) -> String {
        match &*self.settings.read() {
            Some(Ok(s)) => s.link_mode.clone(),
            _ => "user".into(),
        }
    }
}

/// App icon from `/appicon/{id}` (fetched by the server), first letter as fallback.
#[component]
pub fn AppIcon(id: String, name: String, #[props(default = "size-12".to_string())] class: String) -> Element {
    let mut failed = use_signal(|| false);
    let letter: String = name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into());
    rsx! {
        if failed() {
            div { class: "{class} flex items-center justify-center rounded-[24%] bg-gradient-to-b from-[#2c2c2e] to-[#1c1c1e] text-lg font-semibold text-white/80",
                "{letter}"
            }
        } else {
            img {
                class: "{class} rounded-[24%] bg-[#1c1c1e] object-cover",
                src: "/appicon/{id}",
                alt: "",
                loading: "lazy",
                draggable: "false",
                onerror: move |_| failed.set(true),
            }
        }
    }
}

fn now_hour() -> u32 {
    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    {
        js_sys::Date::new_0().get_hours()
    }
    #[cfg(not(all(feature = "web", target_arch = "wasm32")))]
    12
}

fn greeting(hour: u32) -> &'static str {
    match hour {
        5..=10 => "Guten Morgen",
        11..=17 => "Guten Tag",
        18..=22 => "Guten Abend",
        _ => "Gute Nacht",
    }
}

/// Grid size (columns, rows) for a viewport.
fn grid_dims(w: f64, h: f64) -> (usize, usize) {
    let cols = if w >= 1024.0 {
        6
    } else if w >= 640.0 {
        5
    } else {
        4
    };
    let (row_h, reserved, max_rows) = if w >= 768.0 { (132.0, 470.0, 4) } else { (104.0, 400.0, 6) };
    let rows = (((h - reserved) / row_h).floor() as i64).clamp(2, max_rows) as usize;
    (cols, rows)
}

const KEY_JS: &str = r#"
if (window.__nrzHomeKey) window.removeEventListener('keydown', window.__nrzHomeKey);
window.__nrzHomeKey = (e) => {
  const tag = (document.activeElement && document.activeElement.tagName) || '';
  const typing = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); dioxus.send('search'); }
  else if (e.key === 'Escape') dioxus.send('esc');
  else if (!typing && e.key === 'ArrowLeft') dioxus.send('left');
  else if (!typing && e.key === 'ArrowRight') dioxus.send('right');
};
window.addEventListener('keydown', window.__nrzHomeKey);
"#;

const RESIZE_JS: &str = r#"
if (window.__nrzHomeResize) window.removeEventListener('resize', window.__nrzHomeResize);
window.__nrzHomeResize = () => dioxus.send([window.innerWidth, window.innerHeight]);
window.addEventListener('resize', window.__nrzHomeResize);
window.__nrzHomeResize();
"#;

/// Current `w` query parameter (read inside the key listener).
fn current_w() -> String {
    match router().current::<Route>() {
        Route::HomePage { w, .. } => w,
        _ => String::new(),
    }
}

#[component]
pub fn HomePage(w: String, path: String, msg: String) -> Element {
    let win = Win::parse(&w);
    let refresh = use_signal(|| 0u32);
    let mut menu = use_signal(|| None::<MenuReq>);
    let editing = use_signal(|| None::<AppTile>);

    // Start values come with the page (server cache), then refresh in the background.
    let (init_raw, init) = use_init();
    let mut overview = use_live(refresh, init.overview.map(Ok), get_overview);
    let mut tiles = use_live(refresh, init.tiles.map(Ok), app_tiles);
    let settings = use_live(refresh, init.settings.map(Ok), settings_get);
    let mut monitor = use_live(refresh, init.monitor.map(|m| m.map_err(ServerFnError::new)), monitor_view);
    let store = use_resource(move || {
        let _ = refresh();
        crate::api::home::store_search(String::new(), 50)
    });
    let ctx = use_context_provider(|| HomeCtx { overview, tiles, settings, monitor, store, refresh, menu, editing });

    // Live values (answered from the server cache): resources and containers every 3 s, apps every 9 s.
    use_future(move || async move {
        let mut n = 0u32;
        loop {
            sleep_ms(3_000).await;
            n += 1;
            overview.restart();
            monitor.restart();
            if n % 3 == 0 {
                tiles.restart();
            }
        }
    });

    let mut hour = use_signal(|| None::<u32>);
    use_effect(move || hour.set(Some(now_hour())));
    let mut viewport = use_signal(|| (1440.0f64, 900.0f64));
    let mut page = use_signal(|| 0usize);
    let mut spotlight = use_signal(|| false);
    let mut spinning = use_signal(|| false);

    use_future(move || async move {
        let mut ev = document::eval(RESIZE_JS);
        while let Ok(v) = ev.recv::<(f64, f64)>().await {
            viewport.set(v);
        }
    });
    use_future(move || async move {
        let mut ev = document::eval(KEY_JS);
        while let Ok(k) = ev.recv::<String>().await {
            let open = Win::parse(&current_w()).is_some();
            match k.as_str() {
                "search" => spotlight.toggle(),
                "esc" if menu.peek().is_some() => menu.set(None),
                "esc" if spotlight() => spotlight.set(false),
                "esc" if open && current_w() != "files" => close_win(),
                "left" if !spotlight() && !open => page.set(page().saturating_sub(1)),
                "right" if !spotlight() && !open => page.set(page() + 1),
                _ => {}
            }
        }
    });

    let (cols, rows) = grid_dims(viewport().0, viewport().1);
    let per_page = cols * rows;
    let visible: Vec<AppTile> = match &*tiles.read() {
        Some(Ok(list)) => list.iter().filter(|t| !t.hidden && t.id != "nrz-dashboard").cloned().collect(),
        _ => Vec::new(),
    };
    let pages = visible.len().div_ceil(per_page).max(1);
    if page() >= pages {
        page.set(pages - 1);
    }

    let name = match &*settings.read() {
        Some(Ok(s)) => s.name.clone(),
        _ => String::new(),
    };
    let hello = match hour() {
        Some(h) if name.trim().is_empty() => format!("{}.", greeting(h)),
        Some(h) => format!("{}, {}.", greeting(h), name.trim()),
        None => "\u{00a0}".to_string(),
    };
    let loading = tiles.read().is_none();
    let tiles_err = match &*tiles.read() {
        Some(Err(e)) => Some(crate::utils::err_msg(e)),
        _ => None,
    };
    let mut drag_x = use_signal(|| None::<f64>);

    rsx! {
        // Start data for the browser (same text on server and client, see `use_init`).
        script { id: INIT_ID, r#type: "application/json", dangerous_inner_html: "{init_raw}" }
        div { class: "fixed inset-0 overflow-hidden select-none text-white", style: wallpaper_style(),
            div { class: "pointer-events-none absolute inset-0 bg-[radial-gradient(ellipse_at_center,transparent_30%,rgba(0,0,0,0.55)_100%)]" }

            // Top-right actions
            div { class: "absolute right-3 top-[max(env(safe-area-inset-top,0px),0.75rem)] z-20 flex items-center gap-2 md:right-5 md:top-5",
                GlassButton {
                    label: "Aktualisieren",
                    onclick: move |_| {
                        ctx.reload();
                        spinning.set(true);
                        spawn(async move {
                            sleep_ms(900).await;
                            spinning.set(false);
                        });
                    },
                    RefreshCw { class: if spinning() { "size-4 animate-spin" } else { "size-4" } }
                }
                a {
                    href: "/auth/logout",
                    class: "flex size-10 items-center justify-center rounded-full border border-white/10 bg-white/[0.07] text-white/80 backdrop-blur-xl transition hover:bg-white/15 hover:text-white",
                    title: "Abmelden",
                    "aria-label": "Abmelden",
                    LogOut { class: "size-4" }
                }
            }

            div { class: "relative z-10 flex h-full flex-col items-center pt-[calc(env(safe-area-inset-top,0px)+3.75rem)] md:pt-8",
                UmbrellaLogo {}
                h1 { class: "mt-3 text-center text-3xl font-bold tracking-tight drop-shadow-[0_2px_12px_rgba(0,0,0,0.5)] md:mt-4 md:text-5xl", "{hello}" }
                LiveBar {}

                div { class: "relative mt-4 flex w-full max-w-6xl flex-1 min-h-0 items-center md:mt-6",
                    if pages > 1 {
                        PageArrow { left: true, disabled: page() == 0, onclick: move |_| page.set(page().saturating_sub(1)) }
                    }
                    div {
                        class: "h-full w-full overflow-hidden touch-pan-y",
                        onpointerdown: move |e: PointerEvent| drag_x.set(Some(e.client_coordinates().x)),
                        onpointerup: move |e: PointerEvent| {
                            if let Some(x0) = drag_x.take() {
                                let dx = e.client_coordinates().x - x0;
                                if dx < -60.0 && page() + 1 < pages {
                                    page += 1;
                                } else if dx > 60.0 && page() > 0 {
                                    page -= 1;
                                }
                            }
                        },
                        if let Some(e) = tiles_err {
                            div { class: "flex h-full flex-col items-center justify-center gap-3 px-6 text-center",
                                p { class: "max-w-md text-sm text-white/80", "{e}" }
                                button { class: "rounded-full bg-white/15 px-4 py-2 text-sm hover:bg-white/25", onclick: move |_| open_win(Win::Settings), "Einstellungen öffnen" }
                            }
                        } else if loading {
                            div { class: "grid h-full content-center gap-y-6 px-4 md:px-16", style: "grid-template-columns: repeat({cols}, minmax(0, 1fr));",
                                for i in 0..per_page.min(12) {
                                    div { key: "{i}", class: "flex flex-col items-center gap-2",
                                        div { class: "size-14 animate-pulse rounded-[24%] bg-white/10 md:size-[4.5rem]" }
                                        div { class: "h-3 w-16 animate-pulse rounded bg-white/10" }
                                    }
                                }
                            }
                        } else {
                            div {
                                class: "flex h-full transition-transform duration-500 ease-[cubic-bezier(0.22,1,0.36,1)]",
                                style: "transform: translateX(-{page() * 100}%);",
                                for (pi, chunk) in visible.chunks(per_page).enumerate() {
                                    div {
                                        key: "{pi}",
                                        class: "grid h-full w-full shrink-0 content-center gap-y-5 px-2 md:gap-y-8 md:px-16",
                                        style: "grid-template-columns: repeat({cols}, minmax(0, 1fr));",
                                        for t in chunk.iter().cloned() {
                                            HomeTile { key: "{t.id}", tile: t }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if pages > 1 {
                        PageArrow { left: false, disabled: page() + 1 >= pages, onclick: move |_| page.set((page() + 1).min(pages - 1)) }
                    }
                }

                div { class: "flex h-6 items-center gap-1.5",
                    if pages > 1 {
                        for i in 0..pages {
                            button {
                                key: "{i}",
                                class: if i == page() { "h-1.5 w-8 rounded-full bg-white transition-all" } else { "h-1.5 w-4 rounded-full bg-white/30 transition-all hover:bg-white/50" },
                                "aria-label": "Seite {i + 1}",
                                onclick: move |_| page.set(i),
                            }
                        }
                    }
                }

                button {
                    class: "mt-3 flex items-center gap-3 rounded-full border border-white/10 bg-white/[0.07] py-2 pl-2.5 pr-4 text-[15px] text-white/90 shadow-lg backdrop-blur-xl transition hover:bg-white/[0.12]",
                    onclick: move |_| spotlight.set(true),
                    span { class: "size-6 rounded-full bg-[radial-gradient(circle_at_30%_30%,#d8fff1,#4fd1c5_45%,#2b6cb0_100%)] shadow-[0_0_12px_rgba(79,209,197,0.6)]" }
                    "Suche"
                    span { class: "hidden text-white/40 md:inline", "⌘K" }
                }

                Dock { active: win }
            }

            if let Some(w) = win {
                match w {
                    Win::Store => rsx! { store::StoreWindow { updates: false } },
                    Win::Updates => rsx! { store::StoreWindow { updates: true } },
                    Win::Activity => rsx! { activity::ActivityWindow {} },
                    Win::Settings => rsx! { settings::SettingsWindow {} },
                    Win::Files => rsx! { FilesWindow { path: path.clone(), msg: msg.clone() } },
                }
            }
            if spotlight() {
                spotlight::Spotlight { onclose: move |_| spotlight.set(false) }
            }
            AppMenu {}
            edit::EditDialog {}
        }
    }
}

#[component]
fn GlassButton(label: String, onclick: EventHandler<MouseEvent>, children: Element) -> Element {
    rsx! {
        button {
            class: "flex size-10 items-center justify-center rounded-full border border-white/10 bg-white/[0.07] text-white/80 backdrop-blur-xl transition hover:bg-white/15 hover:text-white",
            title: "{label}",
            "aria-label": "{label}",
            onclick: move |e| onclick.call(e),
            {children}
        }
    }
}

#[component]
fn UmbrellaLogo() -> Element {
    rsx! {
        svg { class: "h-9 w-auto drop-shadow-[0_2px_10px_rgba(0,0,0,0.5)] md:h-12", view_box: "0 0 64 36", fill: "none",
            path { d: "M4 30C4 15.6 16.5 4 32 4s28 11.6 28 26", stroke: "white", stroke_width: "6", stroke_linecap: "round" }
            path {
                d: "M4 30c4.7-4.2 9.3-4.2 14 0 4.7-4.2 9.3-4.2 14 0 4.7-4.2 9.3-4.2 14 0 4.7-4.2 9.3-4.2 14 0",
                stroke: "white",
                stroke_width: "5",
                stroke_linecap: "round",
                stroke_linejoin: "round",
            }
        }
    }
}

#[component]
fn PageArrow(left: bool, disabled: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let pos = if left { "left-0" } else { "right-0" };
    rsx! {
        button {
            class: "absolute {pos} top-1/2 z-10 hidden size-12 -translate-y-1/2 items-center justify-center rounded-full border border-white/10 bg-white/[0.06] text-white/80 backdrop-blur-xl transition hover:bg-white/15 disabled:pointer-events-none disabled:opacity-0 md:flex",
            disabled,
            "aria-label": if left { "Vorherige Seite" } else { "Nächste Seite" },
            onclick: move |e| onclick.call(e),
            if left { ChevronLeft { class: "size-5" } } else { ChevronRight { class: "size-5" } }
        }
    }
}

fn busy_label(state: &str) -> Option<&'static str> {
    Some(match state {
        "installing" => "Installiert…",
        "updating" => "Aktualisiert…",
        "starting" => "Startet…",
        "restarting" => "Neustart…",
        "stopping" => "Stoppt…",
        "uninstalling" => "Entfernt…",
        "stopped" => "Gestoppt",
        "ready" | "running" => return None,
        _ => "Fehler",
    })
}

/// Opens a URL in a new tab.
pub fn open_url(url: &str) {
    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    if let Some(w) = web_sys::window() {
        let _ = w.open_with_url_and_target(url, "_blank");
    }
    #[cfg(not(all(feature = "web", target_arch = "wasm32")))]
    let _ = url;
}

#[component]
fn HomeTile(tile: AppTile) -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut menu = ctx.menu;
    let href = if tile.is_running() { tile.url(&ctx.link_mode()) } else { None };
    let busy = busy_label(&tile.state);
    let dim = if busy.is_some() { "opacity-60" } else { "" };
    // Long press (touch) opens the menu like a right click.
    let mut pressed = use_signal(|| false);
    let mut long_fired = use_signal(|| false);
    let t_ctx = tile.clone();
    let t_long = tile.clone();
    let t_click = tile.clone();

    rsx! {
        button {
            class: "group flex min-w-0 flex-col items-center gap-2 rounded-2xl p-1 outline-none focus-visible:bg-white/10 [-webkit-touch-callout:none]",
            title: "{tile.name}",
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                let p = e.client_coordinates();
                menu.set(Some(MenuReq { tile: t_ctx.clone(), x: p.x, y: p.y }));
            },
            onpointerdown: move |e: PointerEvent| {
                if e.pointer_type() != "touch" {
                    return;
                }
                let p = e.client_coordinates();
                let t = t_long.clone();
                pressed.set(true);
                long_fired.set(false);
                spawn(async move {
                    sleep_ms(550).await;
                    if pressed() {
                        long_fired.set(true);
                        menu.set(Some(MenuReq { tile: t, x: p.x, y: p.y }));
                    }
                });
            },
            onpointerup: move |_| pressed.set(false),
            onpointerleave: move |_| pressed.set(false),
            onclick: move |e: MouseEvent| {
                if long_fired() {
                    long_fired.set(false);
                    return;
                }
                match &href {
                    Some(h) => open_url(h),
                    None => {
                        let p = e.client_coordinates();
                        menu.set(Some(MenuReq { tile: t_click.clone(), x: p.x, y: p.y }));
                    }
                }
            },
            div { class: "relative transition-transform duration-200 group-hover:scale-105 group-active:scale-95 {dim}",
                AppIcon { id: tile.id.clone(), name: tile.name.clone(), class: "size-14 md:size-[4.5rem] shadow-[0_8px_24px_rgba(0,0,0,0.45)]" }
                if tile.update_available {
                    span { class: "absolute -right-1 -top-1 flex size-5 items-center justify-center rounded-full bg-sky-500 text-white ring-2 ring-black/40", title: "Update verfügbar",
                        ArrowUp { class: "size-3" }
                    }
                }
            }
            span { class: "w-full truncate px-1 text-center text-[13px] text-white/90 drop-shadow-[0_1px_4px_rgba(0,0,0,0.8)] md:text-[15px]",
                if let Some(b) = busy { "{b}" } else { "{tile.name}" }
            }
        }
    }
}

/// CPU / RAM / storage under the greeting; opens the activity window.
#[component]
fn LiveBar() -> Element {
    let ctx = use_context::<HomeCtx>();
    let ov = ctx.overview.read().clone();
    let failed = matches!(&ov, Some(Err(_))) || matches!(&ov, Some(Ok(o)) if o.resources.is_none());
    let (res, updates) = match ov {
        Some(Ok(o)) => (o.resources, o.apps.iter().filter(|a| a.update_available).count()),
        _ => (None, 0),
    };
    rsx! {
        div { class: "mt-4 flex flex-wrap items-center justify-center gap-2 px-3",
            button {
                class: "flex items-center gap-3 rounded-full border border-white/10 bg-black/25 px-4 py-2 text-xs text-white/85 backdrop-blur-xl transition hover:bg-black/35 md:gap-5 md:text-sm",
                title: "Aktivität öffnen",
                onclick: move |_| open_win(Win::Activity),
                if let Some(r) = res {
                    MiniMeter { icon: rsx! { Cpu {} }, label: "CPU", value: r.cpu.total_used.clamp(0.0, 100.0), text: format!("{:.0} %", r.cpu.total_used) }
                    MiniMeter { icon: rsx! { MemoryStick {} }, label: "RAM", value: pct(r.memory.total_used, r.memory.size), text: fmt_bytes(r.memory.total_used) }
                    MiniMeter { icon: rsx! { HardDrive {} }, label: "Speicher", value: pct(r.storage.total_used, r.storage.size), text: format!("{} frei", fmt_bytes(r.storage.available)) }
                } else if failed {
                    span { class: "text-white/50", "Keine Verbindung zum Umbrel" }
                } else {
                    span { class: "text-white/50", "Aktivität wird geladen…" }
                }
            }
            if updates > 0 {
                button {
                    class: "flex items-center gap-2 rounded-full border border-sky-400/30 bg-sky-500/20 px-4 py-2 text-xs text-sky-100 backdrop-blur-xl transition hover:bg-sky-500/30 md:text-sm",
                    onclick: move |_| open_win(Win::Updates),
                    ArrowUp { class: "size-4" }
                    if updates == 1 { "1 Update" } else { "{updates} Updates" }
                }
            }
        }
    }
}

#[component]
fn MiniMeter(icon: Element, label: String, value: f64, text: String) -> Element {
    let color = if value >= 90.0 {
        "bg-red-400"
    } else if value >= 70.0 {
        "bg-amber-300"
    } else {
        "bg-emerald-300"
    };
    rsx! {
        span { class: "flex items-center gap-2",
            span { class: "text-white/60 [&_svg]:size-3.5", {icon} }
            span { class: "hidden sm:inline text-white/60", "{label}" }
            span { class: "relative hidden h-1.5 w-10 overflow-hidden rounded-full bg-white/15 sm:block md:w-14",
                span { class: "absolute inset-y-0 left-0 rounded-full {color} transition-all duration-700", style: "width: {value.clamp(0.0, 100.0)}%;" }
            }
            span { class: "whitespace-nowrap tabular-nums", "{text}" }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum DockItem {
    Files,
    Store,
    Activity,
    Settings,
}

impl DockItem {
    const ALL: [Self; 4] = [Self::Files, Self::Store, Self::Activity, Self::Settings];

    const fn label(self) -> &'static str {
        match self {
            Self::Files => "Dateien",
            Self::Store => "App Store",
            Self::Activity => "Aktivität",
            Self::Settings => "Einstellungen",
        }
    }

    const fn win(self) -> Win {
        match self {
            Self::Files => Win::Files,
            Self::Store => Win::Store,
            Self::Activity => Win::Activity,
            Self::Settings => Win::Settings,
        }
    }

    fn icon(self) -> Element {
        let class = "size-6 md:size-7".to_string();
        match self {
            Self::Files => rsx! { FolderOpen { class } },
            Self::Store => rsx! { ShoppingBag { class } },
            Self::Activity => rsx! { Activity { class } },
            Self::Settings => rsx! { Settings { class } },
        }
    }
}

/// Dock in the same look as the app icons (dark tiles, white symbols).
#[component]
fn Dock(active: Option<Win>) -> Element {
    rsx! {
        nav { class: "mb-[max(env(safe-area-inset-bottom,0px),0.75rem)] mt-4 rounded-[1.6rem] border border-white/10 bg-white/[0.08] p-2 shadow-2xl backdrop-blur-2xl md:mb-5 md:p-2.5",
            div { class: "flex items-end gap-2 md:gap-3",
                for item in DockItem::ALL {
                    {
                        let is_active = Some(item.win()) == active || (item == DockItem::Store && active == Some(Win::Updates));
                        rsx! {
                            button {
                                key: "{item.label()}",
                                class: "group relative flex flex-col items-center",
                                title: "{item.label()}",
                                "aria-label": "{item.label()}",
                                onclick: move |_| open_win(item.win()),
                                span { class: "pointer-events-none absolute -top-9 whitespace-nowrap rounded-md bg-black/70 px-2 py-1 text-xs text-white opacity-0 backdrop-blur transition group-hover:opacity-100",
                                    "{item.label()}"
                                }
                                span { class: "flex size-12 items-center justify-center rounded-[24%] bg-gradient-to-b from-[#2c2c2e] to-[#1c1c1e] text-white shadow-[inset_0_1px_0_rgba(255,255,255,0.08),0_8px_20px_rgba(0,0,0,0.45)] transition-transform duration-200 group-hover:-translate-y-1 group-hover:scale-110 md:size-14",
                                    {item.icon()}
                                }
                                span { class: if is_active { "mt-1 size-1 rounded-full bg-white" } else { "mt-1 size-1 rounded-full bg-transparent" } }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Glass window used by App Store, activity and settings.
#[component]
pub fn HomeWindow(title: String, #[props(default)] narrow: bool, children: Element) -> Element {
    let w = if narrow { "md:max-w-3xl" } else { "md:max-w-6xl" };
    rsx! {
        div { class: "fixed inset-0 z-50 flex items-stretch justify-center bg-black/45 backdrop-blur-sm md:items-center md:p-6",
            onclick: move |_| close_win(),
            div {
                class: "relative flex w-full {w} flex-col overflow-hidden border-white/10 bg-[#141414]/85 text-white shadow-2xl backdrop-blur-2xl md:h-[min(88dvh,900px)] md:rounded-[1.75rem] md:border",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex shrink-0 items-center justify-between gap-3 px-5 pb-2 pt-[max(env(safe-area-inset-top,0px),1rem)] md:px-7 md:pt-6",
                    h2 { class: "truncate text-xl font-semibold md:text-2xl", "{title}" }
                    button {
                        class: "flex size-9 items-center justify-center rounded-full bg-white/10 text-white/80 transition hover:bg-white/20 hover:text-white",
                        "aria-label": "Schließen",
                        onclick: move |_| close_win(),
                        X { class: "size-4" }
                    }
                }
                div { class: "min-h-0 flex-1 overflow-y-auto px-5 pb-[max(env(safe-area-inset-bottom,0px),1.25rem)] md:px-7 md:pb-7", {children} }
            }
        }
    }
}

/// File manager as a window (drag & drop upload, preview, …) – without blur so its menus
/// are positioned against the whole screen.
#[component]
fn FilesWindow(path: String, msg: String) -> Element {
    rsx! {
        div { class: "fixed inset-0 z-50 flex items-stretch justify-center bg-black/55 md:items-center md:p-6", onclick: move |_| close_win(),
            div {
                class: "relative flex h-full w-full flex-col overflow-hidden border-white/10 bg-[#141414] text-white shadow-2xl md:h-[min(90dvh,960px)] md:max-w-7xl md:rounded-[1.75rem] md:border",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex shrink-0 items-center justify-between gap-3 px-5 pb-2 pt-[max(env(safe-area-inset-top,0px),1rem)] md:px-6 md:pt-5",
                    h2 { class: "text-xl font-semibold", "Dateien" }
                    button {
                        class: "flex size-9 items-center justify-center rounded-full bg-white/10 text-white/80 transition hover:bg-white/20 hover:text-white",
                        "aria-label": "Schließen",
                        onclick: move |_| close_win(),
                        X { class: "size-4" }
                    }
                }
                div { class: "min-h-0 flex-1 px-3 pb-[max(env(safe-area-inset-bottom,0px),0.75rem)] md:px-5 md:pb-5",
                    crate::pages::files::FilesPage { path, msg }
                }
            }
        }
    }
}
