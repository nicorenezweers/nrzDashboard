//! Small browser helpers. Without the `web` feature (server render) they do nothing.

use dioxus::prelude::*;

#[cfg(feature = "web")]
use wasm_bindgen::JsCast;

#[cfg(feature = "web")]
fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}

pub fn store_get(key: &str) -> Option<String> {
    #[cfg(feature = "web")]
    {
        web_sys::window()?
            .local_storage()
            .ok()??
            .get_item(key)
            .ok()?
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = key;
        None
    }
}

pub fn store_set(key: &str, value: &str) {
    #[cfg(feature = "web")]
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(key, value);
    }
    #[cfg(not(feature = "web"))]
    let _ = (key, value);
}

pub fn copy_text(text: &str) {
    #[cfg(feature = "web")]
    if let Some(w) = web_sys::window() {
        let _ = w.navigator().clipboard().write_text(text);
    }
    #[cfg(not(feature = "web"))]
    let _ = text;
}

/// Starts a download through a temporary `<a download>`.
pub fn trigger_download(url: &str) {
    #[cfg(feature = "web")]
    if let Some(doc) = document()
        && let Ok(a) = doc.create_element("a")
        && let Some(body) = doc.body()
    {
        let _ = a.set_attribute("href", url);
        let _ = a.set_attribute("download", "");
        let _ = body.append_child(&a);
        if let Some(el) = a.dyn_ref::<web_sys::HtmlElement>() {
            el.click();
        }
        let _ = body.remove_child(&a);
    }
    #[cfg(not(feature = "web"))]
    let _ = url;
}

pub fn open_tab(url: &str) {
    #[cfg(feature = "web")]
    if let Some(w) = web_sys::window() {
        let _ = w.open_with_url_and_target(url, "_blank");
    }
    #[cfg(not(feature = "web"))]
    let _ = url;
}

/// Device without hover (phone/tablet): single tap opens, long press selects.
pub fn is_touch() -> bool {
    #[cfg(feature = "web")]
    {
        web_sys::window()
            .and_then(|w| w.match_media("(hover: none)").ok().flatten())
            .is_some_and(|m| m.matches())
    }
    #[cfg(not(feature = "web"))]
    false
}

pub fn page_visible() -> bool {
    #[cfg(feature = "web")]
    {
        document().is_none_or(|d| !d.hidden())
    }
    #[cfg(not(feature = "web"))]
    true
}

/// Viewport width/height in CSS pixels.
pub fn viewport() -> (f64, f64) {
    #[cfg(feature = "web")]
    {
        let w = web_sys::window();
        let get = |v: Option<Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>>| {
            v.and_then(|r| r.ok()).and_then(|v| v.as_f64())
        };
        (
            get(w.as_ref().map(|w| w.inner_width())).unwrap_or(1024.0),
            get(w.as_ref().map(|w| w.inner_height())).unwrap_or(768.0),
        )
    }
    #[cfg(not(feature = "web"))]
    (1024.0, 768.0)
}

pub fn focus(id: &str) {
    #[cfg(feature = "web")]
    if let Some(el) = document().and_then(|d| d.get_element_by_id(id))
        && let Some(el) = el.dyn_ref::<web_sys::HtmlElement>()
    {
        let _ = el.focus();
    }
    #[cfg(not(feature = "web"))]
    let _ = id;
}

/// Opens the file dialog of a hidden `<input type=file>`.
pub fn click(id: &str) {
    #[cfg(feature = "web")]
    if let Some(el) = document().and_then(|d| d.get_element_by_id(id))
        && let Some(el) = el.dyn_ref::<web_sys::HtmlElement>()
    {
        el.click();
    }
    #[cfg(not(feature = "web"))]
    let _ = id;
}

/// Focuses a freshly mounted input and selects the first `len` UTF-16 units (name without extension).
pub fn select_range(e: &MountedEvent, len: u32) {
    #[cfg(feature = "web")]
    if let Some(input) = e
        .downcast::<web_sys::Element>()
        .and_then(|el| el.dyn_ref::<web_sys::HtmlInputElement>().cloned())
    {
        let _ = input.focus();
        let _ = input.set_selection_range(0, len);
    }
    #[cfg(not(feature = "web"))]
    let _ = (e, len);
}

/// True if a key event comes from a text field (page shortcuts must not fire there).
pub fn from_text_field(e: &KeyboardEvent) -> bool {
    #[cfg(feature = "web")]
    {
        e.downcast::<web_sys::KeyboardEvent>()
            .and_then(|k| k.target())
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|el| {
                let tag = el.tag_name().to_ascii_uppercase();
                tag == "INPUT"
                    || tag == "TEXTAREA"
                    || tag == "SELECT"
                    || el.has_attribute("contenteditable")
            })
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = e;
        false
    }
}

/// Whether an (external) drag carries files from the operating system.
pub fn drag_has_files(e: &DragEvent) -> bool {
    #[cfg(feature = "web")]
    {
        e.downcast::<web_sys::DragEvent>()
            .and_then(|d| d.data_transfer())
            .is_some_and(|dt| {
                dt.types()
                    .iter()
                    .any(|t| t.as_string().as_deref() == Some("Files"))
            })
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = e;
        false
    }
}

pub fn set_drop_effect(e: &DragEvent, effect: &str) {
    #[cfg(feature = "web")]
    if let Some(dt) = e
        .downcast::<web_sys::DragEvent>()
        .and_then(|d| d.data_transfer())
    {
        dt.set_drop_effect(effect);
    }
    #[cfg(not(feature = "web"))]
    let _ = (e, effect);
}

/// Marks the start of an in-app drag (Firefox needs some data to start dragging).
pub fn start_drag(e: &DragEvent, text: &str) {
    #[cfg(feature = "web")]
    if let Some(dt) = e
        .downcast::<web_sys::DragEvent>()
        .and_then(|d| d.data_transfer())
    {
        let _ = dt.set_data("text/plain", text);
        dt.set_effect_allowed("move");
    }
    #[cfg(not(feature = "web"))]
    let _ = (e, text);
}
