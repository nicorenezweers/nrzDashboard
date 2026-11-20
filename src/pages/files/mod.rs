//! Dateimanager (`/files`) im Stil der umbrelOS-Files-App: eigene Seitenleiste (Orte, Kürzlich, Favoriten,
//! Netzwerk, externe Laufwerke), Raster/Liste, Mehrfachauswahl, Drag & Drop (Upload und Verschieben),
//! Vorschau, Kontextmenü, Suche in /Home, Live-Aktualisierung per Polling.

mod art;
mod editor;
mod items;
mod logic;
mod marquee;
mod overlays;
mod sidebar;
mod state;
mod toolbar;
mod upload;
mod web;

use dioxus::prelude::*;
use icons::{
    ArrowDown, ArrowUp, Clock, CloudUpload, FolderPlus, FolderUp, LoaderCircle, Plus, Search,
    Upload, X,
};

use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::skeleton::Skeleton;
use crate::components::widgets::ErrorBox;

use items::Item;
use logic::{SortKey, ViewMode};
use overlays::{
    ContextMenu, DropOverlay, FolderPicker, NewFolderDialog, PreviewDialog, SelectionBar, Toasts,
};
use sidebar::Sidebar;
use state::{Fm, KEY_SORT, RECENT};
use toolbar::Toolbar;
use upload::UploadPanel;

const POLL_MS: u32 = 5000;

