//! Modular dashboard like Homarr: several boards, edit mode, drag & drop, resize, widget catalogue.

pub(crate) mod appmenu;
mod integrations;
mod layout;
mod settings;
pub(crate) mod widgets;

use std::rc::Rc;

use dioxus::prelude::*;
use icons::{Check, GripVertical, LayoutGrid, MoveDiagonal2, Pencil, Plus, Settings, Trash2, X};
use serde_json::json;

use crate::api::board::{Board, BoardMeta, COLS, Widget, WidgetKind, board_create, board_delete, board_get, board_save, boards_list};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::widgets::{ConfirmButton, ErrorBox, input_class};
use crate::utils::err_msg;

use layout::Rect;
use settings::WidgetSettings;
use widgets::WidgetView;

const ROW_H: f64 = 88.0;
const GAP: f64 = 16.0;
const STORAGE_KEY: &str = "nrzdashboard.board";

#[derive(Clone, Copy, PartialEq)]
enum DragMode {
    Move,
    Resize,
}

#[derive(Clone, PartialEq)]
struct Drag {
    id: String,
    mode: DragMode,
    start: (f64, f64),
    orig: Rect,
}

fn saved_board_id() -> Option<String> {
    #[cfg(feature = "web")]
    {
        web_sys::window()?.local_storage().ok()??.get_item(STORAGE_KEY).ok()?
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = STORAGE_KEY;
        None
    }
}

fn remember_board_id(id: &str) {
    #[cfg(feature = "web")]
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(STORAGE_KEY, id);
    }
    #[cfg(not(feature = "web"))]
    let _ = id;
}

fn new_widget_id() -> String {
    #[cfg(feature = "web")]
    {
        format!("w{:x}{:x}", js_sys::Date::now() as u64, (js_sys::Math::random() * 1e9) as u64)
    }
    #[cfg(not(feature = "web"))]
    {
        "w0".to_string()
    }
}

fn default_options(kind: WidgetKind) -> serde_json::Value {
    match kind {
        WidgetKind::Clock => json!({ "seconds": false, "date": true }),
        WidgetKind::Search => json!({ "engine": "duckduckgo" }),
        WidgetKind::Weather => json!({ "city": "Zürich", "lat": 47.3769, "lon": 8.5417 }),
        WidgetKind::Apps => json!({ "stopped": true }),
        WidgetKind::News => json!({ "headlines": 5, "metrics": true }),
        _ => json!({}),
    }
}

