//! Context menu, preview, folder picker, new-folder dialog, selection bar, toasts and drop overlay.

use dioxus::prelude::*;
use icons::{
    ArrowUp, ChevronLeft, ChevronRight, ClipboardCopy, CloudUpload, Copy, Download, ExternalLink, FileCode, Folder, FolderInput,
    FolderOpen, FolderPlus, LoaderCircle, Pencil, Star, StarOff, Trash2, TriangleAlert, X,
};

use super::items::{kind_icon, root_icon, root_label};
use super::logic::{self, Kind};
use super::state::{Fm, PickOp, PickerState, RECENT, download_url, inline_url};
use super::web;
use crate::api::files::{list_dir, read_text};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::skeleton::Skeleton;
use crate::components::widgets::{ErrorBox, input_class};
use crate::utils::{err_msg, fmt_bytes, fmt_time_ms};

const MENU_ITEM: &str = "flex w-full items-center gap-3 rounded-md px-3 py-2.5 md:py-1.5 text-left text-sm hover:bg-accent";

#[component]
pub fn ContextMenu() -> Element {
    let mut fm = use_context::<Fm>();
    let Some(menu) = fm.menu.read().clone() else { return rsx! {} };
    let Some(entry) = fm.find(&menu.path) else { return rsx! {} };
    let sel = fm.selection();
    let multi = sel.len() > 1 && sel.contains(&entry.path);
    let targets = if multi { sel.clone() } else { vec![entry.path.clone()] };
    let searching = fm.results.read().is_some() || *fm.path.read() == RECENT;
    let fav = (!multi && entry.is_dir()).then(|| fm.is_fav(&entry.path));
    let (vw, vh) = web::viewport();
    let sheet = *fm.touch.read() || vw < 768.0;
    let est_h = if multi { 200.0 } else { 370.0 };
    let x = menu.x.min(vw - 236.0).max(8.0);
    let y = if menu.y + est_h > vh { (menu.y - est_h).max(8.0) } else { menu.y };
    let has_files = targets.iter().any(|p| fm.find(p).is_none_or(|e| !e.is_dir()));

    let mut close = move || fm.menu.set(None);
    let e_open = entry.clone();
    let (t_dl, t_mv, t_cp, t_tr) = (targets.clone(), targets.clone(), targets.clone(), targets.clone());
    let p_ren = entry.path.clone();
    let p_copy = entry.path.clone();
    let p_reveal = entry.path.clone();
    let p_tab = entry.path.clone();
    let p_edit = entry.path.clone();
    let p_fav = entry.path.clone();
    let title = if multi { format!("{} Elemente", targets.len()) } else { entry.name.clone() };

    rsx! {
        div {
            class: "fixed inset-0 z-40 bg-background/40 md:bg-transparent",
            onclick: move |_| fm.menu.set(None),
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                fm.menu.set(None);
            },
        }
        div {
            class: if sheet { "fixed inset-x-0 bottom-0 z-50 rounded-t-2xl border-t bg-popover p-2 pb-[calc(0.5rem+env(safe-area-inset-bottom,0px))] shadow-2xl" } else { "fixed z-50 w-56 rounded-lg border bg-popover p-1 shadow-xl" },
            style: if sheet { String::new() } else { format!("left:{x}px;top:{y}px") },
            onclick: move |e: MouseEvent| e.stop_propagation(),
            oncontextmenu: move |e: MouseEvent| e.prevent_default(),
            if sheet {
                div { class: "mx-auto mb-2 mt-1 h-1 w-10 rounded-full bg-muted" }
            }
            div { class: "truncate px-3 pb-1.5 pt-1 text-xs font-medium text-muted-foreground", "{title}" }
            if !multi {
                button { class: MENU_ITEM, onclick: move |_| { close(); fm.open(&e_open); },
                    FolderOpen { class: "size-4" } "Öffnen"
                }
            }
            if has_files {
                button { class: MENU_ITEM, onclick: move |_| { close(); fm.download(t_dl.clone()); },
                    Download { class: "size-4" } "Herunterladen"
                }
            }
            if !multi && logic::may_edit(logic::kind_of(&entry)) {
                button { class: MENU_ITEM, onclick: move |_| { close(); fm.editor.set(Some(p_edit.clone())); },
                    FileCode { class: "size-4" } "Im Editor öffnen"
                }
            }
            if !multi && !entry.is_dir() {
                button { class: MENU_ITEM, onclick: move |_| { close(); web::open_tab(&format!("{}&inline=1", download_url(&p_tab))); },
                    ExternalLink { class: "size-4" } "In neuem Tab öffnen"
                }
            }
            if !multi {
                button { class: MENU_ITEM, onclick: move |_| { close(); fm.renaming.set(Some(p_ren.clone())); },
                    Pencil { class: "size-4" } "Umbenennen"
                }
            }
            button { class: MENU_ITEM, onclick: move |_| { close(); fm.picker.set(Some(PickerState { op: PickOp::Move, paths: t_mv.clone() })); },
                FolderInput { class: "size-4" } "Verschieben nach …"
            }
            button { class: MENU_ITEM, onclick: move |_| { close(); fm.picker.set(Some(PickerState { op: PickOp::Copy, paths: t_cp.clone() })); },
                Copy { class: "size-4" } "Kopieren nach …"
            }
            if !multi {
                button { class: MENU_ITEM, onclick: move |_| { close(); fm.copy_path(&p_copy); },
                    ClipboardCopy { class: "size-4" } "Pfad kopieren"
                }
            }
            if !multi && searching {
                button { class: MENU_ITEM, onclick: move |_| { close(); fm.reveal_in_folder(&p_reveal); },
                    Folder { class: "size-4" } "Im Ordner zeigen"
                }
            }
            if let Some(is_fav) = fav {
                button { class: MENU_ITEM, onclick: move |_| { close(); fm.toggle_fav(&p_fav); },
                    if is_fav { StarOff { class: "size-4" } "Aus Favoriten entfernen" } else { Star { class: "size-4" } "Zu Favoriten" }
                }
            }
            div { class: "my-1 h-px bg-border" }
            button { class: "{MENU_ITEM} text-destructive", onclick: move |_| { close(); fm.trash(t_tr.clone()); },
                Trash2 { class: "size-4" } "In Papierkorb"
            }
        }
    }
}

