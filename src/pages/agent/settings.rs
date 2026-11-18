//! Agent settings: provider, mode, API keys (only masked hints come back) and models.

use dioxus::prelude::*;
use icons::{KeyRound, X};

use crate::api::agent::{AgentSettings, AgentSettingsInput, Mode, Provider, agent_save_settings};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::widgets::{ErrorBox, input_class};
use crate::utils::err_msg;

pub const SELECT: &str = "h-8 rounded-md border border-input bg-transparent px-2 text-sm text-foreground shadow-xs outline-none dark:bg-input/30 focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 [&>option]:bg-popover [&>option]:text-popover-foreground";

#[component]
pub fn SettingsPanel(
    settings: AgentSettings,
    on_close: EventHandler<()>,
    on_saved: EventHandler<AgentSettings>,
) -> Element {
    let mut provider = use_signal(|| settings.provider);
    let mut mode = use_signal(|| settings.mode);
    let claude_model = use_signal(|| settings.claude_model.clone());
    let grok_model = use_signal(|| settings.grok_model.clone());
    let mut claude_key = use_signal(String::new);
    let mut grok_key = use_signal(String::new);
    let clear_claude = use_signal(|| false);
    let clear_grok = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let save = move |_| {
        spawn(async move {
            saving.set(true);
            error.set(None);
            let input = AgentSettingsInput {
                provider: provider(),
                mode: mode(),
                claude_model: claude_model(),
                grok_model: grok_model(),
                claude_key: claude_key(),
                grok_key: grok_key(),
                clear_claude_key: clear_claude(),
                clear_grok_key: clear_grok(),
            };
            match agent_save_settings(input).await {
                Ok(s) => {
                    claude_key.set(String::new());
                    grok_key.set(String::new());
                    on_saved.call(s);
                }
                Err(e) => error.set(Some(err_msg(&e))),
            }
            saving.set(false);
        });
    };

    rsx! {
        div { class: "mx-auto flex w-full max-w-2xl flex-col gap-4",
            div { class: "flex items-center justify-between gap-2",
                h2 { class: "text-lg font-semibold", "Agent-Einstellungen" }
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    aria_label: "Schliessen",
                    onclick: move |_| on_close.call(()),
                    X { class: "size-4" }
                }
            }
            if let Some(e) = error() {
                ErrorBox { message: e }
            }
            Card {
                CardContent { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    label { class: "flex flex-col gap-1.5 text-sm",
                        span { class: "font-medium", "Standard-Anbieter" }
                        select {
                            class: SELECT,
                            value: provider().as_str(),
                            onchange: move |e| provider.set(Provider::parse(&e.value())),
                            for p in Provider::ALL {
                                option { value: p.as_str(), selected: provider() == p, "{p.label()}" }
                            }
                        }
                    }
                    label { class: "flex flex-col gap-1.5 text-sm",
                        span { class: "font-medium", "Standard-Modus" }
                        select {
                            class: SELECT,
                            value: mode().as_str(),
                            onchange: move |e| mode.set(Mode::parse(&e.value())),
                            for m in Mode::ALL {
                                option { value: m.as_str(), selected: mode() == m, "{m.label()}" }
                            }
                        }
                        span { class: "text-xs text-muted-foreground", "{mode().hint()}" }
                    }
                }
            }
            ProviderCard {
                title: "Claude (Anthropic)",
                description: "Schlüssel von console.anthropic.com – alternativ Umgebungsvariable ANTHROPIC_API_KEY.",
                hint: settings.claude_key_hint.clone(),
                from_env: settings.claude_key_env,
                default_model: crate::api::agent::DEFAULT_CLAUDE_MODEL,
                api_key: claude_key,
                model: claude_model,
                clear: clear_claude,
            }
            ProviderCard {
                title: "Grok (xAI)",
                description: "Schlüssel von console.x.ai – alternativ Umgebungsvariable XAI_API_KEY.",
                hint: settings.grok_key_hint.clone(),
                from_env: settings.grok_key_env,
                default_model: crate::api::agent::DEFAULT_GROK_MODEL,
                api_key: grok_key,
                model: grok_model,
                clear: clear_grok,
            }
            div { class: "flex justify-end gap-2 pb-2",
                Button { variant: ButtonVariant::Outline, onclick: move |_| on_close.call(()), "Abbrechen" }
                Button { disabled: saving(), onclick: save,
                    if saving() { "Speichert …" } else { "Speichern" }
                }
            }
        }
    }
}

#[component]
fn ProviderCard(
    title: String,
    description: String,
    hint: Option<String>,
    from_env: bool,
    default_model: String,
    api_key: Signal<String>,
    model: Signal<String>,
    clear: Signal<bool>,
) -> Element {
    let mut key = api_key;
    let mut model = model;
    let mut clear = clear;
    let status = match (&hint, from_env) {
        (Some(h), true) => format!("Aus Umgebungsvariable: {h}"),
        (Some(h), false) => format!("Gespeichert: {h}"),
        (None, _) => "Kein Schlüssel hinterlegt".to_string(),
    };
    rsx! {
        Card {
            CardHeader {
                CardTitle { class: "flex items-center gap-2", KeyRound { class: "size-4" } "{title}" }
                CardDescription { "{description}" }
            }
            CardContent { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                div { class: "flex flex-col gap-1.5 text-sm",
                    span { class: "font-medium", "API-Schlüssel" }
                    input {
                        class: input_class(),
                        r#type: "password",
                        autocomplete: "off",
                        placeholder: if hint.is_some() { "Leer lassen = unverändert" } else { "Schlüssel einfügen" },
                        value: "{key}",
                        oninput: move |e| key.set(e.value()),
                    }
                    span { class: "text-xs text-muted-foreground", "{status}" }
                    if hint.is_some() && !from_env {
                        label { class: "flex items-center gap-2 text-xs text-muted-foreground",
                            input {
                                r#type: "checkbox",
                                checked: clear(),
                                onchange: move |e| clear.set(e.checked()),
                            }
                            "Gespeicherten Schlüssel entfernen"
                        }
                    }
                }
                label { class: "flex flex-col gap-1.5 text-sm",
                    span { class: "font-medium", "Modell" }
                    input {
                        class: input_class(),
                        placeholder: "{default_model}",
                        value: "{model}",
                        oninput: move |e| model.set(e.value()),
                    }
                    span { class: "text-xs text-muted-foreground", "Standard: {default_model}" }
                }
            }
        }
    }
}