#[component]
pub fn BoardPage() -> Element {
    let mut boards = use_signal(Vec::<BoardMeta>::new);
    let mut board = use_signal(|| None::<Board>);
    let mut error = use_signal(|| None::<String>);
    let mut edit = use_signal(|| false);
    let mut drag = use_signal(|| None::<Drag>);
    let mut preview = use_signal(|| None::<Vec<Widget>>);
    let mut grid_el = use_signal(|| None::<Rc<MountedData>>);
    let mut grid_w = use_signal(|| 1200.0_f64);
    let mut adding = use_signal(|| false);
    let mut editing = use_signal(|| None::<String>);
    let mut board_menu = use_signal(|| false);
    let menu = use_signal(|| None::<appmenu::AppMenuReq>);
    let refresh = use_signal(|| 0u32);
    use_context_provider(|| appmenu::AppMenuCtx { menu, refresh });

    // Load board list + the last opened board.
    use_future(move || async move {
        match boards_list().await {
            Ok(list) => {
                let want = saved_board_id().filter(|id| list.iter().any(|b| &b.id == id));
                let id = want.or_else(|| list.first().map(|b| b.id.clone())).unwrap_or_default();
                boards.set(list);
                match board_get(id).await {
                    Ok(b) => board.set(Some(b)),
                    Err(e) => error.set(Some(err_msg(&e))),
                }
            }
            Err(e) => error.set(Some(err_msg(&e))),
        }
    });

    let measure = move || {
        spawn(async move {
            let el = grid_el.peek().clone();
            if let Some(el) = el {
                if let Ok(r) = el.get_client_rect().await {
                    if r.size.width > 0.0 {
                        grid_w.set(r.size.width);
                    }
                }
            }
        });
    };

    let mut save = move |b: Board| {
        board.set(Some(b.clone()));
        spawn(async move {
            if let Err(e) = board_save(b).await {
                error.set(Some(err_msg(&e)));
            }
        });
    };

    let mut open_board = move |id: String| {
        remember_board_id(&id);
        board_menu.set(false);
        spawn(async move {
            match board_get(id).await {
                Ok(b) => board.set(Some(b)),
                Err(e) => error.set(Some(err_msg(&e))),
            }
        });
    };

    // Pointer handling while dragging (full-screen overlay captures move/up).
    let on_move = move |e: PointerEvent| {
        let d = drag.peek().clone();
        let cur = board.peek().clone();
        let (Some(d), Some(b)) = (d, cur) else { return };
        let p = e.client_coordinates();
        let col = (grid_w() + GAP) / f64::from(COLS);
        let row = ROW_H + GAP;
        let dx = ((p.x - d.start.0) / col).round() as i32;
        let dy = ((p.y - d.start.1) / row).round() as i32;
        let target = match d.mode {
            DragMode::Move => Rect { x: d.orig.x + dx, y: d.orig.y + dy, ..d.orig },
            DragMode::Resize => Rect { w: d.orig.w + dx, h: d.orig.h + dy, ..d.orig },
        };
        let next = layout::place(&b.widgets, &d.id, target);
        if preview.peek().as_ref() != Some(&next) {
            preview.set(Some(next));
        }
    };
    let on_up = move |_| {
        let p = preview.take();
        let cur = board.peek().clone();
        if let (Some(p), Some(mut b)) = (p, cur) {
            b.widgets = p;
            save(b);
        }
        drag.set(None);
    };

    let mut start_drag = move |id: String, mode: DragMode, e: PointerEvent| {
        e.prevent_default();
        let cur = board.peek().clone();
        let Some(b) = cur else { return };
        let Some(w) = b.widgets.iter().find(|w| w.id == id) else { return };
        let p = e.client_coordinates();
        measure();
        drag.set(Some(Drag { id, mode, start: (p.x, p.y), orig: Rect::of(w) }));
    };

    let Some(current) = board() else {
        return rsx! {
            if let Some(e) = error() {
                ErrorBox { message: e }
            } else {
                div { class: "h-40 animate-pulse rounded-xl bg-muted/40" }
            }
        };
    };

    let shown: Vec<Widget> = preview().unwrap_or_else(|| current.widgets.clone());
    let mut ordered = shown.clone();
    ordered.sort_by_key(|w| (w.y, w.x));
    let rows = layout::rows(&shown).max(if edit() { 6 } else { 1 }) + if edit() { 2 } else { 0 };
    let dragging_id = drag().map(|d| d.id);
    let edit_widget = editing().and_then(|id| current.widgets.iter().find(|w| w.id == id).cloned());
    let board_name = current.name.clone();

    rsx! {
        // Header: board switcher + edit controls
        div { class: "mb-5 flex flex-wrap items-center justify-between gap-3",
            div { class: "relative",
                button {
                    class: "flex items-center gap-2 rounded-md px-2 py-1 -ml-2 text-2xl font-semibold tracking-tight hover:bg-accent/60",
                    onclick: move |_| board_menu.toggle(),
                    LayoutGrid { class: "size-5 text-muted-foreground" }
                    "{board_name}"
                    icons::ChevronDown { class: "size-4 text-muted-foreground" }
                }
                if board_menu() {
                    BoardMenu {
                        boards: boards(),
                        current: current.id.clone(),
                        onselect: move |id| open_board(id),
                        oncreate: move |name: String| {
                            spawn(async move {
                                match board_create(name).await {
                                    Ok(b) => {
                                        if let Ok(list) = boards_list().await {
                                            boards.set(list);
                                        }
                                        remember_board_id(&b.id);
                                        board.set(Some(b));
                                        board_menu.set(false);
                                        edit.set(true);
                                    }
                                    Err(e) => error.set(Some(err_msg(&e))),
                                }
                            });
                        },
                        onclose: move |_| board_menu.set(false),
                    }
                }
            }
            div { class: "flex items-center gap-2",
                if edit() {
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, onclick: move |_| adding.set(true),
                        Plus { class: "size-4" }
                        "Widget"
                    }
                    BoardSettingsButton {
                        board: current.clone(),
                        can_delete: boards().len() > 1,
                        onrename: move |name: String| {
                            let cur = board.peek().clone();
                            if let Some(mut b) = cur {
                                b.name = name;
                                save(b);
                                spawn(async move {
                                    if let Ok(list) = boards_list().await {
                                        boards.set(list);
                                    }
                                });
                            }
                        },
                        ondelete: move |id: String| {
                            spawn(async move {
                                match board_delete(id).await {
                                    Ok(()) => {
                                        let list = boards_list().await.unwrap_or_default();
                                        let first = list.first().map(|b| b.id.clone()).unwrap_or_default();
                                        boards.set(list);
                                        edit.set(false);
                                        open_board(first);
                                    }
                                    Err(e) => error.set(Some(err_msg(&e))),
                                }
                            });
                        },
                    }
                    Button { size: ButtonSize::Sm, onclick: move |_| edit.set(false),
                        Check { class: "size-4" }
                        "Fertig"
                    }
                } else {
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| {
                            edit.set(true);
                            measure();
                        },
                        Pencil { class: "size-4" }
                        span { class: "hidden sm:inline", "Bearbeiten" }
                    }
                }
            }
        }

        if let Some(e) = error() {
            div { class: "mb-4 flex items-start gap-2",
                div { class: "flex-1", ErrorBox { message: e } }
                button { class: "p-2 text-muted-foreground", onclick: move |_| error.set(None), X { class: "size-4" } }
            }
        }

        if edit() {
            p { class: "mb-3 text-xs text-muted-foreground hidden md:block",
                "Widgets am Griff ziehen zum Verschieben, an der Ecke unten rechts ziehen für die Grösse."
            }
        }

        if shown.is_empty() {
            div { class: "flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed py-16 text-center",
                LayoutGrid { class: "size-8 text-muted-foreground" }
                p { class: "text-sm text-muted-foreground", "Dieses Board ist leer." }
                Button {
                    size: ButtonSize::Sm,
                    onclick: move |_| {
                        edit.set(true);
                        adding.set(true);
                    },
                    Plus { class: "size-4" }
                    "Widget hinzufügen"
                }
            }
        }

        // The grid. Desktop: 12 columns with explicit placement; mobile: one column in reading order.
        div {
            class: if edit() { "board-grid board-edit" } else { "board-grid" },
            style: "--rows: {rows}",
            onmounted: move |e| {
                grid_el.set(Some(e.data()));
                measure();
            },
            for w in ordered {
                div {
                    key: "{w.id}",
                    class: if dragging_id.as_deref() == Some(w.id.as_str()) { "board-item relative z-10 ring-2 ring-primary/60 rounded-xl" } else { "board-item relative" },
                    style: "--x: {w.x}; --y: {w.y}; --w: {w.w}; --h: {w.h};",
                    WidgetView {
                        widget: w.clone(),
                        edit: edit(),
                        onchange: move |nw: Widget| {
                            let cur = board.peek().clone();
                            if let Some(mut b) = cur {
                                if let Some(slot) = b.widgets.iter_mut().find(|x| x.id == nw.id) {
                                    *slot = nw;
                                }
                                save(b);
                            }
                        },
                    }
                    if edit() {
                        // Edit chrome: drag handle, settings, delete, resize corner
                        div { class: "absolute inset-0 rounded-xl border-2 border-dashed border-primary/30 pointer-events-none" }
                        div { class: "absolute left-2 right-2 top-2 flex items-center justify-between gap-1",
                            div {
                                class: "hidden md:flex size-8 cursor-grab items-center justify-center rounded-md bg-background/90 border text-muted-foreground touch-none select-none active:cursor-grabbing",
                                title: "Verschieben",
                                onpointerdown: {
                                    let id = w.id.clone();
                                    move |e| start_drag(id.clone(), DragMode::Move, e)
                                },
                                GripVertical { class: "size-4" }
                            }
                            div { class: "flex-1" }
                            button {
                                class: "flex size-8 items-center justify-center rounded-md bg-background/90 border text-muted-foreground hover:text-foreground",
                                title: "Einstellungen",
                                onclick: {
                                    let id = w.id.clone();
                                    move |_| editing.set(Some(id.clone()))
                                },
                                Settings { class: "size-4" }
                            }
                            button {
                                class: "flex size-8 items-center justify-center rounded-md bg-background/90 border text-muted-foreground hover:text-destructive",
                                title: "Entfernen",
                                onclick: {
                                    let id = w.id.clone();
                                    move |_| {
                                        let cur = board.peek().clone();
                                        if let Some(mut b) = cur {
                                            b.widgets.retain(|x| x.id != id);
                                            layout::compact(&mut b.widgets, None);
                                            save(b);
                                        }
                                    }
                                },
                                Trash2 { class: "size-4" }
                            }
                        }
                        div {
                            class: "absolute bottom-1 right-1 hidden md:flex size-7 cursor-se-resize items-center justify-center rounded-md bg-background/90 border text-muted-foreground touch-none select-none",
                            title: "Grösse ändern",
                            onpointerdown: {
                                let id = w.id.clone();
                                move |e| start_drag(id.clone(), DragMode::Resize, e)
                            },
                            MoveDiagonal2 { class: "size-3.5" }
                        }
                    }
                }
            }
        }

        appmenu::AppMenuHost {}

        if drag().is_some() {
            div {
                class: "fixed inset-0 z-[60] cursor-grabbing touch-none select-none",
                onpointermove: on_move,
                onpointerup: on_up,
                onpointercancel: on_up,
            }
        }

        if adding() {
            AddWidgetDialog {
                onclose: move |_| adding.set(false),
                onadd: move |kind: WidgetKind| {
                    adding.set(false);
                    let cur = board.peek().clone();
                    if let Some(mut b) = cur {
                        let (w, h) = kind.default_size();
                        let (x, y) = layout::free_spot(&b.widgets, w, h);
                        let id = new_widget_id();
                        b.widgets.push(Widget { id: id.clone(), kind, x, y, w, h, title: String::new(), options: default_options(kind) });
                        save(b);
                        if matches!(kind, WidgetKind::App | WidgetKind::Iframe | WidgetKind::Weather | WidgetKind::Bookmarks | WidgetKind::Notes) {
                            editing.set(Some(id));
                        }
                    }
                },
            }
        }

        if let Some(w) = edit_widget {
            WidgetSettings {
                widget: w,
                onclose: move |_| editing.set(None),
                onsave: move |nw: Widget| {
                    editing.set(None);
                    let cur = board.peek().clone();
                    if let Some(mut b) = cur {
                        if let Some(slot) = b.widgets.iter_mut().find(|x| x.id == nw.id) {
                            *slot = nw;
                        }
                        save(b);
                    }
                },
            }
        }
    }
}

