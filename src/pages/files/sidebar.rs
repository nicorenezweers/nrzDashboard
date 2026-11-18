//! Sidebar of the file manager (like the umbrelOS Files app): big title, places, recent files,
//! favourites (drop folders on the heading), network shares and external drives.
//! On phones the same list opens as a drawer.

use dioxus::prelude::*;
use icons::{Boxes, Clock, Globe, HardDrive, House, StarOff, Usb};

use super::art::{emblem_for, folder_art};
use super::items::DropTarget;
use super::logic;
use super::state::{Fm, RECENT};
use super::web;
use crate::app::Route;
use crate::components::ui::skeleton::Skeleton;

/// Coloured icon of a top-level location (sidebar and breadcrumb).
pub fn place_icon(path: &str, class: &str) -> Element {
    let class = class.to_string();
    match path {
        "/Home" => rsx! { House { class: "{class} text-teal-400" } },
        "/Apps" => rsx! { Boxes { class: "{class} text-violet-400" } },
        "/Network" => rsx! { Globe { class: "{class} text-emerald-400" } },
        "/External" => rsx! { Usb { class: "{class} text-zinc-300" } },
        RECENT => rsx! { Clock { class: "{class} text-sky-400" } },
        _ => rsx! { HardDrive { class: "{class} text-zinc-300" } },
    }
}

#[component]
pub fn Sidebar() -> Element {
    let mut fm = use_context::<Fm>();
    let open = *fm.sidebar_open.read();
    rsx! {
        aside { class: "hidden w-60 shrink-0 flex-col md:flex lg:w-64",
            div { class: "flex h-12 items-center px-2",
                h1 { class: "text-[2.6rem] font-bold leading-none tracking-tight", "Dateien" }
            }
            div { class: "mt-5 min-h-0 flex-1 overflow-y-auto pb-4 pr-1", Places {} }
        }
        if open {
            div {
                class: "fixed inset-0 z-50 bg-background/70 backdrop-blur-sm md:hidden",
                onclick: move |_| fm.sidebar_open.set(false),
                div {
                    class: "absolute inset-y-0 left-0 flex w-[min(20rem,85vw)] flex-col rounded-r-3xl bg-card px-3 pb-[calc(1rem+env(safe-area-inset-bottom,0px))] pt-[calc(1rem+env(safe-area-inset-top,0px))] shadow-2xl",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "px-2 pb-4 text-3xl font-bold tracking-tight", "Dateien" }
                    div { class: "min-h-0 flex-1 overflow-y-auto", Places {} }
                }
            }
        }
    }
}

#[component]
fn Places() -> Element {
    let fm = use_context::<Fm>();
    let roots = fm.roots.read().clone();
    let has = |p: &str| roots.iter().any(|r| r.path == p);
    let cur = fm.path.read().clone();
    // Favourites directly in /Home that vanished (deleted elsewhere) are hidden.
    let home = fm.home_dirs.read().clone();
    let favs: Vec<String> = fm
        .fav_list()
        .into_iter()
        .filter(|f| home.is_empty() || logic::parent(f) != "/Home" || home.iter().any(|d| &d.path == f))
        .collect();
    let ext = fm.ext_dirs.read().clone();
    let net = fm.net_dirs.read().clone();
    let device = fm.device.read().clone();
    let umbrel = if device.is_empty() || device.eq_ignore_ascii_case("umbrel") { "Umbrel".to_string() } else { device };

    // The most specific entry containing the open folder is highlighted.
    let mut candidates: Vec<String> = ["/Home", "/Apps", "/Network", "/External"].iter().map(|s| s.to_string()).collect();
    candidates.extend(favs.iter().cloned());
    candidates.extend(ext.iter().map(|e| e.path.clone()));
    candidates.extend(net.iter().map(|e| e.path.clone()));
    let active = if cur == RECENT {
        RECENT.to_string()
    } else {
        candidates.into_iter().filter(|p| logic::is_within(&cur, p)).max_by_key(String::len).unwrap_or_default()
    };
    let is = |p: &str| active == p;

    if roots.is_empty() {
        return rsx! {
            div { class: "flex flex-col gap-1.5",
                for i in 0..6 {
                    Skeleton { key: "{i}", class: "h-10 w-full rounded-xl" }
                }
            }
        };
    }

    rsx! {
        nav { class: "flex flex-col gap-0.5",
            if has("/Home") {
                SideItem { path: "/Home", label: umbrel, active: is("/Home"), drop: true, icon: place_icon("/Home", "size-5") }
            }
            SideItem { path: RECENT, label: "Kürzlich", active: is(RECENT), icon: place_icon(RECENT, "size-5") }
            if has("/Apps") {
                SideItem { path: "/Apps", label: "Apps", active: is("/Apps"), drop: true, icon: place_icon("/Apps", "size-5") }
            }

            Divider {}
            FavHeading {}
            for f in favs {
                SideItem {
                    key: "{f}",
                    path: f.clone(),
                    label: logic::name_of(&f),
                    active: is(&f),
                    drop: true,
                    removable: true,
                    icon: folder_art(emblem_for(&logic::name_of(&f)), "w-6"),
                }
            }

            if has("/Network") {
                Divider {}
                SideItem { path: "/Network", label: "Netzwerkgeräte", active: is("/Network"), drop: true, icon: place_icon("/Network", "size-5") }
                for n in net {
                    SideItem { key: "{n.path}", path: n.path.clone(), label: n.name.clone(), active: is(&n.path), drop: true, indent: true, icon: folder_art(emblem_for(""), "w-5") }
                }
            }

            if has("/External") && !ext.is_empty() {
                Divider {}
                div { class: "px-3 pb-1 pt-1 text-[13px] text-muted-foreground", "Externer Speicher" }
                for d in ext {
                    SideItem { key: "{d.path}", path: d.path.clone(), label: d.name.clone(), active: is(&d.path), drop: true, icon: place_icon(&d.path, "size-5") }
                }
            }
        }
    }
}

