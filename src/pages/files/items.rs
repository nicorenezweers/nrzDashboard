//! Entries in grid and list view, file icons and drop targets.

use dioxus::prelude::*;
use icons::{
    Boxes, Check, Ellipsis, File, FileArchive, FileCode, FileText, Film, Folder, HardDrive, House, Image, Music, Network,
    Usb,
};

use super::art::{emblem_for, file_art, folder_art};
use super::logic::{self, Kind, ViewMode};
use super::state::{Fm, MenuState, inline_url};
use super::web;
use crate::model::FileEntry;
use crate::utils::{fmt_bytes, fmt_time_ms};

pub fn kind_icon(kind: Kind, class: &str) -> Element {
    let class = class.to_string();
    match kind {
        Kind::Folder => rsx! { Folder { class } },
        Kind::Image => rsx! { Image { class } },
        Kind::Video => rsx! { Film { class } },
        Kind::Audio => rsx! { Music { class } },
        Kind::Pdf | Kind::Text => rsx! { FileText { class } },
        Kind::Code => rsx! { FileCode { class } },
        Kind::Archive => rsx! { FileArchive { class } },
        Kind::Other => rsx! { File { class } },
    }
}

/// Icon for a top-level location (`/Home`, `/Apps`, …).
pub fn root_icon(path: &str, class: &str) -> Element {
    let class = class.to_string();
    match path {
        "/Home" => rsx! { House { class } },
        "/Apps" => rsx! { Boxes { class } },
        "/External" => rsx! { Usb { class } },
        "/Network" => rsx! { Network { class } },
        _ => rsx! { HardDrive { class } },
    }
}

pub fn root_label(path: &str) -> String {
    match path {
        "/Home" => "Home".into(),
        "/Apps" => "Apps".into(),
        "/External" => "Extern".into(),
        "/Network" => "Netzwerk".into(),
        other => logic::name_of(other),
    }
}

/// Wraps children in a drop zone for a folder (breadcrumbs, sidebar): move or upload into `dir`.
#[component]
pub fn DropTarget(dir: String, class: String, #[props(into, default)] active_class: String, children: Element) -> Element {
    let fm = use_context::<Fm>();
    let active = fm.drop_target.read().as_deref() == Some(dir.as_str());
    let (d1, d2, d3) = (dir.clone(), dir.clone(), dir.clone());
    rsx! {
        div {
            class: if active { "{class} {active_class} ring-2 ring-primary bg-primary/10" } else { "{class}" },
            ondragover: move |e: DragEvent| fm.drag_over(&d1, &e),
            ondragleave: move |_| fm.drag_leave(&d2),
            ondrop: move |e: DragEvent| fm.drop_on(&d3, &e),
            {children}
        }
    }
}