#[component]
fn BoardMenu(
    boards: Vec<BoardMeta>,
    current: String,
    onselect: EventHandler<String>,
    oncreate: EventHandler<String>,
    onclose: EventHandler<()>,
) -> Element {
    let mut name = use_signal(String::new);
    rsx! {
        div { class: "fixed inset-0 z-40", onclick: move |_| onclose.call(()) }
        div { class: "absolute left-0 top-full z-50 mt-2 w-72 rounded-xl border bg-popover p-2 shadow-lg",
            p { class: "px-2 pb-1 text-xs text-muted-foreground", "Boards" }
            for b in boards {
                button {
                    key: "{b.id}",
                    class: if b.id == current { "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-sm bg-accent" } else { "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-sm hover:bg-accent/60" },
                    onclick: {
                        let id = b.id.clone();
                        move |_| onselect.call(id.clone())
                    },
                    LayoutGrid { class: "size-4 text-muted-foreground" }
                    span { class: "truncate", "{b.name}" }
                }
            }
            form {
                class: "mt-2 flex gap-2 border-t pt-2",
                onsubmit: move |e: FormEvent| {
                    e.prevent_default();
                    let n = name();
                    if !n.trim().is_empty() {
                        name.set(String::new());
                        oncreate.call(n);
                    }
                },
                input { class: "{input_class()} h-8", placeholder: "Neues Board", value: "{name}", oninput: move |e| name.set(e.value()) }
                Button { button_type: "submit", size: ButtonSize::Sm, class: "h-8", Plus { class: "size-4" } }
            }
        }
    }
}

