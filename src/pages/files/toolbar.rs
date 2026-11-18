//! Top bar of the file manager: back/forward, breadcrumb, new folder, upload, search, view switch, sort.

use dioxus::prelude::*;
use icons::{
    ArrowUpDown, Check, ChevronLeft, ChevronRight, FolderOpen, FolderPlus, FolderUp, LayoutGrid, LayoutList, LoaderCircle,
    Search, SquareCheck, Upload, X,
};

use super::items::{DropTarget, root_label};
use super::logic::{self, Sort, SortKey, ViewMode};
use super::sidebar::place_icon;
use super::state::{Fm, KEY_SORT, KEY_VIEW, RECENT};
use super::web;
use crate::app::Route;

const PILL: &str = "inline-flex h-10 shrink-0 items-center gap-2 rounded-full border border-border px-4 text-sm font-medium text-foreground/90 transition-colors hover:bg-accent disabled:pointer-events-none disabled:opacity-40";
const ROUND: &str = "inline-flex size-10 shrink-0 items-center justify-center rounded-full border border-border text-foreground/90 transition-colors hover:bg-accent";
const ARROW: &str = "inline-flex size-8 shrink-0 items-center justify-center rounded-full text-foreground/80 transition-colors hover:bg-accent disabled:pointer-events-none disabled:opacity-30";
const MENU: &str = "absolute right-0 top-12 z-40 w-52 rounded-2xl border bg-popover p-1.5 shadow-xl";
const MENU_ITEM: &str = "flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm hover:bg-accent";
/// Highlight of the active view (fixed teal, matches the folder icons).
const SEG_ON: &str = "inline-flex h-8 w-10 items-center justify-center rounded-full bg-[#1f5f66] text-white";
const SEG_OFF: &str = "inline-flex h-8 w-10 items-center justify-center rounded-full text-muted-foreground hover:text-foreground";

#[component]
pub fn Toolbar() -> Element {
    let mut fm = use_context::<Fm>();
    let cur = fm.path.read().clone();
    let target = logic::is_target_dir(&cur);
    let searching = fm.results.read().is_some() || *fm.searching.read();
    let touch = *fm.touch.read();
    let select_mode = *fm.select_mode.read();
    let select_btn = rsx! {
        button {
            class: if select_mode { "{PILL} bg-accent" } else { "{PILL}" },
            onclick: move |_| {
                let on = !*fm.select_mode.peek();
                fm.select_mode.set(on);
                if !on {
                    fm.clear_selection();
                }
            },
            SquareCheck { class: "size-4" }
            "Auswählen"
        }
    };

    rsx! {
        div { class: "flex flex-col gap-2 md:h-12 md:flex-row md:items-center md:gap-3",
            // phones: places drawer + title
            div { class: "flex items-center gap-2 md:hidden",
                button {
                    class: ROUND,
                    "aria-label": "Orte",
                    title: "Orte",
                    onclick: move |_| fm.sidebar_open.set(true),
                    FolderOpen { class: "size-5" }
                }
                h1 { class: "min-w-0 flex-1 truncate text-3xl font-bold tracking-tight", "Dateien" }
                if touch && !searching {
                    {select_btn.clone()}
                }
            }

            div { class: "flex min-w-0 flex-1 items-center gap-1",
                button { class: ARROW, title: "Zurück (Alt+←)", disabled: !fm.can_go(-1), onclick: move |_| fm.go(-1), ChevronLeft { class: "size-5" } }
                button { class: ARROW, title: "Vorwärts (Alt+→)", disabled: !fm.can_go(1), onclick: move |_| fm.go(1), ChevronRight { class: "size-5" } }
                Breadcrumbs {}
            }

            div { class: "flex items-center gap-2",
                if touch && !searching {
                    div { class: "hidden md:block", {select_btn} }
                }
                // phones: "Neuer Ordner" lives in the floating upload menu
                div { class: "hidden md:block",
                    button { class: PILL, disabled: !target, title: "Neuer Ordner", onclick: move |_| fm.new_folder.set(true),
                        FolderPlus { class: "size-4" }
                        "Ordner"
                    }
                }
                UploadButton { disabled: !target }
                SearchBox {}
                ViewSwitch {}
                SortButton {}
            }
        }
    }
}