#[component]
pub fn PreviewDialog() -> Element {
    let mut fm = use_context::<Fm>();
    let Some(path) = fm.preview.read().clone() else { return rsx! {} };
    let list = fm.preview_list();
    let Some(entry) = fm.find(&path) else { return rsx! {} };
    let idx = list.iter().position(|e| e.path == path).map(|i| i + 1).unwrap_or(0);
    let total = list.len();
    let kind = logic::kind_of(&entry);
    let src = inline_url(&entry);
    let dl = download_url(&path);
    let meta = [entry.size.map(fmt_bytes), entry.modified.map(fmt_time_ms)].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    let p_dl = path.clone();

    rsx! {
        div { class: "fixed inset-0 z-50 flex flex-col bg-background/95 backdrop-blur-sm",
            onclick: move |_| fm.preview.set(None),
            // header
            div { class: "flex items-center gap-2 border-b bg-card/80 px-3 py-2 pt-[calc(0.5rem+env(safe-area-inset-top,0px))]",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                {kind_icon(kind, "size-4 shrink-0 text-muted-foreground")}
                div { class: "min-w-0 flex-1",
                    div { class: "truncate text-sm font-medium", "{entry.name}" }
                    div { class: "truncate text-xs text-muted-foreground",
                        if total > 1 { "{idx} / {total} · " }
                        "{meta}"
                    }
                }
                Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, title: "Herunterladen", onclick: move |_| web::trigger_download(&download_url(&p_dl)),
                    Download { class: "size-4" }
                    span { class: "hidden sm:inline", "Herunterladen" }
                }
                Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "hidden sm:inline-flex", title: "In neuem Tab öffnen", onclick: move |_| web::open_tab(&format!("{dl}&inline=1")),
                    ExternalLink { class: "size-4" }
                }
                Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, title: "Schliessen (Esc)", onclick: move |_| fm.preview.set(None),
                    X { class: "size-4" }
                }
            }
            // content
            div { class: "relative flex min-h-0 flex-1 items-center justify-center p-3 md:p-8",
                div { class: "flex max-h-full max-w-full items-center justify-center", onclick: move |e: MouseEvent| e.stop_propagation(),
                    match kind {
                        Kind::Image => rsx! { img { key: "{src}", src: "{src}", alt: "{entry.name}", class: "max-h-[calc(100dvh-8rem)] max-w-full rounded-md object-contain shadow-lg" } },
                        Kind::Video => rsx! { video { key: "{src}", src: "{src}", controls: true, autoplay: true, playsinline: true, class: "max-h-[calc(100dvh-8rem)] max-w-full rounded-md bg-black" } },
                        Kind::Audio => rsx! {
                            div { class: "flex w-[min(28rem,90vw)] flex-col items-center gap-4 rounded-xl border bg-card p-6",
                                {kind_icon(Kind::Audio, "size-16 text-muted-foreground")}
                                audio { key: "{src}", src: "{src}", controls: true, autoplay: true, class: "w-full" }
                            }
                        },
                        Kind::Pdf => rsx! { iframe { key: "{src}", src: "{src}", title: "{entry.name}", class: "h-[calc(100dvh-8rem)] w-[min(64rem,95vw)] rounded-md border bg-card" } },
                        _ => rsx! { TextPreview { key: "{path}", path: path.clone() } },
                    }
                }
                if total > 1 {
                    button {
                        class: "absolute left-2 top-1/2 inline-flex size-11 -translate-y-1/2 items-center justify-center rounded-full border bg-card/90 shadow hover:bg-accent",
                        title: "Vorherige (←)",
                        onclick: move |e: MouseEvent| { e.stop_propagation(); fm.preview_step(-1); },
                        ChevronLeft { class: "size-5" }
                    }
                    button {
                        class: "absolute right-2 top-1/2 inline-flex size-11 -translate-y-1/2 items-center justify-center rounded-full border bg-card/90 shadow hover:bg-accent",
                        title: "Nächste (→)",
                        onclick: move |e: MouseEvent| { e.stop_propagation(); fm.preview_step(1); },
                        ChevronRight { class: "size-5" }
                    }
                }
            }
        }
    }
}