#[component]
pub fn FilesPage(path: String, msg: String) -> Element {
    let path = if path.is_empty() {
        "/".to_string()
    } else {
        path
    };
    let initial = path.clone();
    let mut fm = use_context_provider(move || Fm::new(initial));
    let marquee_state = use_signal(|| None::<marquee::Marquee>);

    // Folder from the route (also on back/forward).
    use_effect(use_reactive!(|path| fm.enter(path)));

    use_hook(move || {
        if !msg.is_empty() {
            fm.toast(msg.clone());
        }
    });

    // Browser-only setup (effects never run during the server render).
    use_effect(move || fm.init_client());

    // Live updates: poll the open folder.
    use_future(move || async move {
        let mut tick = 0u32;
        loop {
            crate::utils::sleep_ms(POLL_MS).await;
            let busy =
                fm.drag.peek().is_some() || fm.renaming.peek().is_some() || *fm.ext_drag.peek() > 0;
            if web::page_visible() && !busy {
                fm.reload();
                tick += 1;
                if tick % 6 == 0 {
                    fm.load_sidebar();
                }
            }
        }
    });

    use_drop(upload::abort_all);

    let cur = fm.path.read().clone();

    let onkeydown = move |e: KeyboardEvent| {
        if web::from_text_field(&e) {
            return;
        }
        let key = e.key();
        let m = e.modifiers();
        let cmd = m.meta() || m.ctrl();
        if fm.preview.peek().is_some() {
            match key {
                Key::ArrowLeft | Key::ArrowUp => fm.preview_step(-1),
                Key::ArrowRight | Key::ArrowDown => fm.preview_step(1),
                Key::Escape => fm.preview.set(None),
                Key::Character(ref c) if c == " " => fm.preview.set(None),
                _ => return,
            }
            e.prevent_default();
            return;
        }
        if fm.menu.peek().is_some() || fm.picker.peek().is_some() || *fm.new_folder.peek() {
            if key == Key::Escape {
                fm.menu.set(None);
                fm.picker.set(None);
                fm.new_folder.set(false);
            }
            return;
        }
        if m.alt() && matches!(key, Key::ArrowLeft | Key::ArrowRight) {
            fm.go(if key == Key::ArrowLeft { -1 } else { 1 });
            e.prevent_default();
            return;
        }
        let sel = fm.selection();
        match key {
            Key::Character(ref c) if cmd && c.eq_ignore_ascii_case("a") => fm.select_all(),
            Key::Delete => fm.trash(sel),
            Key::Backspace if cmd => fm.trash(sel),
            Key::F2 | Key::Enter if sel.len() == 1 => fm.renaming.set(sel.into_iter().next()),
            Key::Character(ref c) if c == " " && sel.len() == 1 => {
                if let Some(entry) = fm.find(&sel[0]) {
                    fm.open(&entry);
                }
            }
            Key::ArrowDown | Key::ArrowRight => fm.step(1),
            Key::ArrowUp | Key::ArrowLeft => fm.step(-1),
            Key::Escape => {
                fm.clear_selection();
                fm.select_mode.set(false);
            }
            _ => return,
        }
        e.prevent_default();
    };

    rsx! {
        div {
            id: "fm-root",
            tabindex: "-1",
            class: "relative flex h-full min-h-0 w-full flex-col gap-3 outline-none md:flex-row md:gap-5",
            onkeydown,
            ondragenter: move |e: DragEvent| {
                if fm.drag.peek().is_none() && web::drag_has_files(&e) {
                    *fm.ext_drag.write() += 1;
                }
            },
            ondragleave: move |_| {
                if *fm.ext_drag.peek() > 0 {
                    *fm.ext_drag.write() -= 1;
                }
            },
            ondragover: move |e: DragEvent| {
                if fm.drag.peek().is_none() && web::drag_has_files(&e) {
                    e.prevent_default();
                    let ok = logic::is_target_dir(&fm.path.peek());
                    web::set_drop_effect(&e, if ok { "copy" } else { "none" });
                    if fm.drop_target.peek().is_some() {
                        fm.drop_target.set(None);
                    }
                }
            },
            ondrop: move |e: DragEvent| {
                e.prevent_default();
                fm.ext_drag.set(0);
                if fm.drag.peek().is_none() {
                    let dir = fm.path.peek().clone();
                    if logic::is_target_dir(&dir) {
                        upload::from_drop(fm, &e, dir);
                    } else {
                        fm.toast_err("Bitte zuerst einen Ordner öffnen (z. B. Home)");
                    }
                }
            },
            onmousemove: move |_| {
                // A cancelled OS drag leaves no dragleave behind; the next mouse move clears the overlay.
                if *fm.ext_drag.peek() != 0 {
                    fm.ext_drag.set(0);
                }
            },

            // hidden pickers
            input {
                id: "fm-pick-files",
                r#type: "file",
                multiple: true,
                class: "hidden",
                onchange: move |e: FormEvent| upload::from_input(fm, &e, fm.path.peek().clone(), false),
            }
            input {
                id: "fm-pick-folder",
                r#type: "file",
                multiple: true,
                "webkitdirectory": "true",
                class: "hidden",
                onchange: move |e: FormEvent| upload::from_input(fm, &e, fm.path.peek().clone(), true),
            }

            Sidebar {}

            div { class: "flex min-h-0 min-w-0 flex-1 flex-col gap-3 md:gap-4",
                Toolbar {}
                UploadPanel {}
                div { class: "flex min-h-0 flex-1 flex-col overflow-hidden rounded-3xl bg-card",
                    div {
                        id: marquee::SCROLL_ID,
                        class: "relative min-h-0 flex-1 overflow-y-auto p-2 md:p-6",
                        onclick: move |_| {
                            if !*fm.select_mode.peek() {
                                fm.clear_selection();
                            }
                        },
                        onpointerdown: move |e: PointerEvent| marquee::begin(&e, fm, marquee_state),
                        Content {}
                        marquee::MarqueeRect { state: marquee_state }
                    }
                    StatusLine {}
                }
            }

            // phones: upload button within thumb reach
            if fm.selected.read().is_empty() && !*fm.select_mode.read() && logic::is_target_dir(&cur) {
                MobileFab {}
            }

            SelectionBar {}
            ContextMenu {}
            PreviewDialog {}
            editor::EditorDialog {}
            marquee::MarqueeCapture { state: marquee_state }
            FolderPicker {}
            NewFolderDialog {}
            DropOverlay {}
            Toasts {}
        }
    }
}