/// One file/folder (grid tile or list row) with selection, drag & drop, context menu and inline rename.
#[component]
pub fn Item(entry: FileEntry, mode: ViewMode, #[props(default)] show_path: bool) -> Element {
    let fm = use_context::<Fm>();
    let path = entry.path.clone();
    let is_dir = entry.is_dir();
    let selected = fm.is_selected(&path);
    let renaming = fm.renaming.read().as_deref() == Some(path.as_str());
    let drop_active = is_dir && fm.drop_target.read().as_deref() == Some(path.as_str());
    let dragging = fm.drag.read().as_ref().is_some_and(|d| d.contains(&path));
    let touch = *fm.touch.read();
    let checkboxes = *fm.select_mode.read() || !fm.selected.read().is_empty();
    let mut thumb_failed = use_signal(|| false);

    let size = match (is_dir, entry.size) {
        (_, Some(s)) => fmt_bytes(s),
        (true, None) => "Ordner".into(),
        _ => "–".into(),
    };
    let modified = entry.modified.map(fmt_time_ms).unwrap_or_default();
    let location = logic::parent(&path);
    let type_label = logic::type_label(&entry);

    let state_class = if drop_active {
        "ring-2 ring-primary bg-primary/15"
    } else if selected {
        "bg-accent"
    } else {
        "hover:bg-accent/40"
    };
    let fade = if dragging { "opacity-50" } else { "" };

    // ---- handlers (shared by both layouts) ----
    let e_click = entry.clone();
    let onclick = move |e: MouseEvent| {
        e.stop_propagation();
        let m = e.modifiers();
        fm.click(&e_click, m.meta() || m.ctrl(), m.shift());
    };
    let e_open = entry.clone();
    let ondoubleclick = move |e: MouseEvent| {
        e.stop_propagation();
        if !*fm.touch.peek() && !*fm.select_mode.peek() {
            fm.open(&e_open);
        }
    };
    let p_menu = path.clone();
    let oncontextmenu = move |e: MouseEvent| {
        e.prevent_default();
        e.stop_propagation();
        let mut fm = fm;
        if *fm.touch.peek() {
            // Long press on phones: start selecting.
            fm.select_mode.set(true);
            if !fm.is_selected(&p_menu) {
                fm.toggle(&p_menu);
            }
            return;
        }
        if !fm.selected.peek().contains(&p_menu) {
            fm.select_only(&p_menu);
        }
        let c = e.client_coordinates();
        fm.menu.set(Some(MenuState { x: c.x, y: c.y, path: p_menu.clone() }));
    };
    let p_drag = path.clone();
    let ondragstart = move |e: DragEvent| {
        e.stop_propagation();
        web::start_drag(&e, &p_drag);
        fm.start_drag(&p_drag);
    };
    let ondragend = move |_| fm.end_drag();
    let (p_over, p_leave, p_drop) = (path.clone(), path.clone(), path.clone());
    let ondragover = move |e: DragEvent| {
        if is_dir {
            fm.drag_over(&p_over, &e);
        }
    };
    let ondragleave = move |_| {
        if is_dir {
            fm.drag_leave(&p_leave);
        }
    };
    let ondrop = move |e: DragEvent| {
        if is_dir {
            fm.drop_on(&p_drop, &e);
        }
    };

    let p_check = path.clone();
    let checkbox = rsx! {
        button {
            class: if selected { "inline-flex size-5 shrink-0 items-center justify-center rounded border border-primary bg-primary text-primary-foreground" } else { "inline-flex size-5 shrink-0 items-center justify-center rounded border border-muted-foreground/60 bg-background/70" },
            "aria-label": "Auswählen",
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                fm.toggle(&p_check);
            },
            ondoubleclick: move |e: MouseEvent| e.stop_propagation(),
            if selected { Check { class: "size-3.5" } }
        }
    };

    let p_more = path.clone();
    let more = rsx! {
        button {
            class: "inline-flex size-9 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground",
            "aria-label": "Aktionen",
            title: "Aktionen",
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                let mut fm = fm;
                if !fm.selected.peek().contains(&p_more) {
                    fm.select_only(&p_more);
                }
                let c = e.client_coordinates();
                fm.menu.set(Some(MenuState { x: c.x, y: c.y, path: p_more.clone() }));
            },
            ondoubleclick: move |e: MouseEvent| e.stop_propagation(),
            Ellipsis { class: "size-4" }
        }
    };

    let name_el = if renaming {
        rsx! { RenameField { path: path.clone(), name: entry.name.clone(), is_dir } }
    } else {
        rsx! { span { class: "truncate", title: "{entry.name}", "{entry.name}" } }
    };

    let thumb = !is_dir && logic::has_thumbnail(&entry.name) && !thumb_failed();
    let art = |dir_class: &str, file_class: &str, img_class: &str| -> Element {
        if thumb {
            rsx! {
                img {
                    src: inline_url(&entry),
                    alt: "",
                    loading: "lazy",
                    decoding: "async",
                    draggable: "false",
                    class: "{img_class} object-cover shadow-sm ring-1 ring-border",
                    onerror: move |_| thumb_failed.set(true),
                }
            }
        } else if is_dir {
            folder_art(emblem_for(&entry.name), dir_class)
        } else {
            file_art(&entry.name, file_class)
        }
    };

    match mode {
        ViewMode::Grid => rsx! {
            div {
                "data-path": "{entry.path}",
                class: "group relative flex min-w-0 select-none [-webkit-touch-callout:none] flex-col items-center rounded-2xl px-1.5 pb-3 pt-3 text-center transition-colors md:px-2 md:pt-4 {state_class} {fade}",
                draggable: if renaming || touch { "false" } else { "true" },
                onclick,
                ondoubleclick,
                oncontextmenu,
                ondragstart,
                ondragend,
                ondragover,
                ondragleave,
                ondrop,
                div { class: "flex h-[4.25rem] w-full items-center justify-center md:h-[5.25rem]",
                    {art("w-[4.5rem] md:w-[5.5rem]", "h-[3.9rem] md:h-[4.75rem]", "size-[3.9rem] rounded-xl md:size-[4.75rem]")}
                }
                div { class: if checkboxes || selected { "absolute left-2 top-2" } else if touch { "hidden" } else { "absolute left-2 top-2 opacity-0 group-hover:opacity-100" },
                    {checkbox}
                }
                if !touch {
                    div { class: "absolute right-0.5 top-0.5 opacity-0 group-hover:opacity-100 focus-within:opacity-100", {more} }
                }
                div { class: "mt-2 flex w-full min-w-0 justify-center text-[13px] leading-5 text-foreground md:text-sm", {name_el} }
                div { class: "w-full truncate text-xs leading-5 tabular-nums text-muted-foreground md:text-[13px]",
                    if show_path { "{location}" } else { "{size}" }
                }
            }
        },
        ViewMode::List => rsx! {
            div {
                "data-path": "{entry.path}",
                class: "group flex min-h-14 md:min-h-12 select-none [-webkit-touch-callout:none] items-center gap-3 rounded-xl px-2 transition-colors md:px-3 {state_class} {fade}",
                draggable: if renaming || touch { "false" } else { "true" },
                onclick,
                ondoubleclick,
                oncontextmenu,
                ondragstart,
                ondragend,
                ondragover,
                ondragleave,
                ondrop,
                div { class: if checkboxes || selected { "flex w-6 shrink-0 justify-center" } else if touch { "hidden" } else { "flex w-6 shrink-0 justify-center opacity-0 group-hover:opacity-100" },
                    {checkbox}
                }
                div { class: "flex w-9 shrink-0 items-center justify-center",
                    {art("w-9", "h-8", "size-8 rounded-md")}
                }
                div { class: "min-w-0 flex-1 py-1.5",
                    div { class: "flex min-w-0 text-sm", {name_el} }
                    div { class: "truncate text-xs text-muted-foreground md:hidden",
                        if show_path { "{location}" } else { "{size} · {modified}" }
                    }
                    if show_path {
                        div { class: "hidden md:block truncate text-xs text-muted-foreground", "{location}" }
                    }
                }
                div { class: "hidden md:block w-24 shrink-0 text-right text-sm tabular-nums text-muted-foreground", "{size}" }
                div { class: "hidden md:block w-40 shrink-0 text-sm tabular-nums text-muted-foreground", "{modified}" }
                div { class: "hidden lg:block w-28 shrink-0 truncate text-sm text-muted-foreground", "{type_label}" }
                div { class: if touch { "shrink-0" } else { "shrink-0 md:opacity-0 md:group-hover:opacity-100 focus-within:opacity-100" }, {more} }
            }
        },
    }
}

