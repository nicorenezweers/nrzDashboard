//! Conversation list: new, select, rename, delete.

use dioxus::prelude::*;
use icons::{Check, LoaderCircle, MessageSquare, Pencil, Plus, ShieldAlert, Trash2, X};

use crate::api::agent::{ConvSummary, RunState};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::widgets::input_class;

#[component]
pub fn ConversationList(
    list: Vec<ConvSummary>,
    selected: Option<String>,
    on_new: EventHandler<()>,
    on_select: EventHandler<String>,
    on_rename: EventHandler<(String, String)>,
    on_delete: EventHandler<String>,
) -> Element {
    let mut editing = use_signal(|| None::<String>);
    let mut draft = use_signal(String::new);
    let mut armed = use_signal(|| None::<String>);

    rsx! {
        div { class: "flex min-h-0 flex-1 flex-col gap-2",
            Button {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Sm,
                class: "w-full justify-start",
                onclick: move |_| on_new.call(()),
                Plus { class: "size-4" }
                "Neue Unterhaltung"
            }
            div { class: "-mx-1 flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-1",
                if list.is_empty() {
                    p { class: "px-2 py-4 text-center text-xs text-muted-foreground", "Noch keine Unterhaltungen" }
                }
                for c in list {
                    {
                        let active = selected.as_deref() == Some(c.id.as_str());
                        let id = c.id.clone();
                        let is_editing = editing.read().as_deref() == Some(c.id.as_str());
                        let is_armed = armed.read().as_deref() == Some(c.id.as_str());
                        let time = crate::utils::fmt_time_ms(c.updated_ms as f64);
                        if is_editing {
                            let id_ok = id.clone();
                            let id_key = id.clone();
                            rsx! {
                                div { key: "{id}", class: "flex items-center gap-1 rounded-md bg-accent p-1",
                                    input {
                                        class: "{input_class()} h-8",
                                        value: "{draft}",
                                        autofocus: true,
                                        oninput: move |e| draft.set(e.value()),
                                        onkeydown: move |e: KeyboardEvent| {
                                            if e.key() == Key::Enter {
                                                on_rename.call((id_key.clone(), draft()));
                                                editing.set(None);
                                            } else if e.key() == Key::Escape {
                                                editing.set(None);
                                            }
                                        },
                                    }
                                    Button {
                                        variant: ButtonVariant::Ghost,
                                        size: ButtonSize::IconSm,
                                        aria_label: "Speichern",
                                        onclick: move |_| {
                                            on_rename.call((id_ok.clone(), draft()));
                                            editing.set(None);
                                        },
                                        Check { class: "size-4" }
                                    }
                                    Button {
                                        variant: ButtonVariant::Ghost,
                                        size: ButtonSize::IconSm,
                                        aria_label: "Abbrechen",
                                        onclick: move |_| editing.set(None),
                                        X { class: "size-4" }
                                    }
                                }
                            }
                        } else {
                            let id_sel = id.clone();
                            let id_edit = id.clone();
                            let id_del = id.clone();
                            let title = c.title.clone();
                            rsx! {
                                div {
                                    key: "{id}",
                                    class: if active { "group flex items-center gap-1 rounded-md bg-accent text-accent-foreground" } else { "group flex items-center gap-1 rounded-md text-muted-foreground hover:bg-accent/60 hover:text-foreground" },
                                    button {
                                        r#type: "button",
                                        class: "flex min-w-0 flex-1 items-start gap-2 px-2 py-2 text-left",
                                        onclick: move |_| on_select.call(id_sel.clone()),
                                        match c.state {
                                            RunState::Running => rsx! { LoaderCircle { class: "mt-0.5 size-4 shrink-0 animate-spin text-info" } },
                                            RunState::Waiting => rsx! { ShieldAlert { class: "mt-0.5 size-4 shrink-0 text-warning" } },
                                            RunState::Idle => rsx! { MessageSquare { class: "mt-0.5 size-4 shrink-0" } },
                                        }
                                        span { class: "min-w-0 flex-1",
                                            span { class: "block truncate text-sm", "{c.title}" }
                                            span { class: "block text-[11px] text-muted-foreground", "{c.provider.label()} · {time}" }
                                        }
                                    }
                                    div { class: if is_armed { "flex shrink-0 items-center pr-1" } else { "flex shrink-0 items-center pr-1 md:opacity-0 md:group-hover:opacity-100 md:focus-within:opacity-100" },
                                        if !is_armed {
                                            Button {
                                                variant: ButtonVariant::Ghost,
                                                size: ButtonSize::IconXs,
                                                aria_label: "Umbenennen",
                                                title: "Umbenennen",
                                                onclick: move |_| {
                                                    draft.set(title.clone());
                                                    editing.set(Some(id_edit.clone()));
                                                },
                                                Pencil { class: "size-3.5" }
                                            }
                                        }
                                        Button {
                                            variant: if is_armed { ButtonVariant::Destructive } else { ButtonVariant::Ghost },
                                            size: if is_armed { ButtonSize::Sm } else { ButtonSize::IconXs },
                                            aria_label: "Löschen",
                                            title: "Löschen",
                                            onclick: move |_| {
                                                if is_armed {
                                                    armed.set(None);
                                                    on_delete.call(id_del.clone());
                                                } else {
                                                    armed.set(Some(id_del.clone()));
                                                    spawn(async move {
                                                        crate::utils::sleep_ms(4000).await;
                                                        armed.set(None);
                                                    });
                                                }
                                            },
                                            Trash2 { class: "size-3.5" }
                                            if is_armed {
                                                "Löschen?"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