#[component]
fn BoardSettingsButton(board: Board, can_delete: bool, onrename: EventHandler<String>, ondelete: EventHandler<String>) -> Element {
    let mut open = use_signal(|| false);
    let mut name = use_signal(|| board.name.clone());
    let id = board.id.clone();
    rsx! {
        div { class: "relative",
            Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, onclick: move |_| open.toggle(), Settings { class: "size-4" } }
            if open() {
                div { class: "fixed inset-0 z-40", onclick: move |_| open.set(false) }
                div { class: "absolute right-0 top-full z-50 mt-2 w-72 rounded-xl border bg-popover p-3 shadow-lg flex flex-col gap-3",
                    form {
                        class: "flex flex-col gap-1.5 text-sm",
                        onsubmit: move |e: FormEvent| {
                            e.prevent_default();
                            onrename.call(name());
                            open.set(false);
                        },
                        span { class: "text-muted-foreground", "Name des Boards" }
                        div { class: "flex gap-2",
                            input { class: "{input_class()} h-8", value: "{name}", oninput: move |e| name.set(e.value()) }
                            Button { button_type: "submit", size: ButtonSize::Sm, class: "h-8", "OK" }
                        }
                    }
                    if can_delete {
                        ConfirmButton {
                            label: "Board löschen",
                            confirm_label: "Wirklich löschen?",
                            class: "w-full",
                            onconfirm: move |_| {
                                open.set(false);
                                ondelete.call(id.clone());
                            },
                            Trash2 { class: "size-4" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AddWidgetDialog(onclose: EventHandler<()>, onadd: EventHandler<WidgetKind>) -> Element {
    rsx! {
        div { class: "fixed inset-0 z-[70] flex items-end sm:items-center justify-center bg-black/60 p-0 sm:p-4", onclick: move |_| onclose.call(()),
            div {
                class: "w-full sm:max-w-2xl max-h-[85dvh] overflow-y-auto rounded-t-2xl sm:rounded-2xl border bg-card p-4 sm:p-6 pb-[calc(env(safe-area-inset-bottom,0px)+1rem)]",
                onclick: move |e| e.stop_propagation(),
                div { class: "mb-4 flex items-center justify-between",
                    h2 { class: "text-lg font-semibold", "Widget hinzufügen" }
                    button { class: "p-2 text-muted-foreground hover:text-foreground", onclick: move |_| onclose.call(()), X { class: "size-4" } }
                }
                div { class: "grid grid-cols-1 sm:grid-cols-2 gap-2",
                    for kind in WidgetKind::ALL {
                        button {
                            key: "{kind.label()}",
                            class: "flex items-start gap-3 rounded-xl border p-3 text-left hover:bg-accent/50 transition-colors",
                            onclick: move |_| onadd.call(kind),
                            div { class: "flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted text-foreground [&_svg]:size-4",
                                {widgets::kind_icon(kind)}
                            }
                            div { class: "min-w-0",
                                div { class: "text-sm font-medium", "{kind.label()}" }
                                div { class: "text-xs text-muted-foreground", "{kind.description()}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
