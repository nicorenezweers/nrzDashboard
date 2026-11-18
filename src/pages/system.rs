use dioxus::prelude::*;
use icons::{ArrowUp, Link2, Newspaper, Plug, Power, ScrollText, Wifi};

use crate::api::news::{get_news_settings, save_news_settings};
use crate::api::{get_umbrel_settings, restart_device, save_umbrel_settings, umbrel_tool_text};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::widgets::{ConfirmButton, ErrorBox, Notice, PageHeader, input_class};
use crate::utils::err_msg;

#[component]
pub fn SystemPage() -> Element {
    let mut update = use_resource(|| umbrel_tool_text("check_os_update".into(), "{}".into()));
    let mut network = use_resource(|| umbrel_tool_text("get_network_settings".into(), "{}".into()));
    let mut log_kind = use_signal(|| "umbrelos");
    let mut log_lines = use_signal(|| 200u32);
    let mut logs = use_resource(move || {
        let args = format!("{{\"type\":\"{}\",\"lines\":{}}}", log_kind(), log_lines());
        async move { umbrel_tool_text("get_system_logs".into(), args).await }
    });
    let mut restart_msg = use_signal(|| None::<String>);

    // Live: logs every 5 s, network every 30 s, update check every 10 min.
    use_future(move || async move {
        let mut tick: u32 = 0;
        loop {
            crate::utils::sleep_ms(5_000).await;
            tick += 1;
            logs.restart();
            if tick % 6 == 0 {
                network.restart();
            }
            if tick % 120 == 0 {
                update.restart();
            }
        }
    });

    rsx! {
        PageHeader { title: "System", subtitle: "Umbrel- und nrzNews-Verbindung, Updates, Netzwerk, Logs und Neustart" }

        ConnectionCard {
            onsaved: move |_| {
                update.restart();
                network.restart();
                logs.restart();
            },
        }

        div { class: "mt-4", IntegrationsCard {} }

        div { class: "mt-4", NewsConnectionCard {} }

        div { class: "mt-4 grid grid-cols-1 gap-4 lg:grid-cols-2",
            Card {
                CardHeader {
                    CardTitle { class: "flex items-center gap-2", ArrowUp { class: "size-4" } "umbrelOS-Update" }
                    CardDescription { "Verfügbarkeit und Release Notes" }
                }
                CardContent { class: "flex flex-col gap-3",
                    TextResult { data: update.read().clone() }
                }
            }
            Card {
                CardHeader {
                    CardTitle { class: "flex items-center gap-2", Wifi { class: "size-4" } "Netzwerk" }
                    CardDescription { "Schnittstellen, WLAN und DNS" }
                }
                CardContent { TextResult { data: network.read().clone() } }
            }
        }

        Card { class: "mt-4",
            CardHeader {
                CardTitle { class: "flex items-center gap-2", ScrollText { class: "size-4" } "System-Logs" }
            }
            CardContent { class: "flex flex-col gap-3",
                div { class: "flex flex-wrap items-center gap-2",
                    for (k, l) in [("umbrelos", "umbrelOS"), ("system", "System")] {
                        Button {
                            size: ButtonSize::Sm,
                            variant: if log_kind() == k { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                            onclick: move |_| log_kind.set(k),
                            "{l}"
                        }
                    }
                    div { class: "flex-1" }
                    for n in [100u32, 200, 500, 1500] {
                        Button {
                            size: ButtonSize::Sm,
                            variant: if log_lines() == n { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                            onclick: move |_| log_lines.set(n),
                            "{n}"
                        }
                    }
                }
                TextResult { data: logs.read().clone(), tall: true }
            }
        }

        Card { class: "mt-4 border-destructive/30",
            CardHeader {
                CardTitle { class: "flex items-center gap-2", Power { class: "size-4" } "Gerät neu starten" }
                CardDescription { "Startet das umbrelOS-Gerät neu. Alle Apps sind kurz nicht erreichbar." }
            }
            CardContent { class: "flex flex-col gap-3",
                ConfirmButton {
                    label: "Neu starten",
                    confirm_label: "Jetzt wirklich neu starten?",
                    variant: ButtonVariant::Outline,
                    class: "w-fit",
                    onconfirm: move |_| {
                        spawn(async move {
                            let r = restart_device().await;
                            restart_msg.set(Some(r.unwrap_or_else(|e| err_msg(&e))));
                        });
                    },
                }
                if let Some(m) = restart_msg() {
                    Notice { message: m }
                }
            }
        }
    }
}

#[component]
fn TextResult(data: Option<Result<String, ServerFnError>>, #[props(default)] tall: bool) -> Element {
    let h = if tall { "max-h-[60vh]" } else { "max-h-80" };
    match data {
        None => rsx! { p { class: "text-sm text-muted-foreground", "Lade…" } },
        Some(Err(e)) => rsx! { ErrorBox { message: err_msg(&e) } },
        Some(Ok(t)) => rsx! {
            pre { class: "{h} overflow-auto rounded-md bg-background p-3 text-[11px] leading-relaxed font-mono whitespace-pre-wrap break-all",
                "{t}"
            }
        },
    }
}

/// Umbrel MCP connection: URL + token, stored on the server.
#[component]
fn ConnectionCard(onsaved: EventHandler<()>) -> Element {
    let mut current = use_resource(get_umbrel_settings);
    let mut url = use_signal(String::new);
    let mut token = use_signal(String::new);
    let mut result = use_signal(|| None::<Result<String, String>>);
    let mut busy = use_signal(|| false);

    use_effect(move || {
        if let Some(Ok(s)) = &*current.read() {
            if url.peek().is_empty() {
                url.set(s.url.clone());
            }
        }
    });

    let save = move |e: FormEvent| {
        e.prevent_default();
        spawn(async move {
            busy.set(true);
            result.set(Some(save_umbrel_settings(url(), token()).await.map_err(|e| err_msg(&e))));
            token.set(String::new());
            current.restart();
            onsaved.call(());
            busy.set(false);
        });
    };

    let hint = current.read().as_ref().and_then(|r| r.as_ref().ok()).map(|s| s.token_hint.clone());

    rsx! {
        Card {
            CardHeader {
                CardTitle { class: "flex items-center gap-2", Link2 { class: "size-4" } "Umbrel-Verbindung" }
                CardDescription {
                    "MCP-Token aus umbrelOS → Einstellungen → AI agents (MCP). Wird nur auf dem Server gespeichert."
                }
            }
            CardContent {
                form { class: "grid grid-cols-1 gap-3 md:grid-cols-[1fr_1.5fr_auto] md:items-end", onsubmit: save,
                    label { class: "flex flex-col gap-1.5 text-sm",
                        span { class: "text-muted-foreground", "Umbrel-URL" }
                        input { class: "{input_class()} font-mono", value: "{url}", placeholder: "http://umbrel.local", oninput: move |e| url.set(e.value()) }
                    }
                    label { class: "flex flex-col gap-1.5 text-sm",
                        span { class: "text-muted-foreground",
                            match hint.clone() {
                                Some(Some(h)) => rsx! { "MCP-Token (gesetzt: {h})" },
                                _ => rsx! { "MCP-Token (noch nicht gesetzt)" },
                            }
                        }
                        input {
                            class: "{input_class()} font-mono",
                            r#type: "password",
                            autocomplete: "off",
                            value: "{token}",
                            placeholder: if matches!(hint, Some(Some(_))) { "leer lassen = unverändert" } else { "umbrelmcp_…" },
                            oninput: move |e| token.set(e.value()),
                        }
                    }
                    Button { button_type: "submit", size: ButtonSize::Sm, class: "h-9", disabled: busy(),
                        if busy() { "Prüfe…" } else { "Speichern & testen" }
                    }
                }
                match result() {
                    Some(Ok(m)) => rsx! { p { class: "mt-3 text-sm text-success", "{m}" } },
                    Some(Err(e)) => rsx! { div { class: "mt-3", ErrorBox { message: e } } },
                    None => rsx! {},
                }
            }
        }
    }
}

/// nrzDNS / nrzBudget MCP connections for the DNS and finance widgets.
#[component]
fn IntegrationsCard() -> Element {
    use crate::api::integrations::{Integration, integrations_get};
    let mut infos = use_resource(integrations_get);
    let list = infos.read().clone();
    rsx! {
        Card {
            CardHeader {
                CardTitle { class: "flex items-center gap-2", Plug { class: "size-4" } "Integrationen" }
                CardDescription {
                    "MCP-Zugänge anderer nrz-Apps für die Board-Widgets. Die Standard-Adressen zeigen auf die Container im Umbrel-Netz; Tokens werden nur auf dem Server gespeichert."
                }
            }
            CardContent { class: "flex flex-col gap-5",
                match list {
                    None => rsx! { p { class: "text-sm text-muted-foreground", "Lade…" } },
                    Some(Err(e)) => rsx! { ErrorBox { message: err_msg(&e) } },
                    Some(Ok(items)) => rsx! {
                        for info in items {
                            IntegrationRow {
                                key: "{info.kind.label()}",
                                kind: info.kind,
                                url: info.url.clone(),
                                hint: info.token_hint.clone(),
                                onsaved: move |_| infos.restart(),
                            }
                        }
                    },
                }
                p { class: "text-xs text-muted-foreground",
                    "Für "
                    {Integration::Dns.label()}
                    ": "
                    {Integration::Dns.description()}
                    ". Für "
                    {Integration::Budget.label()}
                    ": "
                    {Integration::Budget.description()}
                    "."
                }
            }
        }
    }
}

#[component]
fn IntegrationRow(
    kind: crate::api::integrations::Integration,
    url: String,
    hint: Option<String>,
    onsaved: EventHandler<()>,
) -> Element {
    use crate::api::integrations::integration_save;
    let mut url_s = use_signal(|| url.clone());
    let mut token = use_signal(String::new);
    let mut result = use_signal(|| None::<Result<String, String>>);
    let mut busy = use_signal(|| false);
    let save = move |e: FormEvent| {
        e.prevent_default();
        spawn(async move {
            busy.set(true);
            result.set(Some(integration_save(kind, url_s(), token()).await.map_err(|e| err_msg(&e))));
            token.set(String::new());
            busy.set(false);
            onsaved.call(());
        });
    };
    rsx! {
        form { class: "flex flex-col gap-2", onsubmit: save,
            div { class: "text-sm font-medium", "{kind.label()}" }
            div { class: "grid grid-cols-1 gap-3 md:grid-cols-[1.2fr_1fr_auto] md:items-end",
                label { class: "flex flex-col gap-1.5 text-sm",
                    span { class: "text-muted-foreground", "MCP-URL" }
                    input { class: "{input_class()} font-mono", value: "{url_s}", placeholder: kind.default_url(), oninput: move |e| url_s.set(e.value()) }
                }
                label { class: "flex flex-col gap-1.5 text-sm",
                    span { class: "text-muted-foreground",
                        match hint.clone() {
                            Some(h) => rsx! { "Token (gesetzt: {h})" },
                            None => rsx! { "Token (noch nicht gesetzt)" },
                        }
                    }
                    input {
                        class: "{input_class()} font-mono",
                        r#type: "password",
                        autocomplete: "off",
                        value: "{token}",
                        placeholder: if hint.is_some() { "leer lassen = unverändert".to_string() } else { kind.token_prefix().to_string() },
                        oninput: move |e| token.set(e.value()),
                    }
                }
                Button { button_type: "submit", size: ButtonSize::Sm, class: "h-9", disabled: busy(),
                    if busy() { "Prüfe…" } else { "Speichern & testen" }
                }
            }
            match result() {
                Some(Ok(m)) => rsx! { p { class: "text-sm text-success", "{m}" } },
                Some(Err(e)) => rsx! { ErrorBox { message: e } },
                None => rsx! {},
            }
        }
    }
}

/// nrzNews-Verbindung: Adresse und API-Token für den MCP-Server von nrzNews (Board-Widget
/// „Nachrichten“). Gespeichert wird erst, wenn der Test klappt.
#[component]
fn NewsConnectionCard() -> Element {
    let mut current = use_resource(get_news_settings);
    let mut url = use_signal(String::new);
    let mut token = use_signal(String::new);
    let mut result = use_signal(|| None::<Result<String, String>>);
    let mut busy = use_signal(|| false);

    use_effect(move || {
        if let Some(Ok(s)) = &*current.read() {
            if url.peek().is_empty() {
                url.set(s.url.clone());
            }
        }
    });

    let submit = move |clear: bool| {
        spawn(async move {
            busy.set(true);
            result.set(Some(save_news_settings(url(), token(), clear).await.map_err(|e| err_msg(&e))));
            token.set(String::new());
            current.restart();
            busy.set(false);
        });
    };

    let hint = current.read().as_ref().and_then(|r| r.as_ref().ok()).map(|s| s.token_hint.clone());
    let connected = matches!(hint, Some(Some(_)));

    rsx! {
        Card {
            CardHeader {
                CardTitle { class: "flex items-center gap-2", Newspaper { class: "size-4" } "nrzNews" }
                CardDescription {
                    "Kennzahlen und Schlagzeilen für das Board-Widget „Nachrichten“. Token in nrzNews unter Einstellungen → Konto → API-Tokens (MCP) anlegen. Auf demselben Umbrel: http://host.docker.internal:8340."
                }
            }
            CardContent {
                form {
                    class: "grid grid-cols-1 gap-3 md:grid-cols-[1fr_1.5fr_auto] md:items-end",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        submit(false);
                    },
                    label { class: "flex flex-col gap-1.5 text-sm",
                        span { class: "text-muted-foreground", "nrzNews-URL" }
                        input { class: "{input_class()} font-mono", value: "{url}", placeholder: "http://host.docker.internal:8340", oninput: move |e| url.set(e.value()) }
                    }
                    label { class: "flex flex-col gap-1.5 text-sm",
                        span { class: "text-muted-foreground",
                            match hint.clone() {
                                Some(Some(h)) => rsx! { "API-Token (gesetzt: {h})" },
                                _ => rsx! { "API-Token (noch nicht gesetzt)" },
                            }
                        }
                        input {
                            class: "{input_class()} font-mono",
                            r#type: "password",
                            autocomplete: "off",
                            value: "{token}",
                            placeholder: if connected { "leer lassen = unverändert" } else { "nrznews_…" },
                            oninput: move |e| token.set(e.value()),
                        }
                    }
                    div { class: "flex gap-2",
                        Button { button_type: "submit", size: ButtonSize::Sm, class: "h-9", disabled: busy(),
                            if busy() { "Prüfe…" } else { "Speichern & testen" }
                        }
                        if connected {
                            Button { button_type: "button", size: ButtonSize::Sm, variant: ButtonVariant::Outline, class: "h-9", disabled: busy(),
                                onclick: move |_| submit(true),
                                "Trennen"
                            }
                        }
                    }
                }
                match result() {
                    Some(Ok(m)) => rsx! { p { class: "mt-3 text-sm text-success", "{m}" } },
                    Some(Err(e)) => rsx! { div { class: "mt-3", ErrorBox { message: e } } },
                    None => rsx! {},
                }
            }
        }
    }
}
