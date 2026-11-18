//! Chat history: user/agent messages, tool cards, confirmation card, empty state, composer.

use dioxus::prelude::*;
use icons::{
    Ban, Bot, ChevronRight, CircleCheck, CircleStop, CircleX, Clock, LoaderCircle, Send,
    ShieldAlert, Sparkles, TriangleAlert, Wrench,
};

use super::markdown::Markdown;
use crate::api::agent::{Item, ToolStatus, ToolView};
use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};

pub const EXAMPLES: [&str; 6] = [
    "Welche Apps verbrauchen am meisten RAM?",
    "Zeig mir die letzten Logs von nrzGit",
    "Wie voll ist mein Speicher und was belegt am meisten Platz?",
    "Gibt es Updates für umbrelOS oder meine Apps?",
    "Welche Apps sind gerade gestoppt?",
    "Fasse den Systemzustand kurz zusammen",
];

#[component]
pub fn ChatItem(item: Item, on_decide: EventHandler<(String, String)>) -> Element {
    match item {
        Item::User { text } => rsx! {
            div { class: "flex justify-end",
                div { class: "max-w-[85%] whitespace-pre-wrap break-words rounded-2xl rounded-br-md bg-secondary px-4 py-2 text-sm text-secondary-foreground",
                    "{text}"
                }
            }
        },
        Item::Assistant { text } => rsx! {
            div { class: "flex gap-3",
                AgentAvatar {}
                div { class: "min-w-0 flex-1 pt-0.5", Markdown { text } }
            }
        },
        Item::Tool(tool) => {
            if tool.status == ToolStatus::Approval {
                rsx! { ApprovalCard { tool, on_decide } }
            } else {
                rsx! { ToolCard { tool } }
            }
        }
        Item::Notice { text, error } => rsx! {
            div {
                class: if error { "flex items-start gap-2 rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive" } else { "flex items-start gap-2 rounded-lg border bg-muted/40 px-3 py-2 text-sm text-muted-foreground" },
                if error {
                    TriangleAlert { class: "size-4 mt-0.5 shrink-0" }
                } else {
                    CircleStop { class: "size-4 mt-0.5 shrink-0" }
                }
                span { class: "min-w-0 break-words", "{text}" }
            }
        },
    }
}

#[component]
pub fn AgentAvatar() -> Element {
    rsx! {
        div { class: "flex size-7 shrink-0 items-center justify-center rounded-full border bg-muted text-muted-foreground",
            Bot { class: "size-4" }
        }
    }
}

/// Streaming answer and "thinking" indicator.
#[component]
pub fn LiveAnswer(text: String, round: u32) -> Element {
    rsx! {
        div { class: "flex gap-3",
            AgentAvatar {}
            div { class: "min-w-0 flex-1 pt-0.5",
                if text.is_empty() {
                    div { class: "flex items-center gap-2 text-sm text-muted-foreground",
                        LoaderCircle { class: "size-4 animate-spin" }
                        if round > 1 {
                            "Arbeitet … (Runde {round})"
                        } else {
                            "Denkt nach …"
                        }
                    }
                } else {
                    Markdown { text }
                    span { class: "ml-0.5 inline-block h-4 w-1.5 animate-pulse rounded-sm bg-muted-foreground align-middle" }
                }
            }
        }
    }
}

fn status_icon(s: ToolStatus) -> Element {
    match s {
        ToolStatus::Pending => rsx! { Clock { class: "size-4 shrink-0 text-muted-foreground" } },
        ToolStatus::Approval => rsx! { ShieldAlert { class: "size-4 shrink-0 text-warning" } },
        ToolStatus::Running => {
            rsx! { LoaderCircle { class: "size-4 shrink-0 animate-spin text-info" } }
        }
        ToolStatus::Done => rsx! { CircleCheck { class: "size-4 shrink-0 text-success" } },
        ToolStatus::Error => rsx! { CircleX { class: "size-4 shrink-0 text-destructive" } },
        ToolStatus::Denied => rsx! { Ban { class: "size-4 shrink-0 text-destructive" } },
        ToolStatus::Cancelled => {
            rsx! { CircleStop { class: "size-4 shrink-0 text-muted-foreground" } }
        }
    }
}

const fn status_label(s: ToolStatus) -> &'static str {
    match s {
        ToolStatus::Pending => "wartet",
        ToolStatus::Approval => "Bestätigung nötig",
        ToolStatus::Running => "läuft",
        ToolStatus::Done => "fertig",
        ToolStatus::Error => "Fehler",
        ToolStatus::Denied => "abgelehnt",
        ToolStatus::Cancelled => "abgebrochen",
    }
}

fn fmt_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.1} s", ms as f64 / 1000.0)
    }
}

const PRE: &str = "max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-md border bg-muted/40 p-2 font-mono text-xs";

#[component]
fn ToolCard(tool: ToolView) -> Element {
    let args_empty = tool.args.trim() == "{}" || tool.args.trim().is_empty();
    rsx! {
        details { class: "group ml-10 rounded-lg border bg-card text-sm",
            summary { class: "flex cursor-pointer list-none items-center gap-2 px-3 py-2 select-none [&::-webkit-details-marker]:hidden",
                {status_icon(tool.status)}
                Wrench { class: "size-3.5 shrink-0 text-muted-foreground" }
                span { class: "min-w-0 truncate font-mono text-xs", "{tool.name}" }
                if tool.destructive {
                    Badge { variant: BadgeVariant::Warning, size: BadgeSize::Sm, "kritisch" }
                }
                span { class: "flex-1" }
                span { class: "hidden text-xs text-muted-foreground sm:inline", "{status_label(tool.status)}" }
                if let Some(ms) = tool.duration_ms {
                    span { class: "text-xs tabular-nums text-muted-foreground", "{fmt_ms(ms)}" }
                }
                ChevronRight { class: "size-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-90" }
            }
            div { class: "flex flex-col gap-2 border-t px-3 py-2",
                div {
                    div { class: "mb-1 text-xs font-medium text-muted-foreground", "Argumente" }
                    if args_empty {
                        div { class: "text-xs text-muted-foreground", "keine" }
                    } else {
                        pre { class: PRE, "{tool.args}" }
                    }
                }
                if let Some(r) = tool.result.clone() {
                    div {
                        div { class: "mb-1 text-xs font-medium text-muted-foreground", "Ergebnis" }
                        pre { class: PRE, "{r}" }
                    }
                }
            }
        }
    }
}