#[component]
fn Divider() -> Element {
    rsx! { div { class: "mx-3 my-3 h-px bg-border" } }
}

/// One sidebar entry; real folders accept drops (move / upload into it).
#[component]
fn SideItem(
    #[props(into)] path: String,
    #[props(into)] label: String,
    active: bool,
    icon: Element,
    #[props(default)] drop: bool,
    #[props(default)] removable: bool,
    #[props(default)] indent: bool,
) -> Element {
    let mut fm = use_context::<Fm>();
    let touch = *fm.touch.read();
    let row = format!(
        "flex h-10 items-center gap-3 rounded-xl pr-3 text-[15px] transition-colors {} {}",
        if indent { "pl-9" } else { "pl-3" },
        if active { "bg-accent font-medium text-foreground" } else { "text-foreground/85 hover:bg-accent/50" }
    );
    let p_rm = path.clone();
    let link = rsx! {
        Link {
            to: Route::FilesPage { path: path.clone(), msg: String::new() },
            class: "{row}",
            span { class: "flex w-6 shrink-0 items-center justify-center", {icon} }
            span { class: "min-w-0 flex-1 truncate", "{label}" }
        }
    };
    rsx! {
        div {
            class: "group relative",
            onclick: move |_| fm.sidebar_open.set(false),
            if drop {
                DropTarget { dir: path.clone(), class: "rounded-xl", {link} }
            } else {
                {link}
            }
            if removable && !touch {
                button {
                    class: "absolute right-1.5 top-1/2 hidden size-7 -translate-y-1/2 items-center justify-center rounded-lg text-muted-foreground hover:bg-background/60 hover:text-foreground group-hover:inline-flex",
                    title: "Aus Favoriten entfernen",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        e.prevent_default();
                        fm.remove_fav(&p_rm);
                    },
                    StarOff { class: "size-3.5" }
                }
            }
        }
    }
}

/// Folders of the current in-app drag that are not favourites yet.
fn dragged_dirs(fm: Fm) -> Vec<String> {
    let favs = fm.fav_list();
    fm.drag
        .peek()
        .as_ref()
        .map(|d| d.iter().filter(|p| !favs.contains(p) && fm.find(p).is_some_and(|e| e.is_dir())).cloned().collect())
        .unwrap_or_default()
}

/// "Favoriten" heading; dropping folders on it adds them as favourites.
#[component]
fn FavHeading() -> Element {
    let mut fm = use_context::<Fm>();
    let mut hover = use_signal(|| false);
    let dragging = fm.drag.read().is_some() && !dragged_dirs(fm).is_empty();
    let class = if hover() {
        "flex h-8 items-center justify-between rounded-xl px-3 text-[13px] text-foreground ring-2 ring-primary bg-primary/10"
    } else if dragging {
        "flex h-8 items-center justify-between rounded-xl px-3 text-[13px] text-muted-foreground outline-dashed outline-1 outline-muted-foreground/50"
    } else {
        "flex h-8 items-center justify-between rounded-xl px-3 text-[13px] text-muted-foreground"
    };
    rsx! {
        div {
            class,
            ondragover: move |e: DragEvent| {
                if dragged_dirs(fm).is_empty() {
                    return;
                }
                e.prevent_default();
                e.stop_propagation();
                web::set_drop_effect(&e, "move");
                if !hover() {
                    hover.set(true);
                }
            },
            ondragleave: move |_| hover.set(false),
            ondrop: move |e: DragEvent| {
                let dirs = dragged_dirs(fm);
                if dirs.is_empty() {
                    return;
                }
                e.prevent_default();
                e.stop_propagation();
                hover.set(false);
                fm.end_drag();
                fm.ext_drag.set(0);
                fm.add_favs(dirs);
            },
            span { "Favoriten" }
            if dragging {
                span { class: "text-xs", "Hier ablegen" }
            }
        }
    }
}