/// "7 Elemente" / "2 von 7 ausgewählt" at the bottom right of the panel.
#[component]
fn StatusLine() -> Element {
    let fm = use_context::<Fm>();
    if !*fm.loaded.read() && fm.results.read().is_none() {
        return rsx! {};
    }
    let total = fm.visible().len();
    let sel = fm.selected.read().len();
    let more = *fm.has_more.read() && fm.results.read().is_none();
    let plus = if more { "+" } else { "" };
    let text = if sel > 0 {
        format!("{sel} von {total}{plus} ausgewählt")
    } else if total == 1 {
        "1 Element".to_string()
    } else {
        format!("{total}{plus} Elemente")
    };
    rsx! {
        div { class: "flex shrink-0 justify-start px-5 md:justify-end pb-3 pt-1 text-xs font-medium tabular-nums text-foreground/80 md:px-6 md:pb-4 md:text-sm", "{text}" }
    }
}

#[component]
fn Content() -> Element {
    let mut fm = use_context::<Fm>();
    let view = *fm.view.read();
    let sort = *fm.sort.read();
    let in_search = fm.results.read().is_some();
    let show_path = in_search || *fm.path.read() == RECENT;

    if *fm.searching.read() && !in_search {
        return rsx! { ListSkeleton {} };
    }
    if let Some(Err(e)) = &*fm.results.read() {
        return rsx! { ErrorBox { message: e.clone() } };
    }
    if !in_search {
        if let Some(e) = fm.error.read().clone() {
            return rsx! { ErrorBox { message: e } };
        }
        if !*fm.loaded.read() {
            return rsx! { ListSkeleton {} };
        }
    }

    let list = fm.visible();
    let total = list.len();
    let all_selected = total > 0 && fm.selected.read().len() == total;
    let header_btn = "inline-flex items-center gap-1 hover:text-foreground";
    let arrow = move |k: SortKey| -> Element {
        if sort.key != k {
            return rsx! {};
        }
        if sort.asc {
            rsx! { ArrowUp { class: "size-3" } }
        } else {
            rsx! { ArrowDown { class: "size-3" } }
        }
    };
    let mut sort_by = move |k: SortKey| {
        let s = fm.sort.peek().toggle(k);
        fm.sort.set(s);
        web::store_set(KEY_SORT, &s.encode());
    };

    if total == 0 {
        return rsx! { EmptyState { search: in_search } };
    }

    rsx! {
        if in_search {
            div { class: "mb-3 flex items-center gap-2 px-2 text-sm text-muted-foreground",
                Search { class: "size-4" }
                "{total} Treffer in /Home"
            }
        }
        div {
            match view {
                ViewMode::List => rsx! {
                    div { class: "flex flex-col gap-0.5",
                        div { class: "hidden items-center gap-3 border-b px-3 pb-2 text-xs font-medium text-muted-foreground md:flex",
                            div { class: "flex w-6 shrink-0 justify-center",
                                button {
                                    class: if all_selected { "inline-flex size-5 items-center justify-center rounded border border-primary bg-primary text-primary-foreground" } else { "inline-flex size-5 items-center justify-center rounded border border-muted-foreground/60" },
                                    title: "Alle auswählen",
                                    onclick: move |e: MouseEvent| {
                                        e.stop_propagation();
                                        if all_selected { fm.clear_selection() } else { fm.select_all() }
                                    },
                                    if all_selected { icons::Check { class: "size-3.5" } }
                                }
                            }
                            div { class: "w-9 shrink-0" }
                            button { class: "{header_btn} flex-1", onclick: move |e: MouseEvent| { e.stop_propagation(); sort_by(SortKey::Name) }, "Name" {arrow(SortKey::Name)} }
                            button { class: "{header_btn} w-24 shrink-0 justify-end", onclick: move |e: MouseEvent| { e.stop_propagation(); sort_by(SortKey::Size) }, "Grösse" {arrow(SortKey::Size)} }
                            button { class: "{header_btn} w-40 shrink-0", onclick: move |e: MouseEvent| { e.stop_propagation(); sort_by(SortKey::Modified) }, "Geändert" {arrow(SortKey::Modified)} }
                            button { class: "{header_btn} hidden w-28 shrink-0 lg:inline-flex", onclick: move |e: MouseEvent| { e.stop_propagation(); sort_by(SortKey::Type) }, "Typ" {arrow(SortKey::Type)} }
                            div { class: "w-9 shrink-0" }
                        }
                        for e in list {
                            Item { key: "{e.path}", entry: e, mode: ViewMode::List, show_path }
                        }
                    }
                },
                ViewMode::Grid => rsx! {
                    div { class: "grid grid-cols-3 gap-x-1 gap-y-2 sm:grid-cols-4 md:grid-cols-[repeat(auto-fill,minmax(9.5rem,1fr))] md:gap-x-4 md:gap-y-5",
                        for e in list {
                            Item { key: "{e.path}", entry: e, mode: ViewMode::Grid, show_path }
                        }
                    }
                },
            }
            if !in_search && *fm.has_more.read() {
                div {
                    class: "flex justify-center py-4",
                    onvisible: move |e: VisibleEvent| {
                        if e.is_intersecting().unwrap_or(false) {
                            fm.load_more();
                        }
                    },
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Outline,
                        disabled: *fm.loading_more.read(),
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            fm.load_more();
                        },
                        if *fm.loading_more.read() { LoaderCircle { class: "size-4 animate-spin" } }
                        "Mehr laden"
                    }
                }
            }
        }
    }
}