/// Inline rename: Enter saves, Escape cancels, leaving the field saves.
#[component]
fn RenameField(path: String, name: String, is_dir: bool) -> Element {
    let fm = use_context::<Fm>();
    let mut draft = use_signal(|| name.clone());
    let stem = logic::stem_utf16_len(&name, is_dir);
    let (p_key, p_blur) = (path.clone(), path.clone());
    rsx! {
        input {
            class: "h-7 w-full min-w-0 rounded-md border border-ring bg-background px-2 text-sm outline-none ring-2 ring-ring/40",
            value: "{draft}",
            onmounted: move |e| web::select_range(&e, stem),
            oninput: move |e| draft.set(e.value()),
            onclick: move |e: MouseEvent| e.stop_propagation(),
            ondoubleclick: move |e: MouseEvent| e.stop_propagation(),
            onkeydown: move |e: KeyboardEvent| {
                e.stop_propagation();
                match e.key() {
                    Key::Enter => {
                        e.prevent_default();
                        fm.rename(p_key.clone(), draft());
                        web::focus("fm-root");
                    }
                    Key::Escape => {
                        let mut fm = fm;
                        fm.renaming.set(None);
                        web::focus("fm-root");
                    }
                    _ => {}
                }
            },
            onblur: move |_| {
                if fm.renaming.peek().as_deref() == Some(p_blur.as_str()) {
                    fm.rename(p_blur.clone(), draft());
                }
            },
        }
    }
}