#[component]
fn ApprovalCard(tool: ToolView, on_decide: EventHandler<(String, String)>) -> Element {
    let id = tool.id.clone();
    let id2 = tool.id.clone();
    let id3 = tool.id.clone();
    rsx! {
        div { class: "ml-10 rounded-lg border border-warning/60 bg-warning/10 p-3 text-sm",
            div { class: "flex items-center gap-2 font-medium",
                ShieldAlert { class: "size-4 shrink-0 text-warning" }
                span { "Bestätigung nötig: " }
                span { class: "min-w-0 truncate font-mono text-xs", "{tool.name}" }
            }
            p { class: "mt-1 text-xs text-muted-foreground",
                "Der Agent möchte ein kritisches Werkzeug ausführen. Prüfe die Argumente:"
            }
            pre { class: "mt-2 {PRE}", "{tool.args}" }
            div { class: "mt-3 flex flex-wrap gap-2",
                Button {
                    size: ButtonSize::Sm,
                    onclick: move |_| on_decide.call((id.clone(), "allow".into())),
                    CircleCheck { class: "size-4" }
                    "Erlauben"
                }
                Button {
                    size: ButtonSize::Sm,
                    variant: ButtonVariant::Outline,
                    onclick: move |_| on_decide.call((id2.clone(), "always".into())),
                    "In dieser Unterhaltung immer erlauben"
                }
                Button {
                    size: ButtonSize::Sm,
                    variant: ButtonVariant::Destructive,
                    onclick: move |_| on_decide.call((id3.clone(), "deny".into())),
                    Ban { class: "size-4" }
                    "Ablehnen"
                }
            }
        }
    }
}

#[component]
pub fn EmptyState(
    on_pick: EventHandler<String>,
    missing_key: Option<String>,
    on_settings: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "mx-auto flex max-w-2xl flex-col items-center gap-5 py-6 text-center md:py-12",
            div { class: "flex size-12 items-center justify-center rounded-full border bg-muted",
                Sparkles { class: "size-6 text-muted-foreground" }
            }
            div {
                h2 { class: "text-lg font-semibold", "Was soll ich auf deinem Umbrel erledigen?" }
                p { class: "mt-1 text-sm text-muted-foreground",
                    "Der Agent nutzt die Werkzeuge deines umbrelOS: Apps, Dateien, Logs, System. Kritische Schritte musst du bestätigen."
                }
            }
            if let Some(p) = missing_key {
                div { class: "flex flex-col items-center gap-2 rounded-lg border bg-muted/40 p-3 text-sm text-muted-foreground",
                    "Für {p} ist noch kein API-Schlüssel hinterlegt."
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Outline,
                        onclick: move |_| on_settings.call(()),
                        "Einstellungen öffnen"
                    }
                }
            }
            div { class: "grid w-full grid-cols-1 gap-2 sm:grid-cols-2",
                for ex in EXAMPLES {
                    button {
                        r#type: "button",
                        class: "rounded-lg border bg-card px-3 py-2 text-left text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground",
                        onclick: move |_| on_pick.call(ex.to_string()),
                        "{ex}"
                    }
                }
            }
        }
    }
}

/// Input at the bottom: Enter sends, Shift+Enter adds a line, grows with the text.
#[component]
pub fn Composer(
    value: Signal<String>,
    busy: bool,
    sending: bool,
    on_send: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut value = value;
    let lines = value.read().lines().count().max(1) + usize::from(value.read().ends_with('\n'));
    let rows = lines.clamp(1, 8);
    let can_send = !busy && !sending && !value.read().trim().is_empty();
    rsx! {
        div { class: "flex items-end gap-2 rounded-xl border bg-card p-2 shadow-xs focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/30",
            textarea {
                class: "max-h-48 min-h-9 flex-1 resize-none bg-transparent px-2 py-1.5 text-base outline-none placeholder:text-muted-foreground field-sizing-content md:text-sm",
                rows: "{rows}",
                placeholder: if busy { "Der Agent arbeitet …" } else { "Nachricht an den Agenten … (Enter senden, Shift+Enter neue Zeile)" },
                value: "{value}",
                oninput: move |e| value.set(e.value()),
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Enter && !e.modifiers().contains(Modifiers::SHIFT) && !e.is_composing() {
                        e.prevent_default();
                        if can_send {
                            on_send.call(());
                        }
                    }
                },
            }
            if busy {
                Button {
                    variant: ButtonVariant::Destructive,
                    size: ButtonSize::Icon,
                    aria_label: "Abbrechen",
                    title: "Abbrechen",
                    onclick: move |_| on_cancel.call(()),
                    CircleStop { class: "size-4" }
                }
            } else {
                Button {
                    size: ButtonSize::Icon,
                    disabled: !can_send,
                    aria_label: "Senden",
                    title: "Senden",
                    onclick: move |_| on_send.call(()),
                    if sending {
                        LoaderCircle { class: "size-4 animate-spin" }
                    } else {
                        Send { class: "size-4" }
                    }
                }
            }
        }
    }
}
