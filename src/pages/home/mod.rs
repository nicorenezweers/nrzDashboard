//! Home screen in the style of the umbrelOS desktop: wallpaper, greeting, paged app grid,
//! live CPU/RAM/storage, Spotlight search (⌘K), dock and windows for App Store, activity,
//! settings and the umbrelOS MCP console.

mod activity;
mod console;
mod prefs;
mod settings;
mod spotlight;
mod store;

use dioxus::prelude::*;
use icons::{
    Activity, ArrowUp, Bot, ChevronLeft, ChevronRight, Cpu, FolderOpen, HardDrive, LayoutDashboard, MemoryStick, RefreshCw,
    Settings, ShoppingBag, Terminal, X,
};

use crate::api::board::{AppTile, app_tiles};
use crate::api::get_overview;
use crate::app::Route;
use crate::model::Overview;
use crate::pages::board::appmenu::{AppMenuCtx, AppMenuHost};
use crate::pages::board::widgets::{AppIcon, browser_url};
use crate::utils::{fmt_bytes, pct, sleep_ms};

pub use prefs::{Prefs, wallpaper_style};

pub const WALLPAPER: Asset = asset!("/assets/wallpaper.jpg");

/// Windows that open above the home screen (`/?w=…`, so the back button closes them).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Win {
    Store,
    Updates,
    Activity,
    Settings,
    Console,
}

impl Win {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Store => "store",
            Self::Updates => "updates",
            Self::Activity => "activity",
            Self::Settings => "settings",
            Self::Console => "mcp",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "store" => Self::Store,
            "updates" => Self::Updates,
            "activity" => Self::Activity,
            "settings" => Self::Settings,
            "mcp" => Self::Console,
            _ => return None,
        })
    }
}

pub fn open_win(win: Win) {
    navigator().push(Route::HomePage { w: win.key().to_string() });
}

pub fn close_win() {
    navigator().replace(Route::HomePage { w: String::new() });
}

/// Data shared by the home screen and its windows.
#[derive(Clone, Copy)]
pub struct HomeCtx {
    pub overview: Resource<Result<Overview, ServerFnError>>,
    pub tiles: Resource<Result<Vec<AppTile>, ServerFnError>>,
    /// Bump to reload everything at once (refresh button, after actions).
    pub refresh: Signal<u32>,
    pub prefs: Signal<Prefs>,
}

impl HomeCtx {
    pub fn reload(mut self) {
        self.refresh += 1;
    }
}