#[component]
fn TextPreview(path: String) -> Element {
    let text = use_resource(use_reactive!(|path| async move { read_text(path).await.map_err(|e| err_msg(&e)) }));
    rsx! {
        div { class: "h-[calc(100dvh-8rem)] w-[min(64rem,95vw)] overflow-auto rounded-md border bg-card",
            match &*text.read() {
                None => rsx! { div { class: "space-y-2 p-4", for i in 0..8 { Skeleton { key: "{i}", class: "h-4 w-full" } } } },
                Some(Err(e)) => rsx! { div { class: "p-4", ErrorBox { message: e.clone() } } },
                Some(Ok((t, truncated))) => rsx! {
                    pre { class: "whitespace-pre-wrap break-words p-4 font-mono text-xs leading-relaxed", "{t}" }
                    if *truncated {
                        div { class: "border-t px-4 py-2 text-xs text-muted-foreground", "Gekürzt – nur die ersten 512 KB werden angezeigt." }
                    }
                },
            }
        }
    }
}

/// Small dialog to choose a target folder for "Verschieben nach…" / "Kopieren nach…".
#[component]
pub fn FolderPicker() -> Element {
    let fm = use_context::<Fm>();
    let Some(state) = fm.picker.read().clone() else { return rsx! {} };
    rsx! { PickerInner { key: "{state.paths.join(\"|\")}", state } }
}

