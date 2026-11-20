//! Uploads: queue with progress per file. The browser sends each `File` directly as the request body
//! (`XMLHttpRequest`, for progress events) to `POST /files/upload-raw?path=…`, which streams it to umbrelOS.
//! Nothing is read into WASM memory.

use dioxus::prelude::*;
use icons::{ChevronDown, ChevronUp, CircleCheck, CircleX, CloudUpload, LoaderCircle, X};

use super::logic;
use super::state::Fm;
use crate::components::ui::progress::Progress;
use crate::utils::fmt_bytes;
#[cfg(feature = "web")]
use dioxus::html::HasFileData;

#[cfg_attr(not(feature = "web"), allow(dead_code))]
#[derive(Clone, PartialEq, Debug)]
pub enum UpState {
    Queued,
    Running,
    Done,
    Failed(String),
    Cancelled,
}

#[derive(Clone, PartialEq, Debug)]
pub struct UploadItem {
    pub id: u64,
    pub name: String,
    pub dir: String,
    pub size: f64,
    pub loaded: f64,
    pub state: UpState,
}

impl UploadItem {
    pub fn active(&self) -> bool {
        matches!(self.state, UpState::Queued | UpState::Running)
    }
}

/// Files dropped from the operating system onto a folder.
pub fn from_drop(fm: Fm, e: &DragEvent, dir: String) {
    #[cfg(feature = "web")]
    {
        let mut files = Vec::new();
        let mut folders = 0;
        for f in e.files() {
            let Some(file) = f.inner().downcast_ref::<web_sys::File>().cloned() else { continue };
            // Dropped folders arrive as empty, typeless entries without extension.
            if file.size() == 0.0 && file.type_().is_empty() && !file.name().contains('.') {
                folders += 1;
                continue;
            }
            let target = logic::join(&dir, &file.name());
            files.push((file, target));
        }
        if folders > 0 {
            fm.toast_err("Ordner bitte über „Hochladen → Ordner“ auswählen – per Ziehen gehen nur Dateien");
        }
        imp::enqueue(fm, files);
    }
    #[cfg(not(feature = "web"))]
    let _ = (fm, e, dir);
}

/// Files picked in a hidden `<input type=file>` (`folder` = with `webkitdirectory`).
pub fn from_input(fm: Fm, e: &FormEvent, dir: String, folder: bool) {
    #[cfg(feature = "web")]
    {
        use wasm_bindgen::JsCast;
        let mut picked: Vec<(web_sys::File, String)> = Vec::new();
        for f in e.files() {
            let Some(file) = f.inner().downcast_ref::<web_sys::File>().cloned() else { continue };
            let rel = if folder {
                js_sys::Reflect::get(&file, &"webkitRelativePath".into())
                    .ok()
                    .and_then(|v| v.as_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| file.name())
            } else {
                file.name()
            };
            picked.push((file, rel));
        }
        // Reset so picking the same file again fires `change`.
        if let Some(input) = e
            .downcast::<web_sys::Event>()
            .and_then(|ev| ev.target())
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        {
            input.set_value("");
        }
        if picked.is_empty() {
            return;
        }
        let rels: Vec<String> = picked.iter().map(|(_, r)| r.clone()).collect();
        let dirs = logic::rel_dirs(&rels);
        let files: Vec<(web_sys::File, String)> = picked.into_iter().map(|(f, r)| (f, logic::join(&dir, &r))).collect();
        if dirs.is_empty() {
            imp::enqueue(fm, files);
        } else {
            fm.spawn(async move {
                match crate::api::files::make_dirs(dir, dirs).await {
                    Ok(()) => imp::enqueue(fm, files),
                    Err(e) => fm.toast_err(crate::utils::err_msg(&e)),
                }
            });
        }
    }
    #[cfg(not(feature = "web"))]
    let _ = (fm, e, dir, folder);
}

pub fn cancel(fm: Fm, id: u64) {
    #[cfg(feature = "web")]
    imp::cancel(fm, id);
    #[cfg(not(feature = "web"))]
    let _ = (fm, id);
}

/// Stops all transfers (page left).
pub fn abort_all() {
    #[cfg(feature = "web")]
    imp::abort_all();
}

#[cfg(feature = "web")]
mod imp {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    use dioxus::core::{Runtime, RuntimeGuard};
    use dioxus::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    use web_sys::{ProgressEvent, XmlHttpRequest};

    use super::{UpState, UploadItem};
    use crate::pages::files::logic;
    use crate::pages::files::state::Fm;
    use crate::utils::enc;

    const PARALLEL: usize = 2;

    struct Active {
        xhr: XmlHttpRequest,
        _progress: Closure<dyn FnMut(ProgressEvent)>,
        _done: Closure<dyn FnMut(ProgressEvent)>,
    }

    thread_local! {
        static PENDING: RefCell<HashMap<u64, (web_sys::File, String)>> = RefCell::new(HashMap::new());
        static ACTIVE: RefCell<HashMap<u64, Active>> = RefCell::new(HashMap::new());
        static NEXT: Cell<u64> = const { Cell::new(1) };
    }

