//! Text editor for txt, md, html, json, yaml, code … – opens inside the app, never downloads.

use dioxus::prelude::*;
use icons::{Eye, FileCode, LoaderCircle, Save, WrapText, X};

use crate::api::files::{read_text_file, save_text_file};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::utils::err_msg;

use super::logic;
use super::state::Fm;

const TEXTAREA_ID: &str = "fm-editor";

#[component]
pub fn EditorDialog() -> Element {
    let fm = use_context::<Fm>();
    let Some(path) = fm.editor.read().clone() else { return rsx! {} };
    rsx! { Editor { key: "{path}", path } }
}

fn name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn ext_of(path: &str) -> String {
    name_of(path).rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

/// Inserts text at the cursor via the browser so undo/redo keeps working.
fn insert_at_cursor(text: &str) {
    #[cfg(feature = "web")]
    {
        use wasm_bindgen::JsCast;
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            if let Ok(html) = doc.dyn_into::<web_sys::HtmlDocument>() {
                let _ = html.exec_command_with_show_ui_and_value("insertText", false, text);
            }
        }
    }
    #[cfg(not(feature = "web"))]
    let _ = text;
}

fn focus_editor() {
    #[cfg(feature = "web")]
    {
        use wasm_bindgen::JsCast;
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(TEXTAREA_ID))
            .and_then(|e| e.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
        {
            let _ = el.focus();
            el.set_selection_range(0, 0).ok();
        }
    }
}