fn now_hour() -> u32 {
    #[cfg(feature = "web")]
    {
        js_sys::Date::new_0().get_hours()
    }
    #[cfg(not(feature = "web"))]
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
    let row_h = if w >= 768.0 { 132.0 } else { 104.0 };
    let reserved = if w >= 768.0 { 470.0 } else { 400.0 };
    let rows = (((h - reserved) / row_h).floor() as i64).clamp(2, if w >= 768.0 { 4 } else { 6 }) as usize;
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

#[component]
pub fn HomePage(w: String) -> Element {
    let win = Win::parse(&w);
    let menu = use_signal(|| None);
    let refresh = use_signal(|| 0u32);
    use_context_provider(|| AppMenuCtx { menu, refresh });
    let prefs = use_signal(Prefs::default);
    let mut prefs_w = prefs;
    use_effect(move || prefs_w.set(Prefs::load()));

    let mut overview = use_resource(move || {
        let _ = refresh();
        get_overview()
    });
    let mut tiles = use_resource(move || {
        let _ = refresh();
        app_tiles()
    });
    let ctx = use_context_provider(|| HomeCtx { overview, tiles, refresh, prefs });

    // Live values: resources every 5 s, app list every 15 s.
    use_future(move || async move {
        let mut n = 0u32;
        loop {
            sleep_ms(5_000).await;
            n += 1;
            overview.restart();
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

    use_future(move || async move {
        let mut ev = document::eval(KEY_JS);
        while let Ok(k) = ev.recv::<String>().await {
            match k.as_str() {
                "search" => spotlight.toggle(),
                "esc" => {
                    if spotlight() {
                        spotlight.set(false);
                    } else if Win::parse(&current_w()).is_some() {
                        close_win();
                    }
                }
                "left" if !spotlight() && Win::parse(&current_w()).is_none() => {
                    page.set(page().saturating_sub(1));
                }
                "right" if !spotlight() && Win::parse(&current_w()).is_none() => {
                    page.set(page() + 1);
                }
                _ => {}
            }
        }
    });

    let name = prefs.read().name.clone();
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

    let do_refresh = move |_| {
        ctx.reload();
        spinning.set(true);
        spawn(async move {
            sleep_ms(900).await;
            spinning.set(false);
        });
    };

    rsx! {
        div {
            class: "fixed inset-0 overflow-hidden select-none text-white",
            style: wallpaper_style(&prefs.read()),
            // soft vignette like umbrelOS
            div { class: "pointer-events-none absolute inset-0 bg-[radial-gradient(ellipse_at_center,transparent_30%,rgba(0,0,0,0.55)_100%)]" }

            // Top-right actions
            div { class: "absolute right-3 top-3 z-20 flex items-center gap-2 md:right-5 md:top-5",
                button {
                    class: "flex size-10 items-center justify-center rounded-full border border-white/10 bg-white/[0.07] text-white/80 backdrop-blur-xl transition hover:bg-white/15 hover:text-white",
                    title: "Aktualisieren",
                    "aria-label": "Aktualisieren",
                    onclick: do_refresh,
                    RefreshCw { class: if spinning() { "size-4 animate-spin" } else { "size-4" } }
                }
                Link {
                    to: Route::SystemPage {},
                    class: "hidden md:flex h-10 items-center rounded-full border border-white/10 bg-white/[0.07] px-4 text-sm text-white/80 backdrop-blur-xl transition hover:bg-white/15 hover:text-white",
                    "System"
                }
                a {
                    href: "/auth/logout",
                    class: "flex h-10 items-center rounded-full border border-white/10 bg-white/[0.07] px-4 text-sm text-white/80 backdrop-blur-xl transition hover:bg-white/15 hover:text-white",
                    "Abmelden"
                }
            }

            div { class: "relative z-10 flex h-full flex-col items-center pt-[calc(env(safe-area-inset-top,0px)+3.75rem)] md:pt-8",
                // Logo + greeting
                UmbrellaLogo {}
                h1 { class: "mt-3 text-center text-3xl font-bold tracking-tight drop-shadow-[0_2px_12px_rgba(0,0,0,0.5)] md:mt-4 md:text-5xl",
                    "{hello}"
                }
                LiveBar {}

                // App grid with pages
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
                                Link { to: Route::SystemPage {}, class: "rounded-full bg-white/15 px-4 py-2 text-sm hover:bg-white/25", "Umbrel-Verbindung einrichten" }
                            }
                        } else if loading {
                            div { class: "grid h-full content-center gap-y-6 px-4 md:px-16", style: "grid-template-columns: repeat({cols}, minmax(0, 1fr));",
                                for i in 0..per_page.min(12) {
                                    div { key: "{i}", class: "flex flex-col items-center gap-2",
                                        div { class: "size-14 animate-pulse rounded-[22%] bg-white/10 md:size-[4.5rem]" }
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

                // Page dots
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

                // Search pill
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
                    Win::Console => rsx! { console::ConsoleWindow {} },
                }
            }
            if spotlight() {
                spotlight::Spotlight { onclose: move |_| spotlight.set(false) }
            }
            AppMenuHost {}
        }
    }
}

/// Current `w` query parameter (read inside the key listener, outside of render).
fn current_w() -> String {
    match router().current::<Route>() {
        Route::HomePage { w } => w,
        _ => String::new(),
    }
}

#[component]
fn UmbrellaLogo() -> Element {
    rsx! {
        svg {
            class: "h-9 w-auto drop-shadow-[0_2px_10px_rgba(0,0,0,0.5)] md:h-12",
            view_box: "0 0 64 36",
            fill: "none",
            path {
                d: "M4 30C4 15.6 16.5 4 32 4s28 11.6 28 26",
                stroke: "white",
                stroke_width: "6",
                stroke_linecap: "round",
            }
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

#[component]
fn HomeTile(tile: AppTile) -> Element {
    let menu = use_context::<AppMenuCtx>();
    let href = tile.url.as_deref().map(browser_url).filter(|_| matches!(tile.state.as_str(), "ready" | "running"));
    let icon = tile.icon.clone().unwrap_or_default();
    let menu_tile = tile.clone();
    let oncontext = move |e: MouseEvent| menu.open(menu_tile.clone(), &e);
    let busy = busy_label(&tile.state);
    let dim = if busy.is_some() { "opacity-60" } else { "" };
    let inner = rsx! {
        div { class: "relative transition-transform duration-200 group-hover:scale-105 group-active:scale-95 {dim}",
            AppIcon {
                key: "{tile.id}|{icon}",
                id: tile.id.clone(),
                name: tile.name.clone(),
                url: icon.clone(),
                class: "size-14 md:size-[4.5rem] shadow-[0_8px_24px_rgba(0,0,0,0.45)]",
            }
            if tile.update_available {
                span { class: "absolute -right-1 -top-1 flex size-5 items-center justify-center rounded-full bg-sky-500 text-white ring-2 ring-black/40", title: "Update verfügbar",
                    ArrowUp { class: "size-3" }
                }
            }
        }
        span { class: "w-full truncate px-1 text-center text-[13px] text-white/90 drop-shadow-[0_1px_4px_rgba(0,0,0,0.8)] md:text-[15px]",
            if let Some(b) = busy { "{b}" } else { "{tile.name}" }
        }
    };
    rsx! {
        if let Some(h) = href {
            a {
                href: "{h}",
                target: "_blank",
                rel: "noopener noreferrer",
                title: "{tile.name} – Rechtsklick für Optionen",
                draggable: "false",
                class: "group flex min-w-0 flex-col items-center gap-2 rounded-2xl p-1 outline-none focus-visible:bg-white/10",
                oncontextmenu: oncontext,
                {inner}
            }
        } else {
            button {
                class: "group flex min-w-0 flex-col items-center gap-2 rounded-2xl p-1",
                title: "{tile.name} – Rechtsklick für Optionen",
                onclick: {
                    let t = tile.clone();
                    move |e: MouseEvent| menu.open(t.clone(), &e)
                },
                oncontextmenu: oncontext,
                {inner}
            }
        }
    }
}

/// Compact CPU / RAM / storage readout under the greeting; opens the activity window.
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

#[derive(Clone, PartialEq)]
enum DockTarget {
    Route(Route),
    Win(Win),
}

struct DockItem {
    label: &'static str,
    gradient: &'static str,
    target: DockTarget,
}

fn dock_items() -> Vec<DockItem> {
    vec![
        DockItem {
            label: "Dateien",
            gradient: "bg-gradient-to-b from-[#ff8a65] to-[#c2185b]",
            target: DockTarget::Route(Route::FilesPage { path: "/Home".into(), msg: String::new() }),
        },
        DockItem { label: "Board", gradient: "bg-gradient-to-b from-[#3d8b7a] to-[#1d3f3a]", target: DockTarget::Route(Route::BoardPage {}) },
        DockItem { label: "App Store", gradient: "bg-gradient-to-b from-[#3b82f6] to-[#1e3a8a]", target: DockTarget::Win(Win::Store) },
        DockItem { label: "Monitore", gradient: "bg-gradient-to-b from-[#f5e6b8] to-[#c9a96a]", target: DockTarget::Route(Route::MonitorsPage {}) },
        DockItem { label: "Agent", gradient: "bg-gradient-to-b from-[#a78bfa] to-[#5b21b6]", target: DockTarget::Route(Route::AgentPage {}) },
        DockItem { label: "MCP-Konsole", gradient: "bg-gradient-to-b from-[#374151] to-[#0b0f17]", target: DockTarget::Win(Win::Console) },
        DockItem { label: "Einstellungen", gradient: "bg-gradient-to-b from-[#94a3b8] to-[#475569]", target: DockTarget::Win(Win::Settings) },
        DockItem { label: "Aktivität", gradient: "bg-gradient-to-b from-[#1f4d3f] to-[#0b1f1a]", target: DockTarget::Win(Win::Activity) },
    ]
}

fn dock_icon(label: &str) -> Element {
    let class = "size-5 md:size-7 text-white drop-shadow".to_string();
    match label {
        "Dateien" => rsx! { FolderOpen { class } },
        "Board" => rsx! { LayoutDashboard { class } },
        "App Store" => rsx! { ShoppingBag { class } },
        "Monitore" => rsx! { Activity { class: "size-5 md:size-7 text-[#5b4a1f]" } },
        "Agent" => rsx! { Bot { class } },
        "MCP-Konsole" => rsx! { Terminal { class: "size-5 md:size-7 text-emerald-300" } },
        "Einstellungen" => rsx! { Settings { class } },
        _ => rsx! { Activity { class: "size-5 md:size-7 text-emerald-300" } },
    }
}

#[component]
fn Dock(active: Option<Win>) -> Element {
    rsx! {
        nav { class: "mb-[max(env(safe-area-inset-bottom,0px),0.75rem)] mt-4 max-w-[calc(100vw-1rem)] overflow-x-auto rounded-[1.4rem] border border-white/10 bg-white/[0.08] p-2 shadow-2xl backdrop-blur-2xl md:mb-5 md:p-2.5",
            div { class: "flex items-end gap-1.5 md:gap-3",
                for item in dock_items() {
                    {
                        let is_active = matches!(item.target, DockTarget::Win(w) if Some(w) == active)
                            || (active == Some(Win::Updates) && item.target == DockTarget::Win(Win::Store));
                        let target = item.target.clone();
                        rsx! {
                            button {
                                key: "{item.label}",
                                class: "group relative flex flex-col items-center",
                                title: "{item.label}",
                                "aria-label": "{item.label}",
                                onclick: move |_| match &target {
                                    DockTarget::Route(r) => {
                                        navigator().push(r.clone());
                                    }
                                    DockTarget::Win(w) => open_win(*w),
                                },
                                span { class: "pointer-events-none absolute -top-9 whitespace-nowrap rounded-md bg-black/70 px-2 py-1 text-xs text-white opacity-0 backdrop-blur transition group-hover:opacity-100",
                                    "{item.label}"
                                }
                                span { class: "flex size-10 items-center justify-center rounded-[28%] border border-white/15 shadow-[inset_0_1px_0_rgba(255,255,255,0.25),0_6px_16px_rgba(0,0,0,0.4)] transition-transform duration-200 group-hover:-translate-y-1 group-hover:scale-110 md:size-14 {item.gradient}",
                                    {dock_icon(item.label)}
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

/// Glass window used by App Store, activity, settings and console.
#[component]
pub fn HomeWindow(title: String, #[props(default)] narrow: bool, children: Element) -> Element {
    let w = if narrow { "md:max-w-3xl" } else { "md:max-w-6xl" };
    rsx! {
        div { class: "fixed inset-0 z-50 flex items-stretch justify-center bg-black/45 backdrop-blur-sm md:items-center md:p-6 animate-in fade-in",
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