    fn update(fm: Fm, id: u64, f: impl FnOnce(&mut UploadItem)) {
        let mut uploads = fm.uploads;
        if let Ok(mut list) = uploads.try_write()
            && let Some(item) = list.iter_mut().find(|i| i.id == id)
        {
            f(item);
        }
    }

    pub fn enqueue(fm: Fm, files: Vec<(web_sys::File, String)>) {
        if files.is_empty() {
            return;
        }
        let mut uploads = fm.uploads;
        for (file, target) in files {
            let id = NEXT.with(|n| {
                let v = n.get();
                n.set(v + 1);
                v
            });
            uploads.write().push(UploadItem {
                id,
                name: logic::name_of(&target),
                dir: logic::parent(&target),
                size: file.size(),
                loaded: 0.0,
                state: UpState::Queued,
            });
            PENDING.with(|p| p.borrow_mut().insert(id, (file, target)));
        }
        pump(fm);
    }

    fn pump(fm: Fm) {
        let Ok(list) = fm.uploads.try_peek() else { return };
        let running = list.iter().filter(|i| i.state == UpState::Running).count();
        let next: Vec<u64> =
            list.iter().filter(|i| i.state == UpState::Queued).map(|i| i.id).take(PARALLEL.saturating_sub(running)).collect();
        drop(list);
        for id in next {
            start(fm, id);
        }
    }

    fn start(fm: Fm, id: u64) {
        let Some((file, target)) = PENDING.with(|p| p.borrow_mut().remove(&id)) else {
            update(fm, id, |i| i.state = UpState::Failed("Datei nicht mehr verfügbar".into()));
            return;
        };
        update(fm, id, |i| i.state = UpState::Running);
        if send(fm, id, &file, &target).is_err() {
            update(fm, id, |i| i.state = UpState::Failed("Upload konnte nicht gestartet werden".into()));
        }
    }

    fn send(fm: Fm, id: u64, file: &web_sys::File, target: &str) -> Result<(), wasm_bindgen::JsValue> {
        let rt = Runtime::current();
        let scope = fm.scope();
        let xhr = XmlHttpRequest::new()?;
        xhr.open_with_async("POST", &format!("/files/upload-raw?path={}", enc(target)), true)?;
        xhr.set_request_header("Content-Type", "application/octet-stream")?;

        let rt_p = rt.clone();
        let progress = Closure::<dyn FnMut(ProgressEvent)>::new(move |e: ProgressEvent| {
            let _guard = RuntimeGuard::new(rt_p.clone());
            rt_p.in_scope(scope, || update(fm, id, |i| i.loaded = e.loaded()));
        });

        let xhr_done = xhr.clone();
        let rt_d = rt.clone();
        let done = Closure::<dyn FnMut(ProgressEvent)>::new(move |_e: ProgressEvent| {
            let status = xhr_done.status().unwrap_or(0);
            let text = xhr_done.response_text().ok().flatten().unwrap_or_default();
            let _guard = RuntimeGuard::new(rt_d.clone());
            rt_d.in_scope(scope, || {
                update(fm, id, |i| {
                    if i.state == UpState::Cancelled {
                        return;
                    }
                    i.state = match status {
                        200..=299 => {
                            i.loaded = i.size;
                            UpState::Done
                        }
                        0 => UpState::Failed("Verbindung abgebrochen".into()),
                        401 => UpState::Failed("Sitzung abgelaufen – bitte neu anmelden".into()),
                        413 => UpState::Failed("Datei zu gross für den Server/Proxy".into()),
                        _ if !text.trim().is_empty() => UpState::Failed(text.trim().chars().take(300).collect()),
                        s => UpState::Failed(format!("Fehler HTTP {s}")),
                    };
                });
                pump(fm);
                fm.reload();
            });
            // Drop the closures after this callback has returned.
            wasm_bindgen_futures::spawn_local(async move {
                ACTIVE.with(|a| a.borrow_mut().remove(&id));
            });
        });

        xhr.upload()?.set_onprogress(Some(progress.as_ref().unchecked_ref()));
        xhr.set_onloadend(Some(done.as_ref().unchecked_ref()));
        xhr.send_with_opt_blob(Some(file))?;
        ACTIVE.with(|a| a.borrow_mut().insert(id, Active { xhr, _progress: progress, _done: done }));
        Ok(())
    }

    pub fn cancel(fm: Fm, id: u64) {
        PENDING.with(|p| p.borrow_mut().remove(&id));
        update(fm, id, |i| {
            if i.active() {
                i.state = UpState::Cancelled;
            }
        });
        let xhr = ACTIVE.with(|a| a.borrow().get(&id).map(|x| x.xhr.clone()));
        if let Some(xhr) = xhr {
            let _ = xhr.abort();
        }
        pump(fm);
    }

    pub fn abort_all() {
        PENDING.with(|p| p.borrow_mut().clear());
        let active: Vec<Active> = ACTIVE.with(|a| a.borrow_mut().drain().map(|(_, v)| v).collect());
        for a in &active {
            a.xhr.set_onloadend(None);
            if let Ok(up) = a.xhr.upload() {
                up.set_onprogress(None);
            }
            let _ = a.xhr.abort();
        }
    }
}

