//! Widget renderers for the board.

use dioxus::prelude::*;
use icons::{
    Activity, AppWindow, Bookmark, Boxes, Clock, Cloud, CloudDrizzle, CloudFog, CloudLightning, CloudMoon, CloudRain,
    CloudSnow, CloudSun, Cpu, Droplets, ExternalLink, Frame, Globe, HardDrive, MemoryStick, Moon, Newspaper, Pencil, Search,
    Server, ShieldCheck, Star, StickyNote, Sun, Thermometer, TriangleAlert, Wallet, Wind,
};
use serde_json::json;

use crate::api::board::{AppTile, Widget, WidgetKind, app_tiles, ping_url, weather};
use crate::api::get_overview;
use crate::app::Route;
use crate::components::widgets::input_class;
use crate::utils::{browser_host_url, err_msg, fmt_ago, fmt_bytes, fmt_uptime, pct, sleep_ms};

pub fn kind_icon(kind: WidgetKind) -> Element {
    match kind {
        WidgetKind::Apps => rsx! { Boxes {} },
        WidgetKind::App => rsx! { AppWindow {} },
        WidgetKind::System => rsx! { Server {} },
        WidgetKind::Monitors => rsx! { Activity {} },
        WidgetKind::News => rsx! { Newspaper {} },
        WidgetKind::Clock => rsx! { Clock {} },
        WidgetKind::Weather => rsx! { CloudSun {} },
        WidgetKind::Notes => rsx! { StickyNote {} },
        WidgetKind::Bookmarks => rsx! { Bookmark {} },
        WidgetKind::Search => rsx! { Search {} },
        WidgetKind::Iframe => rsx! { Frame {} },
        WidgetKind::Dns => rsx! { ShieldCheck {} },
        WidgetKind::Finance => rsx! { Wallet {} },
    }
}

/// `http://umbrel.local:PORT` → host the browser is using.
pub fn browser_url(url: &str) -> String {
    #[cfg(feature = "web")]
    if let Some(host) = web_sys::window().and_then(|w| w.location().hostname().ok()).filter(|h| !h.is_empty()) {
        return url.replace("umbrel.local", &host);
    }
    url.to_string()
}

/// Card frame shared by all widgets.
#[component]
fn Frame_(title: String, #[props(default)] padded: bool, #[props(default)] edit: bool, children: Element) -> Element {
    let pad = if padded { "p-4" } else { "" };
    rsx! {
        div { class: "flex h-full min-h-0 flex-col overflow-hidden rounded-xl border bg-card text-card-foreground shadow-sm",
            if !title.is_empty() {
                div { class: if edit { "pl-12 pr-24 pt-4 pb-1 text-sm font-medium truncate" } else { "px-4 pt-3 pb-1 text-sm font-medium truncate" }, "{title}" }
            }
            div { class: "min-h-0 flex-1 {pad}", {children} }
        }
    }
}

#[component]
pub fn WidgetView(widget: Widget, edit: bool, onchange: EventHandler<Widget>) -> Element {
    let title = widget.title.clone();
    let body = match widget.kind {
        WidgetKind::Apps => rsx! { AppsWidget { widget: widget.clone() } },
        WidgetKind::App => rsx! { AppWidget { widget: widget.clone() } },
        WidgetKind::System => rsx! { SystemWidget { widget: widget.clone() } },
        WidgetKind::Monitors => rsx! { MonitorsWidget { widget: widget.clone() } },
        WidgetKind::News => rsx! { NewsWidget { widget: widget.clone() } },
        WidgetKind::Clock => rsx! { ClockWidget { widget: widget.clone() } },
        WidgetKind::Weather => rsx! { WeatherWidget { widget: widget.clone() } },
        WidgetKind::Notes => rsx! { NotesWidget { widget: widget.clone(), onchange } },
        WidgetKind::Bookmarks => rsx! { BookmarksWidget { widget: widget.clone() } },
        WidgetKind::Search => rsx! { SearchWidget { widget: widget.clone() } },
        WidgetKind::Iframe => rsx! { IframeWidget { widget: widget.clone() } },
        WidgetKind::Dns => rsx! { super::integrations::DnsWidget { widget: widget.clone() } },
        WidgetKind::Finance => rsx! { super::integrations::FinanceWidget { widget: widget.clone() } },
    };
    let padded = !matches!(widget.kind, WidgetKind::Iframe | WidgetKind::App | WidgetKind::Search);
    rsx! {
        Frame_ { title, padded, edit, {body} }
    }
}