#[component]
fn Breadcrumbs() -> Element {
    let fm = use_context::<Fm>();
    let cur = fm.path.read().clone();
    let searching = fm.results.read().is_some() || *fm.searching.read();
    let item = "inline-flex shrink-0 items-center gap-2 rounded-lg px-2 py-1.5 text-[15px]";

    if searching {
        return rsx! {
            div { class: "{item} ml-1 text-foreground/90",
                Search { class: "size-4 text-muted-foreground" }
                "Suche"
            }
        };
    }
    if cur == RECENT {
        return rsx! {
            div { class: "{item} ml-1 text-foreground/90", {place_icon(RECENT, "size-4")} "Kürzlich" }
        };
    }
    if cur == "/" {
        return rsx! { div { class: "{item} ml-1 text-foreground/90", "Alle Orte" } };
    }

    let crumbs = logic::crumbs(&cur);
    let n = crumbs.len();
    rsx! {
        nav { class: "ml-1 flex min-w-0 items-center gap-0.5 overflow-x-auto whitespace-nowrap [scrollbar-width:none]",
            for (i, (name, p)) in crumbs.into_iter().enumerate() {
                div { key: "{p}", class: "flex shrink-0 items-center gap-0.5",
                    if i > 0 {
                        ChevronRight { class: "size-3.5 shrink-0 text-muted-foreground" }
                    }
                    DropTarget { dir: p.clone(), class: "rounded-lg",
                        Link {
                            to: Route::FilesPage { path: p.clone(), msg: String::new() },
                            class: if i + 1 == n { "{item} text-foreground" } else { "{item} text-muted-foreground hover:bg-accent hover:text-foreground" },
                            if i == 0 { {place_icon(&p, "size-4")} }
                            {if i == 0 { root_label(&p) } else { name }}
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn UploadButton(disabled: bool) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        div { class: "relative hidden md:block",
            button {
                class: PILL,
                disabled,
                title: "Hochladen (oder Dateien einfach hierher ziehen)",
                onclick: move |_| open.toggle(),
                Upload { class: "size-4" }
                "Hochladen"
            }
            if open() {
                div { class: "fixed inset-0 z-30", onclick: move |_| open.set(false) }
                div { class: MENU,
                    button { class: MENU_ITEM, onclick: move |_| { open.set(false); web::click("fm-pick-files"); },
                        Upload { class: "size-4" } "Dateien hochladen"
                    }
                    button { class: MENU_ITEM, onclick: move |_| { open.set(false); web::click("fm-pick-folder"); },
                        FolderUp { class: "size-4" } "Ordner hochladen"
                    }
                }
            }
        }
    }
}

#[component]
fn SearchBox() -> Element {
    let fm = use_context::<Fm>();
    rsx! {
        div { class: "relative min-w-0 flex-1 md:w-44 md:flex-none lg:w-56",
            Search { class: "pointer-events-none absolute left-3.5 top-3 size-4 text-muted-foreground" }
            input {
                class: "h-10 w-full rounded-full border border-border bg-transparent pl-10 pr-9 text-sm outline-none transition-colors placeholder:text-muted-foreground focus:border-ring focus:ring-2 focus:ring-ring/30 [&::-webkit-search-cancel-button]:hidden",
                r#type: "search",
                placeholder: "Suchen",
                title: "In /Home suchen",
                value: "{fm.query}",
                oninput: move |e| fm.search(e.value()),
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Escape {
                        fm.clear_search();
                    }
                },
            }
            if *fm.searching.read() {
                LoaderCircle { class: "absolute right-3.5 top-3 size-4 animate-spin text-muted-foreground" }
            } else if !fm.query.read().is_empty() {
                button {
                    class: "absolute right-2 top-2 rounded-full p-1 text-muted-foreground hover:text-foreground",
                    "aria-label": "Suche leeren",
                    onclick: move |_| fm.clear_search(),
                    X { class: "size-4" }
                }
            }
        }
    }
}

#[component]
fn ViewSwitch() -> Element {
    let mut fm = use_context::<Fm>();
    let view = *fm.view.read();
    let mut set = move |v: ViewMode| {
        fm.view.set(v);
        web::store_set(KEY_VIEW, v.key());
    };
    rsx! {
        div { class: "inline-flex h-10 shrink-0 items-center rounded-full border border-border p-0.5",
            button { class: if view == ViewMode::Grid { SEG_ON } else { SEG_OFF }, title: "Raster", onclick: move |_| set(ViewMode::Grid),
                LayoutGrid { class: "size-4" }
            }
            button { class: if view == ViewMode::List { SEG_ON } else { SEG_OFF }, title: "Liste", onclick: move |_| set(ViewMode::List),
                LayoutList { class: "size-4" }
            }
        }
    }
}

#[component]
fn SortButton() -> Element {
    let mut fm = use_context::<Fm>();
    let mut open = use_signal(|| false);
    let sort = *fm.sort.read();
    let mut set = move |s: Sort| {
        fm.sort.set(s);
        web::store_set(KEY_SORT, &s.encode());
    };
    let keys = [(SortKey::Name, "Name"), (SortKey::Size, "Grösse"), (SortKey::Modified, "Geändert"), (SortKey::Type, "Typ")];
    rsx! {
        div { class: "relative",
            button { class: ROUND, title: "Sortieren", onclick: move |_| open.toggle(), ArrowUpDown { class: "size-4" } }
            if open() {
                div { class: "fixed inset-0 z-30", onclick: move |_| open.set(false) }
                div { class: MENU,
                    div { class: "px-3 pb-1 pt-1 text-xs text-muted-foreground", "Sortieren nach" }
                    for (k, label) in keys {
                        button {
                            key: "{label}",
                            class: MENU_ITEM,
                            onclick: move |_| set(Sort { key: k, asc: if sort.key == k { sort.asc } else { k == SortKey::Name || k == SortKey::Type } }),
                            span { class: "flex w-4 justify-center", if sort.key == k { Check { class: "size-4" } } }
                            "{label}"
                        }
                    }
                    div { class: "my-1 h-px bg-border" }
                    button { class: MENU_ITEM, onclick: move |_| set(Sort { key: sort.key, asc: true }),
                        span { class: "flex w-4 justify-center", if sort.asc { Check { class: "size-4" } } }
                        "Aufsteigend"
                    }
                    button { class: MENU_ITEM, onclick: move |_| set(Sort { key: sort.key, asc: false }),
                        span { class: "flex w-4 justify-center", if !sort.asc { Check { class: "size-4" } } }
                        "Absteigend"
                    }
                }
            }
        }
    }
}