/// Upload list with progress (fixed bottom right on desktop, in the page flow on phones).
#[component]
pub fn UploadPanel() -> Element {
    let fm = use_context::<Fm>();
    let mut uploads = fm.uploads;
    let mut collapsed = use_signal(|| false);
    let list = uploads.read().clone();
    if list.is_empty() {
        return rsx! {};
    }
    let active = list.iter().filter(|i| i.active()).count();
    let done = list.iter().filter(|i| i.state == UpState::Done).count();
    let failed = list.iter().filter(|i| matches!(i.state, UpState::Failed(_))).count();
    let total: f64 = list.iter().filter(|i| i.active()).map(|i| i.size).sum();
    let loaded: f64 = list.iter().filter(|i| i.active()).map(|i| i.loaded).sum();
    let title = if active > 0 {
        format!("Lade hoch: {active} offen · {}", if total > 0.0 { format!("{:.0} %", loaded / total * 100.0) } else { "0 %".into() })
    } else if failed > 0 {
        format!("{done} hochgeladen, {failed} fehlgeschlagen")
    } else {
        format!("{done} hochgeladen")
    };

    rsx! {
        div { class: "mb-3 md:mb-0 md:fixed md:bottom-6 md:right-6 md:z-30 md:w-96 overflow-hidden rounded-xl border bg-card shadow-lg",
            div { class: "flex items-center gap-2 border-b px-3 py-2",
                if active > 0 {
                    LoaderCircle { class: "size-4 animate-spin text-muted-foreground" }
                } else {
                    CloudUpload { class: "size-4 text-muted-foreground" }
                }
                span { class: "flex-1 truncate text-sm font-medium", "{title}" }
                button {
                    class: "inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent",
                    title: if collapsed() { "Aufklappen" } else { "Zuklappen" },
                    onclick: move |_| collapsed.toggle(),
                    if collapsed() { ChevronUp { class: "size-4" } } else { ChevronDown { class: "size-4" } }
                }
                if active == 0 {
                    button {
                        class: "inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent",
                        title: "Liste schliessen",
                        onclick: move |_| uploads.write().clear(),
                        X { class: "size-4" }
                    }
                }
            }
            if !collapsed() {
                div { class: "max-h-72 overflow-y-auto divide-y",
                    for item in list.into_iter().rev() {
                        UploadRow { key: "{item.id}", item }
                    }
                }
                if active > 0 {
                    p { class: "px-3 py-2 text-[11px] text-muted-foreground border-t",
                        "Seite bitte geöffnet lassen, bis alle Uploads fertig sind."
                    }
                }
            }
        }
    }
}

#[component]
fn UploadRow(item: UploadItem) -> Element {
    let fm = use_context::<Fm>();
    let pct = if item.size > 0.0 { (item.loaded / item.size * 100.0).clamp(0.0, 100.0) } else { 0.0 };
    let id = item.id;
    let dir = logic::name_of(&item.dir);
    rsx! {
        div { class: "flex items-start gap-3 px-3 py-2.5",
            div { class: "mt-0.5 shrink-0",
                match &item.state {
                    UpState::Done => rsx! { CircleCheck { class: "size-4 text-success" } },
                    UpState::Failed(_) => rsx! { CircleX { class: "size-4 text-destructive" } },
                    UpState::Cancelled => rsx! { CircleX { class: "size-4 text-muted-foreground" } },
                    UpState::Running => rsx! { LoaderCircle { class: "size-4 animate-spin text-muted-foreground" } },
                    UpState::Queued => rsx! { CloudUpload { class: "size-4 text-muted-foreground" } },
                }
            }
            div { class: "min-w-0 flex-1",
                div { class: "truncate text-sm", title: "{item.name}", "{item.name}" }
                match &item.state {
                    UpState::Running => rsx! {
                        Progress { value: pct, class: "mt-1.5 h-1.5" }
                        div { class: "mt-1 text-xs tabular-nums text-muted-foreground",
                            "{fmt_bytes(item.loaded)} von {fmt_bytes(item.size)} · {pct:.0} %"
                        }
                    },
                    UpState::Queued => rsx! { div { class: "text-xs text-muted-foreground", "Wartet · {fmt_bytes(item.size)} → {dir}" } },
                    UpState::Done => rsx! { div { class: "text-xs text-muted-foreground", "{fmt_bytes(item.size)} → {dir}" } },
                    UpState::Cancelled => rsx! { div { class: "text-xs text-muted-foreground", "Abgebrochen" } },
                    UpState::Failed(e) => rsx! { div { class: "text-xs text-destructive break-words", "{e}" } },
                }
            }
            if item.active() {
                button {
                    class: "inline-flex size-8 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground",
                    title: "Abbrechen",
                    onclick: move |_| cancel(fm, id),
                    X { class: "size-4" }
                }
            }
        }
    }
}
