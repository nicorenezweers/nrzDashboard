//! App-specific building blocks on top of the nrzUI components.

use dioxus::prelude::*;
use icons::{RefreshCw, TriangleAlert};

use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::Card;
use crate::components::ui::progress::Progress;

#[component]
pub fn PageHeader(
    title: String,
    #[props(into, optional)] subtitle: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "mb-6 flex flex-wrap items-end justify-between gap-3",
            div {
                h1 { class: "text-2xl font-semibold tracking-tight", "{title}" }
                if let Some(s) = subtitle {
                    p { class: "text-sm text-muted-foreground mt-1", "{s}" }
                }
            }
            div { class: "flex flex-wrap items-center gap-2", {children} }
        }
    }
}

#[component]
pub fn RefreshButton(
    onclick: EventHandler<MouseEvent>,
    #[props(optional)] loading: bool,
) -> Element {
    rsx! {
        Button {
            variant: ButtonVariant::Outline,
            size: ButtonSize::Sm,
            onclick: move |e| onclick.call(e),
            RefreshCw { class: if loading { "size-4 animate-spin" } else { "size-4" } }
            span { class: "hidden sm:inline", "Aktualisieren" }
        }
    }
}

#[component]
pub fn ErrorBox(message: String) -> Element {
    rsx! {
        div { class: "flex items-start gap-3 rounded-lg border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive",
            TriangleAlert { class: "size-4 mt-0.5 shrink-0" }
            span { class: "break-words min-w-0", "{message}" }
        }
    }
}

#[component]
pub fn Notice(message: String) -> Element {
    rsx! {
        div { class: "rounded-lg border bg-muted/40 p-3 text-sm text-muted-foreground break-words", "{message}" }
    }
}

/// Card with a big number and an optional progress bar.
#[component]
pub fn StatCard(
    label: String,
    value: String,
    #[props(into, optional)] detail: Option<String>,
    #[props(optional)] percent: Option<f64>,
    icon: Element,
) -> Element {
    let bar = percent.map(|p| {
        let color = if p >= 90.0 {
            "[&>div]:bg-destructive"
        } else if p >= 75.0 {
            "[&>div]:bg-warning"
        } else {
            ""
        };
        (p, color)
    });
    rsx! {
        Card { class: "gap-3 py-4 px-4",
            div { class: "flex items-center justify-between text-sm text-muted-foreground",
                span { "{label}" }
                span { class: "[&_svg]:size-4", {icon} }
            }
            div { class: "text-2xl font-semibold tabular-nums", "{value}" }
            if let Some((p, color)) = bar {
                Progress { value: p, class: "h-1.5 {color}" }
            }
            if let Some(d) = detail {
                div { class: "text-xs text-muted-foreground truncate", "{d}" }
            }
        }
    }
}

#[component]
pub fn StatusDot(up: Option<bool>) -> Element {
    let class = match up {
        Some(true) => "bg-success shadow-[0_0_0_3px] shadow-success/20",
        Some(false) => "bg-destructive shadow-[0_0_0_3px] shadow-destructive/20",
        None => "bg-muted-foreground/50",
    };
    rsx! { span { class: "inline-block size-2.5 shrink-0 rounded-full {class}" } }
}

/// Button that needs a second click to fire (no JS confirm dialogs).
#[component]
pub fn ConfirmButton(
    label: String,
    #[props(into, default = "Wirklich?".to_string())] confirm_label: String,
    #[props(default = ButtonVariant::Outline)] variant: ButtonVariant,
    #[props(default = ButtonSize::Sm)] size: ButtonSize,
    #[props(optional)] disabled: bool,
    #[props(into, optional)] class: Option<String>,
    onconfirm: EventHandler<()>,
    children: Element,
) -> Element {
    let mut armed = use_signal(|| false);
    rsx! {
        Button {
            variant: if armed() { ButtonVariant::Destructive } else { variant },
            size,
            disabled,
            class: class.unwrap_or_default(),
            onclick: move |_| {
                if armed() {
                    armed.set(false);
                    onconfirm.call(());
                } else {
                    armed.set(true);
                    spawn(async move {
                        crate::utils::sleep_ms(4000).await;
                        armed.set(false);
                    });
                }
            },
            {children}
            if armed() { "{confirm_label}" } else { "{label}" }
        }
    }
}

pub fn input_class() -> &'static str {
    "text-foreground placeholder:text-muted-foreground dark:bg-input/30 border-input h-9 w-full min-w-0 rounded-md border bg-transparent px-3 py-1 text-base md:text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50"
}