#[component]
fn Editor(path: String) -> Element {
    let mut fm = use_context::<Fm>();
    let name = name_of(&path);
    let ext = ext_of(&path);
    let lang = logic::language_of(&name);
    let can_preview = matches!(ext.as_str(), "md" | "markdown" | "html" | "htm");
    let is_json = matches!(ext.as_str(), "json" | "jsonc" | "json5");
    let internal = path.starts_with("/Home/") || path.starts_with("/Apps/");

    let mut content = use_signal(String::new);
    let mut original = use_signal(String::new);
    let mut loaded = use_signal(|| false);
    let mut load_error = use_signal(|| None::<String>);
    let mut wrap = use_signal(|| matches!(lang, "Text" | "Markdown" | "Log"));
    let mut preview = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut confirm_close = use_signal(|| false);
    let mut scroll = use_signal(|| 0.0_f64);

    use_future({
        let path = path.clone();
        move || {
            let path = path.clone();
            async move {
                match read_text_file(path).await {
                    Ok(t) => {
                        original.set(t.content.clone());
                        content.set(t.content);
                        loaded.set(true);
                        focus_editor();
                    }
                    Err(e) => load_error.set(Some(err_msg(&e))),
                }
            }
        }
    });

    let dirty = loaded() && content() != original();

    let save = {
        let path = path.clone();
        move || {
            if saving() || !loaded() {
                return;
            }
            let path = path.clone();
            let text = content();
            spawn(async move {
                saving.set(true);
                match save_text_file(path.clone(), text.clone()).await {
                    Ok(s) if s.copied => {
                        fm.toast(format!("Als Kopie gespeichert: {} – auf USB/Netzwerk kann umbrelOS nicht überschreiben.", name_of(&s.path)));
                        original.set(text);
                        fm.reload();
                        fm.editor.set(Some(s.path));
                    }
                    Ok(_) => {
                        original.set(text);
                        fm.toast(if path.starts_with("/Home/") || path.starts_with("/Apps/") {
                            "Gespeichert – die vorherige Version liegt im Papierkorb."
                        } else {
                            "Gespeichert."
                        });
                        fm.reload();
                    }
                    Err(e) => fm.toast_err(err_msg(&e)),
                }
                saving.set(false);
            });
        }
    };
    let save_btn = save.clone();
    let save_key = save;

    let mut close = move || {
        if dirty && !confirm_close() {
            confirm_close.set(true);
            return;
        }
        fm.editor.set(None);
    };

    let format_json = move |_| match serde_json::from_str::<serde_json::Value>(&content()) {
        Ok(v) => {
            if let Ok(pretty) = serde_json::to_string_pretty(&v) {
                content.set(pretty + "\n");
            }
        }
        Err(e) => fm.toast_err(format!("Kein gültiges JSON: {e}")),
    };

    let text = content();
    let lines = text.lines().count().max(1) + usize::from(text.ends_with('\n'));
    let numbers: String = (1..=lines).map(|n| format!("{n}\n")).collect();
    let html_preview = if preview() && can_preview {
        if matches!(ext.as_str(), "md" | "markdown") {
            Some((true, crate::pages::board::widgets::markdown(&text)))
        } else {
            Some((false, text.clone()))
        }
    } else {
        None
    };

    let has_preview = html_preview.is_some();
    rsx! {
        div {
            class: "fixed inset-0 z-[70] flex bg-black/60 md:p-6",
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    e.prevent_default();
                    close();
                }
            },
            div { class: "flex min-h-0 w-full flex-col overflow-hidden bg-card md:rounded-2xl md:border shadow-2xl",
                // Header
                div { class: "flex shrink-0 items-center gap-2 border-b px-3 py-2 md:px-4",
                    FileCode { class: "size-4 shrink-0 text-muted-foreground" }
                    div { class: "min-w-0 flex-1",
                        div { class: "flex items-center gap-2 truncate text-sm font-medium",
                            span { class: "truncate", "{name}" }
                            if dirty {
                                span { class: "size-2 shrink-0 rounded-full bg-warning", title: "Ungespeicherte Änderungen" }
                            }
                        }
                        div { class: "truncate font-mono text-[11px] text-muted-foreground", "{path}" }
                    }
                    if is_json {
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "hidden sm:inline-flex", onclick: format_json, "JSON formatieren" }
                    }
                    if can_preview {
                        Button {
                            size: ButtonSize::Sm,
                            variant: if preview() { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                            title: "Vorschau",
                            onclick: move |_| preview.toggle(),
                            Eye { class: "size-4" }
                            span { class: "hidden sm:inline", "Vorschau" }
                        }
                    }
                    Button {
                        size: ButtonSize::Sm,
                        variant: if wrap() { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                        title: "Zeilenumbruch",
                        onclick: move |_| wrap.toggle(),
                        WrapText { class: "size-4" }
                    }
                    Button {
                        size: ButtonSize::Sm,
                        disabled: !dirty || saving(),
                        title: "Speichern (Strg/⌘+S)",
                        onclick: move |_| save_btn(),
                        if saving() { LoaderCircle { class: "size-4 animate-spin" } } else { Save { class: "size-4" } }
                        span { class: "hidden sm:inline", "Speichern" }
                    }
                    if confirm_close() {
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Destructive, onclick: move |_| fm.editor.set(None), "Verwerfen?" }
                    }
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, title: "Schliessen (Esc)", onclick: move |_| close(), X { class: "size-4" } }
                }

                // Body
                if let Some(e) = load_error() {
                    div { class: "flex flex-1 items-center justify-center p-8 text-center text-sm text-destructive", "{e}" }
                } else if !loaded() {
                    div { class: "flex flex-1 items-center justify-center", LoaderCircle { class: "size-6 animate-spin text-muted-foreground" } }
                } else {
                    div { class: "flex min-h-0 flex-1",
                        div { class: if has_preview { "relative hidden min-h-0 flex-1 md:flex" } else { "relative flex min-h-0 flex-1" },
                            if !wrap() {
                                div { class: "pointer-events-none w-12 shrink-0 select-none overflow-hidden border-r bg-background/40 py-3 pr-2 text-right font-mono text-[13px] leading-6 text-muted-foreground/60",
                                    pre { style: "transform: translateY(-{scroll}px)", "{numbers}" }
                                }
                            }
                            textarea {
                                id: TEXTAREA_ID,
                                class: if wrap() { "min-h-0 flex-1 resize-none bg-transparent p-3 font-mono text-[13px] leading-6 text-foreground outline-none whitespace-pre-wrap break-words" } else { "min-h-0 flex-1 resize-none bg-transparent p-3 font-mono text-[13px] leading-6 text-foreground outline-none whitespace-pre overflow-x-auto" },
                                spellcheck: false,
                                autocomplete: "off",
                                "autocorrect": "off",
                                autocapitalize: "off",
                                wrap: if wrap() { "soft" } else { "off" },
                                value: "{text}",
                                oninput: move |e| {
                                    content.set(e.value());
                                    confirm_close.set(false);
                                },
                                onscroll: move |e: ScrollEvent| scroll.set(e.data().scroll_top()),
                                onkeydown: move |e: KeyboardEvent| {
                                    let m = e.modifiers();
                                    if (m.meta() || m.ctrl()) && e.key() == Key::Character("s".into()) {
                                        e.prevent_default();
                                        save_key();
                                    } else if e.key() == Key::Tab && !m.shift() {
                                        e.prevent_default();
                                        insert_at_cursor("  ");
                                    }
                                },
                            }
                        }
                        if let Some((is_md, html)) = html_preview {
                            div { class: "min-h-0 flex-1 overflow-auto border-l bg-background/40",
                                if is_md {
                                    div { class: "md p-5 text-sm leading-relaxed", dangerous_inner_html: "{html}" }
                                } else {
                                    iframe { class: "h-full w-full bg-white", "sandbox": "", srcdoc: "{html}" }
                                }
                            }
                        }
                    }
                }

                // Status bar
                div { class: "flex shrink-0 items-center gap-3 border-t px-3 py-1.5 text-[11px] text-muted-foreground md:px-4",
                    span { "{lang}" }
                    span { "{lines} Zeilen" }
                    span { class: "hidden sm:inline", "{text.chars().count()} Zeichen" }
                    span { "UTF-8" }
                    div { class: "flex-1" }
                    span { class: "hidden md:inline",
                        if internal { "Speichern legt die vorherige Version in den Papierkorb · Strg/⌘+S" } else { "USB/Netzwerk: Speichern erzeugt eine Kopie · Strg/⌘+S" }
                    }
                }
            }
        }
    }
}