#[component]
fn EmptyState(search: bool) -> Element {
    let fm = use_context::<Fm>();
    let cur = fm.path.read().clone();
    let target = logic::is_target_dir(&cur);
    rsx! {
        div { class: "flex h-full min-h-[40dvh] flex-col items-center justify-center gap-3 p-8 text-center",
            if search {
                Search { class: "size-10 text-muted-foreground" }
                div { class: "text-sm font-medium", "Keine Treffer" }
                p { class: "text-sm text-muted-foreground", "Die Suche umfasst nur /Home." }
            } else if cur == RECENT {
                Clock { class: "size-10 text-muted-foreground" }
                div { class: "text-sm font-medium", "Noch nichts geöffnet" }
                p { class: "text-sm text-muted-foreground", "Hier erscheinen die zuletzt geöffneten Dateien." }
            } else {
                CloudUpload { class: "size-10 text-muted-foreground" }
                div { class: "text-sm font-medium", "Dieser Ordner ist leer" }
                if target {
                    p { class: "text-sm text-muted-foreground", "Dateien hierher ziehen oder hochladen." }
                    div { class: "flex flex-wrap justify-center gap-2",
                        Button { size: ButtonSize::Sm, onclick: move |_| web::click("fm-pick-files"), Upload { class: "size-4" } "Dateien hochladen" }
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, onclick: move |_| web::click("fm-pick-folder"), FolderUp { class: "size-4" } "Ordner hochladen" }
                    }
                }
            }
        }
    }
}

#[component]
fn ListSkeleton() -> Element {
    rsx! {
        div { class: "flex flex-col gap-1",
            for i in 0..8 {
                div { key: "{i}", class: "flex items-center gap-3 rounded-xl px-3 py-3",
                    Skeleton { class: "size-7 rounded-md" }
                    Skeleton { class: "h-4 flex-1" }
                    Skeleton { class: "hidden h-4 w-24 md:block" }
                }
            }
        }
    }
}

/// Floating upload button for phones with a small menu (files, folder, new folder).
#[component]
fn MobileFab() -> Element {
    let mut fm = use_context::<Fm>();
    let mut open = use_signal(|| false);
    let item =
        "flex items-center gap-3 rounded-full border bg-popover py-2 pl-3 pr-4 text-sm shadow-lg";
    rsx! {
        div { class: "absolute bottom-4 right-4 z-30 flex flex-col items-end gap-2 md:hidden",
            if open() {
                button { class: item, onclick: move |_| { open.set(false); fm.new_folder.set(true); }, FolderPlus { class: "size-4" } "Neuer Ordner" }
                button { class: item, onclick: move |_| { open.set(false); web::click("fm-pick-folder"); }, FolderUp { class: "size-4" } "Ordner hochladen" }
                button { class: item, onclick: move |_| { open.set(false); web::click("fm-pick-files"); }, Upload { class: "size-4" } "Dateien hochladen" }
            }
            button {
                class: "inline-flex size-14 items-center justify-center rounded-full bg-primary text-primary-foreground shadow-xl",
                "aria-label": "Hochladen",
                onclick: move |_| open.toggle(),
                if open() { X { class: "size-6" } } else { Plus { class: "size-6" } }
            }
        }
    }
}
