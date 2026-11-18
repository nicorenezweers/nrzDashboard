//! KI-Agent page: chat with an agent (Claude or Grok) that drives the umbrelOS MCP tools.
//! The loop runs on the server; this page polls the conversation every 700 ms while it is busy.

mod chat;
mod markdown;
mod settings;
mod sidebar;

use dioxus::prelude::*;
use icons::{Menu, Settings};

use crate::api::agent::{
    AgentSettings, ConvSummary, Conversation, Mode, Provider, RunState, agent_cancel, agent_decide,
    agent_delete, agent_get, agent_list, agent_rename, agent_send, agent_set_options,
    agent_settings,
};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::widgets::ErrorBox;
use crate::utils::{err_msg, sleep_ms};
use chat::{ChatItem, Composer, EmptyState, LiveAnswer};
use settings::{SELECT, SettingsPanel};
use sidebar::ConversationList;

const POLL_MS: u32 = 700;

fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1} M", n as f64 / 1e6)
    } else if n >= 10_000 {
        format!("{:.0} k", n as f64 / 1e3)
    } else if n >= 1_000 {
        format!("{:.1} k", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

#[component]
pub fn AgentPage() -> Element {
    let mut settings = use_signal(|| None::<AgentSettings>);
    let mut list = use_signal(Vec::<ConvSummary>::new);
    let mut selected = use_signal(|| None::<String>);
    let mut conv = use_signal(|| None::<Conversation>);
    let mut provider = use_signal(Provider::default);
    let mut mode = use_signal(Mode::default);
    let mut input = use_signal(String::new);
    let mut sending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut show_settings = use_signal(|| false);
    let mut show_list = use_signal(|| false);

    let refresh_list = move || {
        spawn(async move {
            if let Ok(l) = agent_list().await {
                list.set(l);
            }
        });
    };

    // Loads a conversation now (version 0 = always return it).
    let load = move |id: String| {
        spawn(async move {
            match agent_get(id.clone(), 0).await {
                Ok(Some(c)) if selected.peek().as_deref() == Some(id.as_str()) => {
                    provider.set(c.provider);
                    mode.set(c.mode);
                    conv.set(Some(c));
                }
                Ok(_) => {}
                Err(e) => error.set(Some(err_msg(&e))),
            }
        });
    };

    // Initial data + live polling while the agent works.
    use_future(move || async move {
        match agent_settings().await {
            Ok(s) => {
                provider.set(s.provider);
                mode.set(s.mode);
                settings.set(Some(s));
            }
            Err(e) => error.set(Some(err_msg(&e))),
        }
        if let Ok(l) = agent_list().await {
            list.set(l);
        }
        loop {
            sleep_ms(POLL_MS).await;
            let Some(id) = selected.peek().clone() else {
                continue;
            };
            let (version, busy) = match conv.peek().as_ref() {
                Some(c) if c.id == id => (c.version, c.state != RunState::Idle),
                _ => continue,
            };
            if !busy {
                continue;
            }
            match agent_get(id.clone(), version).await {
                Ok(Some(c)) if selected.peek().as_deref() == Some(id.as_str()) => {
                    let finished = c.state == RunState::Idle;
                    let state_changed = conv.peek().as_ref().is_some_and(|o| o.state != c.state);
                    conv.set(Some(c));
                    if finished || state_changed {
                        if let Ok(l) = agent_list().await {
                            list.set(l);
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => error.set(Some(err_msg(&e))),
            }
        }
    });

    // Keep the newest message in view (only when the user is already near the bottom).
    use_effect(move || {
        let _ = conv.read();
        document::eval(
            "requestAnimationFrame(() => { const el = document.getElementById('agent-scroll'); \
             if (el && el.scrollHeight - el.scrollTop - el.clientHeight < 240) el.scrollTop = el.scrollHeight; });",
        );
    });

    let select = move |id: String| {
        show_list.set(false);
        show_settings.set(false);
        error.set(None);
        if selected.peek().as_deref() != Some(id.as_str()) {
            conv.set(None);
            selected.set(Some(id.clone()));
        }
        load(id);
        document::eval(
            "setTimeout(() => { const el = document.getElementById('agent-scroll'); if (el) el.scrollTop = el.scrollHeight; }, 150);",
        );
    };

    let new_chat = move |_| {
        show_list.set(false);
        show_settings.set(false);
        selected.set(None);
        conv.set(None);
        error.set(None);
        if let Some(s) = settings.peek().as_ref() {
            provider.set(s.provider);
            mode.set(s.mode);
        }
    };

    let send_text = move |text: String| {
        let text = text.trim().to_string();
        if text.is_empty() || sending() {
            return;
        }
        spawn(async move {
            sending.set(true);
            error.set(None);
            input.set(String::new());
            let id = selected.peek().clone().unwrap_or_default();
            match agent_send(id.clone(), text.clone(), provider(), mode()).await {
                Ok(new_id) => {
                    if id != new_id {
                        selected.set(Some(new_id.clone()));
                    }
                    if let Ok(Some(c)) = agent_get(new_id, 0).await {
                        conv.set(Some(c));
                    }
                    document::eval(
                        "setTimeout(() => { const el = document.getElementById('agent-scroll'); if (el) el.scrollTop = el.scrollHeight; }, 50);",
                    );
                    if let Ok(l) = agent_list().await {
                        list.set(l);
                    }
                }
                Err(e) => {
                    error.set(Some(err_msg(&e)));
                    if input.peek().is_empty() {
                        input.set(text);
                    }
                }
            }
            sending.set(false);
        });
    };

    let mut set_options = move |p: Provider, m: Mode| {
        provider.set(p);
        mode.set(m);
        if let Some(id) = selected.peek().clone() {
            spawn(async move {
                if let Err(e) = agent_set_options(id.clone(), p, m).await {
                    error.set(Some(err_msg(&e)));
                }
                load(id);
            });
        }
    };

    let cancel = move |_| {
        if let Some(id) = selected.peek().clone() {
            spawn(async move {
                if let Err(e) = agent_cancel(id.clone()).await {
                    error.set(Some(err_msg(&e)));
                }
                load(id);
                refresh_list();
            });
        }
    };

    let decide = move |(call_id, decision): (String, String)| {
        if let Some(id) = selected.peek().clone() {
            spawn(async move {
                if let Err(e) = agent_decide(id.clone(), call_id, decision).await {
                    error.set(Some(err_msg(&e)));
                }
                load(id);
            });
        }
    };

    let rename = move |(id, title): (String, String)| {
        spawn(async move {
            if let Err(e) = agent_rename(id.clone(), title).await {
                error.set(Some(err_msg(&e)));
            }
            if selected.peek().as_deref() == Some(id.as_str()) {
                load(id);
            }
            refresh_list();
        });
    };

    let delete = move |id: String| {
        spawn(async move {
            if let Err(e) = agent_delete(id.clone()).await {
                error.set(Some(err_msg(&e)));
            }
            if selected.peek().as_deref() == Some(id.as_str()) {
                selected.set(None);
                conv.set(None);
            }
            refresh_list();
        });
    };

    let c = conv();
    let busy = c.as_ref().is_some_and(|c| c.state != RunState::Idle);
    let title = c
        .as_ref()
        .map(|c| c.title.clone())
        .unwrap_or_else(|| "Neue Unterhaltung".into());
    let model = settings()
        .map(|s| s.model(provider()).to_string())
        .unwrap_or_default();
    let missing_key = settings()
        .filter(|s| !s.has_key(provider()))
        .map(|_| provider().label().to_string());
    let list_view = rsx! {
        ConversationList {
            list: list(),
            selected: selected(),
            on_new: new_chat,
            on_select: select,
            on_rename: rename,
            on_delete: delete,
        }
    };

    rsx! {
        div { class: "fixed inset-x-0 top-14 bottom-[calc(4rem+env(safe-area-inset-bottom,0px))] z-30 flex bg-background md:static md:z-auto md:h-[calc(100dvh-4rem)] md:gap-4",
            // Desktop: conversation list
            aside { class: "hidden w-64 shrink-0 flex-col rounded-xl border bg-card p-3 md:flex", {list_view.clone()} }

            // Chat
            section { class: "relative flex min-w-0 flex-1 flex-col md:rounded-xl md:border md:bg-card",
                // Header
                header { class: "relative flex flex-wrap items-center gap-2 border-b px-3 py-2",
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::IconSm,
                        class: "md:hidden",
                        aria_label: "Unterhaltungen",
                        onclick: move |_| show_list.toggle(),
                        Menu { class: "size-4" }
                    }
                    div { class: "min-w-0 flex-1",
                        h1 { class: "truncate text-sm font-semibold", "{title}" }
                        div { class: "flex flex-wrap items-center gap-x-2 text-[11px] text-muted-foreground",
                            span { class: "truncate font-mono", "{model}" }
                            if let Some(c) = c.as_ref().filter(|c| c.usage.total_in() + c.usage.output > 0) {
                                span { title: "Eingabe (davon aus Cache) / Ausgabe",
                                    "· {fmt_tokens(c.usage.total_in())} ein"
                                    if c.usage.cache_read > 0 {
                                        " ({fmt_tokens(c.usage.cache_read)} Cache)"
                                    }
                                    " / {fmt_tokens(c.usage.output)} aus"
                                }
                            }
                        }
                    }
                    select {
                        class: SELECT,
                        title: "Anbieter",
                        value: provider().as_str(),
                        disabled: busy,
                        onchange: move |e| set_options(Provider::parse(&e.value()), mode()),
                        for p in Provider::ALL {
                            option { value: p.as_str(), selected: provider() == p, "{p.label()}" }
                        }
                    }
                    select {
                        class: SELECT,
                        title: "{mode().hint()}",
                        value: mode().as_str(),
                        onchange: move |e| set_options(provider(), Mode::parse(&e.value())),
                        for m in Mode::ALL {
                            option { value: m.as_str(), selected: mode() == m, "{m.label()}" }
                        }
                    }
                    Button {
                        variant: if show_settings() { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                        size: ButtonSize::IconSm,
                        aria_label: "Agent-Einstellungen",
                        title: "Agent-Einstellungen",
                        onclick: move |_| show_settings.toggle(),
                        Settings { class: "size-4" }
                    }
                    // Mobile: conversation list as a drop-down
                    if show_list() {
                        div { class: "absolute inset-x-0 top-full z-20 flex max-h-[60vh] flex-col border-b bg-background p-3 shadow-lg md:hidden",
                            {list_view}
                        }
                    }
                }

                // Messages / settings
                div { id: "agent-scroll", class: "min-h-0 flex-1 overflow-y-auto px-3 py-4 md:px-6",
                    if show_settings() {
                        if let Some(s) = settings() {
                            SettingsPanel {
                                settings: s,
                                on_close: move |_| show_settings.set(false),
                                on_saved: move |s: AgentSettings| {
                                    if selected.peek().is_none() {
                                        provider.set(s.provider);
                                        mode.set(s.mode);
                                    }
                                    settings.set(Some(s));
                                    show_settings.set(false);
                                },
                            }
                        }
                    } else {
                        div { class: "mx-auto flex w-full max-w-3xl flex-col gap-4",
                            match c.as_ref() {
                                None if selected().is_some() => rsx! {
                                    p { class: "py-8 text-center text-sm text-muted-foreground", "Lädt …" }
                                },
                                None => rsx! {
                                    EmptyState {
                                        missing_key,
                                        on_pick: move |t: String| send_text(t),
                                        on_settings: move |_| show_settings.set(true),
                                    }
                                },
                                Some(c) => rsx! {
                                    for (i, item) in c.items.iter().enumerate() {
                                        ChatItem { key: "{i}", item: item.clone(), on_decide: decide }
                                    }
                                    if c.state == RunState::Running {
                                        LiveAnswer { text: c.streaming.clone(), round: c.round }
                                    }
                                },
                            }
                        }
                    }
                }

                // Composer
                div { class: "border-t px-3 py-2 md:px-6 md:py-3",
                    div { class: "mx-auto flex w-full max-w-3xl flex-col gap-2",
                        if let Some(e) = error() {
                            ErrorBox { message: e }
                        }
                        if c.as_ref().is_some_and(|c| c.state == RunState::Waiting) {
                            p { class: "text-xs text-warning", "Der Agent wartet auf deine Bestätigung (siehe oben)." }
                        }
                        Composer {
                            value: input,
                            busy,
                            sending: sending(),
                            on_send: move |_| send_text(input()),
                            on_cancel: cancel,
                        }
                    }
                }
            }
        }
    }
}