// ---------- App icon ----------

fn icon_candidates(id: &str) -> Vec<String> {
    let mut v = Vec::new();
    if id.starts_with("nrz-") {
        v.push(format!("http://{}:8390/nicorenezweers/nrzStore/raw/branch/main/{id}/icon.png", host_for_icons()));
    }
    v.push(format!("https://getumbrel.github.io/umbrel-apps-gallery/{id}/icon.svg"));
    v
}

fn host_for_icons() -> String {
    #[cfg(feature = "web")]
    if let Some(h) = web_sys::window().and_then(|w| w.location().hostname().ok()).filter(|h| !h.is_empty()) {
        return h;
    }
    "umbrel.local".into()
}

/// App icon with fallbacks: nrzStore / Umbrel gallery / custom URL → first letter.
#[component]
pub fn AppIcon(id: String, name: String, #[props(default)] url: String, #[props(default = "size-12".to_string())] class: String) -> Element {
    let candidates = if url.is_empty() { icon_candidates(&id) } else { vec![url.clone()] };
    let mut idx = use_signal(|| 0usize);
    let letter: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into());
    rsx! {
        if let Some(src) = candidates.get(idx()).cloned() {
            img {
                class: "{class} rounded-[22%] object-cover bg-muted",
                src: "{src}",
                alt: "",
                loading: "lazy",
                onerror: move |_| idx += 1,
            }
        } else {
            div { class: "{class} flex items-center justify-center rounded-[22%] bg-muted text-lg font-semibold text-muted-foreground",
                "{letter}"
            }
        }
    }
}

fn status_dot(state: &str) -> &'static str {
    match state {
        "ready" | "running" => "bg-success",
        "stopped" => "bg-muted-foreground/60",
        "starting" | "restarting" | "updating" | "installing" | "stopping" => "bg-warning animate-pulse",
        _ => "bg-destructive",
    }
}

// ---------- Apps ----------