#[component]
fn PickerInner(state: PickerState) -> Element {
    let mut fm = use_context::<Fm>();
    let start = fm.path.peek().clone();
    let mut at = use_signal(move || if start == "/" { "/Home".to_string() } else { start });
    let listing = use_resource(move || async move {
        let p = at();
        let mut dirs = Vec::new();
        let mut last = None;
        // Folders only; walk a few pages so big folders still show their subfolders.
        for _ in 0..5 {
            let l = list_dir(p.clone(), last.clone()).await.map_err(|e| err_msg(&e))?;
            last = l.files.last().map(|f| f.name.clone());
            dirs.extend(l.files.into_iter().filter(|f| f.is_dir()));
            if !l.has_more {
                break;
            }
        }
        logic::sort_entries(&mut dirs, logic::Sort::default());
        Ok::<_, String>(dirs)
    });
    let roots = fm.roots.read().clone();
    let here = at();
    let valid = match state.op {
        PickOp::Move => logic::can_move_into(&state.paths, &here),
        PickOp::Copy => here != "/",
    };
    let verb = if state.op == PickOp::Move { "verschieben" } else { "kopieren" };
    let count = if state.paths.len() == 1 { format!("„{}“", logic::name_of(&state.paths[0])) } else { format!("{} Elemente", state.paths.len()) };
    let paths = state.paths.clone();
    let op = state.op;

    rsx! {
        div { class: "fixed inset-0 z-50 flex items-end justify-center bg-background/70 backdrop-blur-sm md:items-center md:p-6",
            onclick: move |_| fm.picker.set(None),
            div { class: "flex max-h-[85dvh] w-full flex-col overflow-hidden rounded-t-2xl border bg-card shadow-2xl md:max-w-lg md:rounded-xl",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                div { class: "flex items-center gap-2 border-b px-4 py-3",
                    div { class: "min-w-0 flex-1",
                        div { class: "text-sm font-semibold", "{count} {verb}" }
                        div { class: "truncate font-mono text-xs text-muted-foreground", "Ziel: {here}" }
                    }
                    button { class: "inline-flex size-8 items-center justify-center rounded-md hover:bg-accent", onclick: move |_| fm.picker.set(None), X { class: "size-4" } }
                }
                div { class: "flex gap-1 overflow-x-auto border-b px-3 py-2",
                    for r in roots {
                        button {
                            key: "{r.path}",
                            class: if logic::is_within(&here, &r.path) { "inline-flex shrink-0 items-center gap-1.5 rounded-full bg-accent px-3 py-1 text-xs" } else { "inline-flex shrink-0 items-center gap-1.5 rounded-full border px-3 py-1 text-xs text-muted-foreground hover:bg-accent" },
                            onclick: {
                                let p = r.path.clone();
                                move |_| at.set(p.clone())
                            },
                            {root_icon(&r.path, "size-3.5")}
                            "{root_label(&r.path)}"
                        }
                    }
                }
                div { class: "min-h-48 flex-1 overflow-y-auto p-2",
                    if here != "/" && logic::parent(&here) != "/" {
                        button { class: MENU_ITEM, onclick: move |_| at.set(logic::parent(&at())),
                            ArrowUp { class: "size-4 text-muted-foreground" }
                            span { class: "text-muted-foreground", "Übergeordneter Ordner" }
                        }
                    }
                    match &*listing.read() {
                        None => rsx! { div { class: "flex justify-center py-8", LoaderCircle { class: "size-5 animate-spin text-muted-foreground" } } },
                        Some(Err(e)) => rsx! { ErrorBox { message: e.clone() } },
                        Some(Ok(dirs)) if dirs.is_empty() => rsx! { p { class: "px-3 py-6 text-center text-sm text-muted-foreground", "Keine Unterordner" } },
                        Some(Ok(dirs)) => rsx! {
                            for d in dirs.clone() {
                                button {
                                    key: "{d.path}",
                                    class: MENU_ITEM,
                                    disabled: op == PickOp::Move && paths.iter().any(|p| logic::is_within(&d.path, p)),
                                    onclick: {
                                        let p = d.path.clone();
                                        move |_| at.set(p.clone())
                                    },
                                    Folder { class: "size-4 text-foreground/70" }
                                    span { class: "flex-1 truncate", "{d.name}" }
                                    ChevronRight { class: "size-4 text-muted-foreground" }
                                }
                            }
                        },
                    }
                }
                div { class: "flex items-center justify-end gap-2 border-t px-4 py-3 pb-[calc(0.75rem+env(safe-area-inset-bottom,0px))] md:pb-3",
                    Button { variant: ButtonVariant::Ghost, size: ButtonSize::Sm, onclick: move |_| fm.picker.set(None), "Abbrechen" }
                    Button {
                        size: ButtonSize::Sm,
                        disabled: !valid,
                        onclick: move |_| {
                            let dest = at();
                            fm.picker.set(None);
                            match op {
                                PickOp::Move => fm.move_to(state.paths.clone(), &dest),
                                PickOp::Copy => fm.batch("copy", state.paths.clone(), dest),
                            }
                        },
                        if op == PickOp::Move { "Hierher verschieben" } else { "Hierher kopieren" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn NewFolderDialog() -> Element {
    let mut fm = use_context::<Fm>();
    let mut name = use_signal(|| "Neuer Ordner".to_string());
    if !*fm.new_folder.read() {
        return rsx! {};
    }
    rsx! {
        div { class: "fixed inset-0 z-50 flex items-start justify-center bg-background/70 p-4 pt-24 backdrop-blur-sm",
            onclick: move |_| fm.new_folder.set(false),
            form {
                class: "w-full max-w-sm space-y-3 rounded-xl border bg-card p-4 shadow-2xl",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                onsubmit: move |e: FormEvent| {
                    e.prevent_default();
                    fm.mkdir(name());
                    name.set("Neuer Ordner".into());
                },
                div { class: "flex items-center gap-2 text-sm font-semibold", FolderPlus { class: "size-4" } "Neuer Ordner" }
                input {
                    class: input_class(),
                    value: "{name}",
                    onmounted: move |e| web::select_range(&e, name.peek().encode_utf16().count() as u32),
                    oninput: move |e| name.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        e.stop_propagation();
                        if e.key() == Key::Escape {
                            fm.new_folder.set(false);
                        }
                    },
                }
                div { class: "flex justify-end gap-2",
                    Button { variant: ButtonVariant::Ghost, size: ButtonSize::Sm, onclick: move |_| fm.new_folder.set(false), "Abbrechen" }
                    Button { button_type: "submit", size: ButtonSize::Sm, "Erstellen" }
                }
            }
        }
    }
}

/// Bulk actions for the selection; on phones above the bottom navigation.
#[component]
pub fn SelectionBar() -> Element {
    let mut fm = use_context::<Fm>();
    let count = fm.selected.read().len();
    let mode = *fm.select_mode.read();
    if count == 0 && !mode {
        return rsx! {};
    }
    let all = fm.visible().len();
    let single_dir = if count == 1 { fm.selection().pop().filter(|p| fm.find(p).is_some_and(|e| e.is_dir())) } else { None };
    let is_fav = single_dir.as_deref().is_some_and(|p| fm.is_fav(p));
    let btn = "flex min-w-14 flex-col items-center gap-0.5 rounded-lg px-2 py-1.5 text-[11px] hover:bg-accent disabled:opacity-40 md:flex-row md:gap-2 md:px-3 md:text-sm";
    let label = if count == 1 { "1 ausgewählt".to_string() } else { format!("{count} ausgewählt") };
    rsx! {
        div { class: "pointer-events-none fixed inset-x-0 bottom-[calc(4.25rem+env(safe-area-inset-bottom,0px))] z-30 flex justify-center px-2 md:bottom-6",
            div { class: "pointer-events-auto flex max-w-full items-center gap-1 overflow-x-auto rounded-2xl border bg-popover p-1.5 shadow-2xl",
                button {
                    class: "inline-flex size-10 shrink-0 items-center justify-center rounded-lg hover:bg-accent",
                    title: "Auswahl aufheben (Esc)",
                    onclick: move |_| {
                        fm.clear_selection();
                        fm.select_mode.set(false);
                    },
                    X { class: "size-4" }
                }
                div { class: "shrink-0 px-1 text-sm font-medium tabular-nums whitespace-nowrap", "{label}" }
                if count < all {
                    button { class: "shrink-0 rounded-lg px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground whitespace-nowrap", onclick: move |_| fm.select_all(), "Alle" }
                }
                div { class: "mx-1 h-8 w-px shrink-0 bg-border" }
                button { class: btn, disabled: count == 0, onclick: move |_| fm.download(fm.selection()), Download { class: "size-4" } "Laden" }
                button { class: btn, disabled: count == 0, onclick: move |_| fm.picker.set(Some(PickerState { op: PickOp::Move, paths: fm.selection() })), FolderInput { class: "size-4" } "Verschieben" }
                button { class: btn, disabled: count == 0, onclick: move |_| fm.picker.set(Some(PickerState { op: PickOp::Copy, paths: fm.selection() })), Copy { class: "size-4" } "Kopieren" }
                if let Some(dir) = single_dir.clone() {
                    button { class: btn, onclick: move |_| fm.toggle_fav(&dir),
                        if is_fav { StarOff { class: "size-4" } "Favorit" } else { Star { class: "size-4" } "Favorit" }
                    }
                }
                if count == 1 {
                    button { class: btn, onclick: move |_| {
                            let p = fm.selection().pop();
                            fm.select_mode.set(false);
                            fm.renaming.set(p);
                        }, Pencil { class: "size-4" } "Umbenennen" }
                }
                button { class: "{btn} text-destructive", disabled: count == 0, onclick: move |_| fm.trash(fm.selection()), Trash2 { class: "size-4" } "Papierkorb" }
            }
        }
    }
}

#[component]
pub fn Toasts() -> Element {
    let mut fm = use_context::<Fm>();
    let toasts = fm.toasts.read().clone();
    rsx! {
        div { class: "pointer-events-none fixed inset-x-3 top-16 z-[90] flex flex-col items-center gap-2 md:inset-x-auto md:right-6 md:top-6 md:w-96 md:items-stretch",
            for t in toasts {
                div {
                    key: "{t.id}",
                    class: if t.error { "pointer-events-auto flex w-full items-start gap-2 rounded-lg border border-destructive/40 bg-card p-3 text-sm text-destructive shadow-lg" } else { "pointer-events-auto flex w-full items-start gap-2 rounded-lg border bg-card p-3 text-sm shadow-lg" },
                    if t.error { TriangleAlert { class: "mt-0.5 size-4 shrink-0" } }
                    span { class: "min-w-0 flex-1 break-words", "{t.text}" }
                    button {
                        class: "shrink-0 text-muted-foreground hover:text-foreground",
                        onclick: move |_| fm.toasts.write().retain(|x| x.id != t.id),
                        X { class: "size-4" }
                    }
                }
            }
        }
    }
}

/// Visible while files from the operating system are dragged over the page.
#[component]
pub fn DropOverlay() -> Element {
    let fm = use_context::<Fm>();
    if *fm.ext_drag.read() <= 0 || fm.drag.read().is_some() {
        return rsx! {};
    }
    let path = fm.path.read().clone();
    let target = fm.drop_target.read().clone().unwrap_or_else(|| path.clone());
    let can = target != "/";
    rsx! {
        div { class: "pointer-events-none fixed inset-0 z-40 flex items-end justify-center bg-primary/5 p-6 pb-24 md:items-center md:pb-6",
            div { class: "absolute inset-3 rounded-2xl border-2 border-dashed border-primary/70" }
            div { class: "relative flex items-center gap-3 rounded-xl border bg-popover px-5 py-4 shadow-2xl",
                CloudUpload { class: "size-6 text-primary" }
                if can {
                    div {
                        div { class: "text-sm font-semibold", "Loslassen zum Hochladen" }
                        div { class: "font-mono text-xs text-muted-foreground", "nach {target}" }
                    }
                } else {
                    div { class: "text-sm", "Bitte zuerst einen Ordner öffnen (z. B. Home)" }
                }
            }
        }
    }
}