#[component]
fn AppsWidget(widget: Widget) -> Element {
    let ctx = use_context::<super::appmenu::AppMenuCtx>();
    let refresh = ctx.refresh;
    let mut tiles = use_resource(move || {
        let _ = refresh();
        app_tiles()
    });
    use_future(move || async move {
        loop {
            sleep_ms(10_000).await;
            tiles.restart();
        }
    });
    let show_stopped = widget.opt_bool("stopped", true);
    match tiles.read().clone() {
        None => rsx! { div { class: "grid grid-cols-[repeat(auto-fill,minmax(76px,1fr))] gap-3", for _ in 0..8 { div { class: "h-20 animate-pulse rounded-xl bg-muted/40" } } } },
        Some(Err(e)) => rsx! { p { class: "text-sm text-destructive", {err_msg(&e)} } },
        Some(Ok(list)) => {
            let list: Vec<AppTile> = list.into_iter().filter(|a| !a.hidden && (show_stopped || a.state != "stopped")).collect();
            rsx! {
                div { class: "h-full overflow-y-auto pr-1 -mr-1",
                    div { class: "grid grid-cols-[repeat(auto-fill,minmax(76px,1fr))] gap-x-2 gap-y-3",
                        for a in list {
                            AppTileView { key: "{a.id}", tile: a }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AppTileView(tile: AppTile) -> Element {
    let ctx = use_context::<super::appmenu::AppMenuCtx>();
    let href = tile.url.as_deref().map(browser_url);
    let dot = status_dot(&tile.state);
    let icon = tile.icon.clone().unwrap_or_default();
    let menu_tile = tile.clone();
    let oncontext = move |e: MouseEvent| ctx.open(menu_tile.clone(), &e);
    let inner = rsx! {
        div { class: "relative",
            AppIcon { key: "{tile.id}|{icon}", id: tile.id.clone(), name: tile.name.clone(), url: icon.clone(), class: "size-12" }
            span { class: "absolute -right-0.5 -bottom-0.5 size-3 rounded-full border-2 border-card {dot}" }
            if tile.update_available {
                span { class: "absolute -right-1 -top-1 size-2.5 rounded-full bg-warning", title: "Update verfügbar" }
            }
        }
        span { class: "w-full truncate text-center text-xs", "{tile.name}" }
    };
    rsx! {
        if let Some(h) = href {
            a { href: "{h}", target: "_blank", rel: "noopener noreferrer", title: "{tile.name} – Rechtsklick für Optionen",
                class: "flex flex-col items-center gap-1.5 rounded-lg p-1.5 hover:bg-accent/50 transition-colors",
                oncontextmenu: oncontext,
                {inner}
            }
        } else {
            div { class: "flex flex-col items-center gap-1.5 rounded-lg p-1.5 opacity-70", title: "{tile.name}", oncontextmenu: oncontext, {inner} }
        }
    }
}

// ---------- Single app / link ----------

#[component]
fn AppWidget(widget: Widget) -> Element {
    let app_id = widget.opt_str("app");
    let custom_url = widget.opt_str("url");
    let custom_name = widget.opt_str("name");
    let icon = widget.opt_str("icon");
    let ctx = use_context::<super::appmenu::AppMenuCtx>();
    let refresh = ctx.refresh;
    let mut tiles = use_resource(move || {
        let _ = refresh();
        async move { app_tiles().await.ok() }
    });
    use_future(move || async move {
        loop {
            sleep_ms(15_000).await;
            tiles.restart();
        }
    });
    let configured = !app_id.is_empty() || !custom_url.is_empty();
    let tile = if app_id.is_empty() {
        None
    } else {
        tiles.read().clone().flatten().and_then(|l| l.into_iter().find(|t| t.id == app_id))
    };
    let name = if !custom_name.is_empty() { custom_name } else { tile.as_ref().map(|t| t.name.clone()).unwrap_or_else(|| "App".into()) };
    let raw_url = if !custom_url.is_empty() { Some(custom_url) } else { tile.as_ref().and_then(|t| t.url.clone()) };
    let ping_target = raw_url.clone().unwrap_or_default();
    let mut ping = use_signal(|| None::<Option<f64>>);
    use_effect(use_reactive!(|ping_target| {
        spawn(async move {
            if ping_target.is_empty() {
                return;
            }
            loop {
                if let Ok(v) = ping_url(ping_target.clone()).await {
                    ping.set(Some(v));
                }
                sleep_ms(30_000).await;
            }
        });
    }));
    let href = raw_url.as_deref().map(browser_url);
    let (dot, label) = match ping() {
        Some(Some(ms)) => ("bg-success", format!("{ms:.0} ms")),
        Some(None) => ("bg-destructive", "offline".to_string()),
        None => ("bg-muted-foreground/50", String::new()),
    };
    let id = tile.as_ref().map(|t| t.id.clone()).unwrap_or_else(|| name.to_lowercase());
    let icon = if icon.is_empty() { tile.as_ref().and_then(|t| t.icon.clone()).unwrap_or_default() } else { icon };
    let icon_key = format!("{id}|{icon}");
    let menu_tile = tile.clone();
    let oncontext = move |e: MouseEvent| {
        if let Some(t) = menu_tile.clone() {
            ctx.open(t, &e);
        }
    };
    let content = rsx! {
        AppIcon { key: "{icon_key}", id, name: name.clone(), url: icon, class: "size-14" }
        div { class: "max-w-full truncate text-sm font-medium", "{name}" }
        div { class: "flex items-center gap-1.5 text-[11px] text-muted-foreground",
            span { class: "size-2 rounded-full {dot}" }
            "{label}"
        }
    };
    rsx! {
        if let Some(h) = href {
            a { href: "{h}", target: "_blank", rel: "noopener noreferrer",
                class: "flex h-full flex-col items-center justify-center gap-1.5 p-3 hover:bg-accent/40 transition-colors",
                oncontextmenu: oncontext,
                {content}
            }
        } else {
            div { class: "flex h-full flex-col items-center justify-center gap-1.5 p-3 text-center", oncontextmenu: oncontext,
                {content}
                if !configured {
                    p { class: "text-xs text-muted-foreground", "In den Einstellungen eine App oder URL wählen." }
                }
            }
        }
    }
}

// ---------- System ----------

#[component]
fn SystemWidget(widget: Widget) -> Element {
    let mut ov = use_resource(get_overview);
    use_future(move || async move {
        loop {
            sleep_ms(5_000).await;
            ov.restart();
        }
    });
    let tall = widget.h >= 4;
    match ov.read().clone() {
        None => rsx! { div { class: "h-full animate-pulse rounded-lg bg-muted/40" } },
        Some(Err(e)) => rsx! { p { class: "text-sm text-destructive", {err_msg(&e)} } },
        Some(Ok(o)) => {
            if let Some(e) = o.error.clone().filter(|_| o.resources.is_none()) {
                return rsx! { p { class: "text-sm text-destructive", "{e}" } };
            }
            let r = o.resources.clone().unwrap_or_default();
            let info = o.info.clone().unwrap_or_default();
            let mem = pct(r.memory.total_used, r.memory.size);
            let disk = pct(r.storage.total_used, r.storage.size);
            rsx! {
                div { class: "flex h-full flex-col gap-3 overflow-y-auto",
                    Meter { icon: rsx! { Cpu {} }, label: "CPU", value: r.cpu.total_used, detail: format!("{:.0} %", r.cpu.total_used) }
                    Meter { icon: rsx! { MemoryStick {} }, label: "RAM", value: mem, detail: format!("{} / {}", fmt_bytes(r.memory.total_used), fmt_bytes(r.memory.size)) }
                    Meter { icon: rsx! { HardDrive {} }, label: "Speicher", value: disk, detail: format!("{} frei", fmt_bytes(r.storage.available)) }
                    div { class: "flex items-center justify-between text-xs text-muted-foreground",
                        span { class: "flex items-center gap-1.5 [&_svg]:size-3.5", Thermometer {} "{r.temperature.temperature:.0} °C" }
                        span { "Laufzeit {fmt_uptime(info.uptime_seconds)}" }
                    }
                    if tall {
                        div { class: "mt-1 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs",
                            span { class: "text-muted-foreground", "Host" }
                            span { class: "truncate", "{info.hostname}" }
                            span { class: "text-muted-foreground", "OS" }
                            span { class: "truncate", "{info.version_name}" }
                            span { class: "text-muted-foreground", "CPU" }
                            span { class: "truncate", "{info.cpu}" }
                            span { class: "text-muted-foreground", "Apps" }
                            span { "{o.apps.iter().filter(|a| a.state == \"ready\").count()} aktiv / {o.apps.len()}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Meter(icon: Element, label: String, value: f64, detail: String) -> Element {
    let color = if value >= 90.0 {
        "bg-destructive"
    } else if value >= 75.0 {
        "bg-warning"
    } else {
        "bg-primary/80"
    };
    rsx! {
        div { class: "flex flex-col gap-1",
            div { class: "flex items-center justify-between text-xs",
                span { class: "flex items-center gap-1.5 text-muted-foreground [&_svg]:size-3.5", {icon} "{label}" }
                span { class: "tabular-nums", "{detail}" }
            }
            div { class: "h-1.5 overflow-hidden rounded-full bg-secondary",
                div { class: "h-full rounded-full transition-all duration-500 {color}", style: format!("width: {:.1}%", value.clamp(0.0, 100.0)) }
            }
        }
    }
}

// ---------- Monitors ----------

#[component]
fn MonitorsWidget(widget: Widget) -> Element {
    let mut data = use_resource(crate::api::monitor::monitor_overview);
    use_future(move || async move {
        loop {
            sleep_ms(5_000).await;
            data.restart();
        }
    });
    let beats = if widget.w >= 6 { 40 } else if widget.w >= 4 { 25 } else { 12 };
    match data.read().clone() {
        None => rsx! { div { class: "h-full animate-pulse rounded-lg bg-muted/40" } },
        Some(Err(e)) => rsx! { p { class: "text-sm text-destructive", {err_msg(&e)} } },
        Some(Ok(list)) => {
            use crate::api::monitor::Status;
            let up = list.iter().filter(|m| m.status == Status::Up).count();
            let down = list.iter().filter(|m| m.status == Status::Down).count();
            rsx! {
                div { class: "flex h-full flex-col gap-2",
                    div { class: "flex items-center gap-3 text-xs text-muted-foreground",
                        span { class: "flex items-center gap-1.5", span { class: "size-2 rounded-full bg-success" } "{up} up" }
                        span { class: "flex items-center gap-1.5", span { class: "size-2 rounded-full bg-destructive" } "{down} down" }
                        div { class: "flex-1" }
                        Link { to: Route::MonitorsPage {}, class: "hover:text-foreground", "Alle →" }
                    }
                    if list.is_empty() {
                        p { class: "text-sm text-muted-foreground", "Noch keine Monitore." }
                    }
                    div { class: "flex min-h-0 flex-1 flex-col gap-2.5 overflow-y-auto pr-1",
                        for m in list {
                            div { key: "{m.id}", class: "flex flex-col gap-1",
                                div { class: "flex items-center justify-between gap-2 text-sm",
                                    span { class: "flex min-w-0 items-center gap-2",
                                        span { class: "size-2 shrink-0 rounded-full {m.status.bg()}" }
                                        span { class: "truncate", "{m.name}" }
                                    }
                                    span { class: "shrink-0 text-xs tabular-nums text-muted-foreground",
                                        {crate::pages::monitors::fmt_ms_opt(m.latency_ms)}
                                        " · "
                                        {crate::pages::monitors::fmt_pct(m.uptime_24h)}
                                    }
                                }
                                crate::pages::monitors::HeartbeatBar { beats: m.beats.clone(), max: beats, class: "h-4" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------- News (nrzNews) ----------

/// Kennzahlen und Schlagzeilen aus nrzNews (MCP-Werkzeug `dashboard_overview`). Passt sich der
/// Grösse an: schmal nur die Zahlen, höher auch die neuesten ungelesenen Artikel.
#[component]
fn NewsWidget(widget: Widget) -> Element {
    let headlines = widget.options.get("headlines").and_then(serde_json::Value::as_u64).unwrap_or(5).min(20) as u32;
    let show_metrics = widget.opt_bool("metrics", true);
    let mut data = use_resource(use_reactive!(|headlines| async move {
        crate::api::news::news_overview(headlines).await
    }));
    use_future(move || async move {
        loop {
            sleep_ms(60_000).await;
            data.restart();
        }
    });
    let cols = if widget.w >= 8 { "grid-cols-6" } else if widget.w >= 4 { "grid-cols-3" } else { "grid-cols-2" };
    match data.read().clone() {
        None => rsx! { div { class: "h-full animate-pulse rounded-lg bg-muted/40" } },
        Some(Err(e)) => rsx! { p { class: "text-sm text-destructive", {err_msg(&e)} } },
        Some(Ok(None)) => rsx! {
            div { class: "flex h-full flex-col items-start justify-center gap-2 text-sm text-muted-foreground",
                span { class: "flex items-center gap-2 text-foreground [&_svg]:size-4", Newspaper {} "nrzNews verbinden" }
                p { "In nrzNews unter Einstellungen → Konto → API-Tokens ein Token anlegen und unter System → nrzNews eintragen." }
                Link { to: Route::SystemPage {}, class: "hover:text-foreground", "Zu System →" }
            }
        },
        Some(Ok(Some(ov))) => {
            let metrics: Vec<_> = if widget.w < 4 {
                // schmal: die wichtigsten zwei
                ov.metrics.iter().filter(|m| m.key == "unread" || m.key == "today").cloned().collect()
            } else {
                ov.metrics.clone()
            };
            rsx! {
                div { class: "flex h-full min-h-0 flex-col gap-3",
                    if show_metrics {
                        div { class: "grid {cols} gap-2",
                            for m in metrics {
                                div {
                                    key: "{m.key}",
                                    class: if m.status == "warning" && m.value > 0 { "rounded-lg border border-warning/40 bg-warning/10 px-2.5 py-1.5" } else { "rounded-lg bg-secondary/40 px-2.5 py-1.5" },
                                    title: "{m.hint}",
                                    div { class: "truncate text-[11px] text-muted-foreground", "{m.label}" }
                                    div {
                                        class: if m.status == "warning" && m.value > 0 { "text-lg font-semibold tabular-nums text-warning" } else { "text-lg font-semibold tabular-nums" },
                                        "{m.value}"
                                    }
                                }
                            }
                        }
                    }
                    if headlines > 0 && widget.h >= 3 {
                        div { class: "flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto pr-1",
                            if ov.latest.is_empty() {
                                p { class: "text-sm text-muted-foreground", "Alles gelesen." }
                            }
                            for a in ov.latest.iter().cloned() {
                                a {
                                    key: "{a.id}",
                                    class: "group -mx-1.5 flex items-start gap-2 rounded-md px-1.5 py-1 hover:bg-accent/40",
                                    href: "{a.url}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    span { class: "mt-1.5 size-1.5 shrink-0 rounded-full bg-primary" }
                                    span { class: "min-w-0 flex-1",
                                        span { class: "line-clamp-1 text-sm group-hover:underline", "{a.title}" }
                                        span { class: "block truncate text-xs text-muted-foreground", "{a.feed_title} · {fmt_ago(a.published_at)}" }
                                    }
                                    if a.starred {
                                        Star { class: "mt-1 size-3.5 shrink-0 fill-current text-warning" }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "flex-1" }
                    }
                    if !ov.feed_errors.is_empty() && widget.h >= 3 {
                        p {
                            class: "flex items-center gap-1.5 truncate text-xs text-warning [&_svg]:size-3.5 [&_svg]:shrink-0",
                            title: ov.feed_errors.iter().map(|f| format!("{}: {}", f.title, f.error.clone().unwrap_or_default())).collect::<Vec<_>>().join("\n"),
                            TriangleAlert {}
                            span { class: "truncate",
                                "Fehler: "
                                {ov.feed_errors.iter().map(|f| f.title.clone()).collect::<Vec<_>>().join(", ")}
                            }
                        }
                    }
                    div { class: "flex items-center justify-between gap-2 text-xs text-muted-foreground",
                        span { class: "truncate",
                            match ov.last_fetched_at {
                                Some(ts) => format!("{} · abgerufen {}", ov.user, fmt_ago(ts)),
                                None => ov.user.clone(),
                            }
                        }
                        a { class: "shrink-0 hover:text-foreground", href: browser_host_url(&ov.source_url), target: "_blank", rel: "noopener noreferrer", "nrzNews →" }
                    }
                }
            }
        }
    }
}

// ---------- Clock ----------

fn now_parts() -> (String, String) {
    #[cfg(feature = "web")]
    {
        let d = js_sys::Date::new_0();
        const DAYS: [&str; 7] = ["Sonntag", "Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag"];
        const MONTHS: [&str; 12] =
            ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];
        let time = format!("{:02}:{:02}:{:02}", d.get_hours(), d.get_minutes(), d.get_seconds());
        let date = format!(
            "{}, {}. {} {}",
            DAYS.get(d.get_day() as usize).unwrap_or(&""),
            d.get_date(),
            MONTHS.get(d.get_month() as usize).unwrap_or(&""),
            d.get_full_year()
        );
        (time, date)
    }
    #[cfg(not(feature = "web"))]
    {
        (String::from("--:--:--"), String::new())
    }
}

#[component]
fn ClockWidget(widget: Widget) -> Element {
    let mut now = use_signal(now_parts);
    use_future(move || async move {
        loop {
            sleep_ms(1_000).await;
            now.set(now_parts());
        }
    });
    let (time, date) = now();
    let time = if widget.opt_bool("seconds", false) { time } else { time.chars().take(5).collect() };
    let big = if widget.h >= 2 && widget.w >= 3 { "text-5xl" } else { "text-3xl" };
    rsx! {
        div { class: "flex h-full flex-col items-center justify-center gap-1",
            div { class: "{big} font-semibold tabular-nums tracking-tight", "{time}" }
            if widget.opt_bool("date", true) && widget.h >= 2 {
                div { class: "text-sm text-muted-foreground", "{date}" }
            }
        }
    }
}

// ---------- Weather ----------

pub fn weather_label(code: i32) -> &'static str {
    match code {
        0 => "Klar",
        1 => "Überwiegend klar",
        2 => "Teilweise bewölkt",
        3 => "Bedeckt",
        45 | 48 => "Nebel",
        51..=57 => "Nieselregen",
        61..=67 | 80..=82 => "Regen",
        71..=77 | 85 | 86 => "Schnee",
        95..=99 => "Gewitter",
        _ => "–",
    }
}

fn weather_icon(code: i32, day: bool, class: &str) -> Element {
    let class = class.to_string();
    match code {
        0 | 1 => {
            if day {
                rsx! { Sun { class } }
            } else {
                rsx! { Moon { class } }
            }
        }
        2 => {
            if day {
                rsx! { CloudSun { class } }
            } else {
                rsx! { CloudMoon { class } }
            }
        }
        3 => rsx! { Cloud { class } },
        45 | 48 => rsx! { CloudFog { class } },
        51..=57 => rsx! { CloudDrizzle { class } },
        61..=67 | 80..=82 => rsx! { CloudRain { class } },
        71..=77 | 85 | 86 => rsx! { CloudSnow { class } },
        95..=99 => rsx! { CloudLightning { class } },
        _ => rsx! { Cloud { class } },
    }
}

fn weekday(iso: &str) -> String {
    #[cfg(feature = "web")]
    {
        let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(&format!("{iso}T12:00:00")));
        const D: [&str; 7] = ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"];
        D.get(d.get_day() as usize).map_or_else(|| iso.to_string(), |s| (*s).to_string())
    }
    #[cfg(not(feature = "web"))]
    {
        iso.to_string()
    }
}

#[component]
fn WeatherWidget(widget: Widget) -> Element {
    let lat = widget.opt_f64("lat").unwrap_or(47.3769);
    let lon = widget.opt_f64("lon").unwrap_or(8.5417);
    let city = widget.opt_str("city");
    let mut data = use_resource(use_reactive!(|lat, lon| async move { weather(lat, lon).await }));
    use_future(move || async move {
        loop {
            sleep_ms(600_000).await;
            data.restart();
        }
    });
    let wide = widget.w >= 4 || widget.h >= 3;
    match data.read().clone() {
        None => rsx! { div { class: "h-full animate-pulse rounded-lg bg-muted/40" } },
        Some(Err(e)) => rsx! { p { class: "text-sm text-destructive", {err_msg(&e)} } },
        Some(Ok(w)) => rsx! {
            div { class: "flex h-full flex-col justify-between gap-2",
                div { class: "flex items-center gap-3",
                    {weather_icon(w.code, w.is_day, "size-10 shrink-0")}
                    div { class: "min-w-0",
                        div { class: "text-3xl font-semibold tabular-nums", "{w.temperature:.0}°" }
                        div { class: "truncate text-xs text-muted-foreground", "{weather_label(w.code)} · {city}" }
                    }
                }
                if widget.h >= 2 {
                    div { class: "flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground",
                        span { class: "flex items-center gap-1 [&_svg]:size-3.5", Thermometer {} "gefühlt {w.apparent:.0}°" }
                        span { class: "flex items-center gap-1 [&_svg]:size-3.5", Wind {} "{w.wind:.0} km/h" }
                        span { class: "flex items-center gap-1 [&_svg]:size-3.5", Droplets {} "{w.humidity:.0} %" }
                    }
                }
                if wide {
                    div { class: "grid grid-cols-5 gap-1 border-t pt-2",
                        for (date, code, max, min) in w.daily.iter().skip(1).take(5).cloned() {
                            div { class: "flex flex-col items-center gap-0.5 text-xs",
                                span { class: "text-muted-foreground", {weekday(&date)} }
                                {weather_icon(code, true, "size-4")}
                                span { class: "tabular-nums", "{max:.0}°" }
                                span { class: "tabular-nums text-muted-foreground", "{min:.0}°" }
                            }
                        }
                    }
                }
            }
        },
    }
}

// ---------- Notes ----------

/// Markdown → HTML; raw HTML in the input is escaped, links open in a new tab.
pub fn markdown(text: &str) -> String {
    use pulldown_cmark::{Event, Options, Parser, html};
    let parser = Parser::new_ext(text, Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES)
        .map(|e| match e {
            Event::Html(s) | Event::InlineHtml(s) => Event::Text(s),
            other => other,
        });
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out.replace("<a href=\"javascript:", "<a data-blocked=\"").replace("<a href=", "<a target=\"_blank\" rel=\"noopener noreferrer\" href=")
}

#[component]
fn NotesWidget(widget: Widget, onchange: EventHandler<Widget>) -> Element {
    let text = widget.opt_str("text");
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| text.clone());
    let html = markdown(&text);
    rsx! {
        div { class: "group relative h-full",
            if editing() {
                textarea {
                    class: "h-full w-full resize-none rounded-md border border-input bg-transparent p-2 font-mono text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                    value: "{draft}",
                    autofocus: true,
                    oninput: move |e| draft.set(e.value()),
                    onblur: {
                        let widget = widget.clone();
                        move |_| {
                            editing.set(false);
                            let mut w = widget.clone();
                            if let Some(o) = w.options.as_object_mut() {
                                o.insert("text".into(), json!(draft()));
                            } else {
                                w.options = json!({ "text": draft() });
                            }
                            onchange.call(w);
                        }
                    },
                }
            } else {
                div {
                    class: "md h-full overflow-y-auto text-sm leading-relaxed",
                    ondoubleclick: move |_| {
                        draft.set(text.clone());
                        editing.set(true);
                    },
                    dangerous_inner_html: "{html}",
                }
                button {
                    class: "absolute bottom-0 right-0 hidden group-hover:flex size-7 items-center justify-center rounded-md border bg-background/90 text-muted-foreground hover:text-foreground",
                    title: "Bearbeiten",
                    onclick: move |_| editing.set(true),
                    Pencil { class: "size-3.5" }
                }
            }
        }
    }
}

// ---------- Bookmarks ----------

pub fn parse_bookmarks(s: &str) -> Vec<(String, String)> {
    s.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() {
                return None;
            }
            match l.split_once('|') {
                Some((t, u)) => Some((t.trim().to_string(), u.trim().to_string())),
                None => Some((l.to_string(), l.to_string())),
            }
        })
        .filter(|(_, u)| u.starts_with("http://") || u.starts_with("https://"))
        .collect()
}

#[component]
fn BookmarksWidget(widget: Widget) -> Element {
    let items = parse_bookmarks(&widget.opt_str("items"));
    rsx! {
        div { class: "flex h-full flex-col gap-0.5 overflow-y-auto -mx-2",
            if items.is_empty() {
                p { class: "px-2 text-sm text-muted-foreground", "Noch keine Links – in den Einstellungen hinzufügen." }
            }
            for (title, url) in items {
                a {
                    href: "{browser_url(&url)}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    class: "group flex items-center gap-2.5 rounded-md px-2 py-1.5 text-sm hover:bg-accent/50",
                    div { class: "flex size-6 shrink-0 items-center justify-center rounded bg-muted text-[11px] font-semibold text-muted-foreground",
                        {title.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()}
                    }
                    span { class: "truncate flex-1", "{title}" }
                    ExternalLink { class: "size-3.5 shrink-0 text-muted-foreground opacity-0 group-hover:opacity-100" }
                }
            }
        }
    }
}

// ---------- Search ----------

pub const ENGINES: [(&str, &str, &str); 5] = [
    ("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/"),
    ("google", "Google", "https://www.google.com/search"),
    ("startpage", "Startpage", "https://www.startpage.com/do/search"),
    ("brave", "Brave", "https://search.brave.com/search"),
    ("bing", "Bing", "https://www.bing.com/search"),
];

#[component]
fn SearchWidget(widget: Widget) -> Element {
    let engine = widget.opt_str("engine");
    let (_, name, action) = ENGINES.iter().find(|(k, _, _)| *k == engine).copied().unwrap_or(ENGINES[0]);
    rsx! {
        form { class: "flex h-full items-center gap-2 px-3", method: "get", action: "{action}", target: "_blank",
            Search { class: "size-4 shrink-0 text-muted-foreground" }
            input {
                class: "{input_class()} border-0 shadow-none bg-transparent dark:bg-transparent focus-visible:ring-0 px-1",
                name: "q",
                r#type: "search",
                autocomplete: "off",
                placeholder: "Mit {name} suchen …",
            }
            Globe { class: "size-4 shrink-0 text-muted-foreground" }
        }
    }
}

// ---------- iFrame ----------

#[component]
fn IframeWidget(widget: Widget) -> Element {
    let url = widget.opt_str("url");
    if url.is_empty() {
        return rsx! { p { class: "p-4 text-sm text-muted-foreground", "In den Einstellungen eine URL eintragen." } };
    }
    rsx! {
        iframe {
            class: "h-full w-full border-0 bg-background",
            src: "{browser_url(&url)}",
            "sandbox": "allow-scripts allow-same-origin allow-forms allow-popups",
            "referrerpolicy": "no-referrer",
            "loading": "lazy",
        }
    }
}
