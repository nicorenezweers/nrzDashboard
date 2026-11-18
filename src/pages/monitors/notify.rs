//! Global notification channels (ntfy, Discord, Telegram, webhook) with "Test senden".

use dioxus::prelude::*;
use icons::{Pencil, Plus, Save, Send, Trash2};

use super::parts::{Check, Dialog, Field, select_class};
use crate::api::monitor::{
    NotifyChannel, NotifyKind, delete_notify_channel, notify_channels, save_notify_channel,
    test_notify_channel,
};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::widgets::{ConfirmButton, ErrorBox, Notice, input_class};
use crate::utils::err_msg;

fn new_channel() -> NotifyChannel {
    NotifyChannel {
        enabled: true,
        url: "https://ntfy.sh".into(),
        ..NotifyChannel::default()
    }
}

#[component]
pub fn NotifySettings(onclose: EventHandler<()>) -> Element {
    let mut list = use_resource(notify_channels);
    let mut editing = use_signal(|| None::<NotifyChannel>);
    let mut info = use_signal(|| None::<Result<String, String>>);

    let test = move |c: NotifyChannel| {
        spawn(async move {
            info.set(Some(Ok("Sende Testnachricht …".into())));
            info.set(Some(test_notify_channel(c).await.map_err(|e| err_msg(&e))));
        });
    };

    rsx! {
        Dialog { title: "Benachrichtigungen", onclose,
            div { class: "flex flex-col gap-4",
                p { class: "text-sm text-muted-foreground",
                    "Bei jedem Statuswechsel (up ↔ down) eines Monitors mit aktivierten Benachrichtigungen wird an alle aktiven Kanäle gesendet."
                }
                match info() {
                    Some(Ok(msg)) => rsx! { Notice { message: msg } },
                    Some(Err(e)) => rsx! { ErrorBox { message: e } },
                    None => rsx! {},
                }

                if let Some(c) = editing() {
                    ChannelForm {
                        key: "{c.id}",
                        initial: c,
                        oncancel: move |_| editing.set(None),
                        onsaved: move |_| {
                            editing.set(None);
                            info.set(Some(Ok("Gespeichert".into())));
                            list.restart();
                        },
                        ontest: test,
                    }
                } else {
                    match &*list.read() {
                        None => rsx! { p { class: "text-sm text-muted-foreground", "Lädt …" } },
                        Some(Err(e)) => rsx! { ErrorBox { message: err_msg(e) } },
                        Some(Ok(channels)) if channels.is_empty() => rsx! {
                            div { class: "rounded-lg border border-dashed p-6 text-center text-sm text-muted-foreground", "Noch keine Kanäle eingerichtet." }
                        },
                        Some(Ok(channels)) => rsx! {
                            div { class: "flex flex-col divide-y rounded-lg border",
                                for c in channels.clone() {
                                    ChannelRow {
                                        key: "{c.id}",
                                        channel: c,
                                        onedit: move |c| editing.set(Some(c)),
                                        ontest: test,
                                        ondeleted: move |_| list.restart(),
                                        onerror: move |e| info.set(Some(Err(e))),
                                    }
                                }
                            }
                        },
                    }
                    Button {
                        class: "self-start",
                        size: ButtonSize::Sm,
                        onclick: move |_| {
                            info.set(None);
                            editing.set(Some(new_channel()));
                        },
                        Plus { class: "size-4" }
                        "Kanal hinzufügen"
                    }
                }
            }
        }
    }
}

#[component]
fn ChannelRow(
    channel: NotifyChannel,
    onedit: EventHandler<NotifyChannel>,
    ontest: EventHandler<NotifyChannel>,
    ondeleted: EventHandler<()>,
    onerror: EventHandler<String>,
) -> Element {
    let c = channel;
    let (c_edit, c_test, id) = (c.clone(), c.clone(), c.id.clone());
    let detail = match c.kind {
        NotifyKind::Ntfy => format!("{}/{}", c.url, c.topic),
        NotifyKind::Telegram => format!("Chat {}", c.chat_id),
        _ => c.url.clone(),
    };
    rsx! {
        div { class: "flex flex-wrap items-center gap-3 p-3",
            div { class: "min-w-0 flex-1",
                div { class: "flex items-center gap-2",
                    span { class: "truncate font-medium", "{c.name}" }
                    span { class: "rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground", "{c.kind.label()}" }
                    if !c.enabled {
                        span { class: "rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground", "aus" }
                    }
                }
                div { class: "truncate text-xs text-muted-foreground", "{detail}" }
            }
            div { class: "flex gap-1",
                Button {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    onclick: move |_| ontest.call(c_test.clone()),
                    Send { class: "size-4" }
                    span { class: "hidden sm:inline", "Test senden" }
                }
                Button {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::IconSm,
                    aria_label: "Bearbeiten",
                    onclick: move |_| onedit.call(c_edit.clone()),
                    Pencil { class: "size-4" }
                }
                ConfirmButton {
                    label: "",
                    confirm_label: "Löschen?",
                    onconfirm: move |()| {
                        let id = id.clone();
                        spawn(async move {
                            match delete_notify_channel(id).await {
                                Ok(()) => ondeleted.call(()),
                                Err(e) => onerror.call(err_msg(&e)),
                            }
                        });
                    },
                    Trash2 { class: "size-4" }
                }
            }
        }
    }
}

#[component]
fn ChannelForm(
    initial: NotifyChannel,
    oncancel: EventHandler<()>,
    onsaved: EventHandler<()>,
    ontest: EventHandler<NotifyChannel>,
) -> Element {
    let mut c = use_signal(|| initial.clone());
    let mut error = use_signal(|| None::<String>);
    let cur = c.read().clone();
    let kind = cur.kind;
    let secret_hint = if cur.id.is_empty() {
        ""
    } else {
        "Maskiert – zum Ändern neu eingeben"
    };

    let save = move |e: FormEvent| {
        e.prevent_default();
        let ch = c.read().clone();
        spawn(async move {
            match save_notify_channel(ch).await {
                Ok(_) => onsaved.call(()),
                Err(e) => error.set(Some(err_msg(&e))),
            }
        });
    };

    rsx! {
        form { class: "flex flex-col gap-4 rounded-lg border p-3", onsubmit: save,
            div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                Field { label: "Art",
                    select {
                        class: select_class(),
                        onchange: move |e| {
                            let k = NotifyKind::parse(&e.value());
                            let mut w = c.write();
                            if w.kind == NotifyKind::Ntfy && w.url == "https://ntfy.sh" {
                                w.url.clear();
                            }
                            if k == NotifyKind::Ntfy && w.url.is_empty() {
                                w.url = "https://ntfy.sh".into();
                            }
                            w.kind = k;
                        },
                        for k in NotifyKind::ALL {
                            option { value: k.key(), selected: k == kind, "{k.label()}" }
                        }
                    }
                }
                Field { label: "Name",
                    input { class: input_class(), value: "{cur.name}", placeholder: kind.label(), oninput: move |e| c.write().name = e.value() }
                }
            }
            match kind {
                NotifyKind::Ntfy => rsx! {
                    div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                        Field { label: "Server-URL",
                            input { class: input_class(), value: "{cur.url}", placeholder: "https://ntfy.sh", oninput: move |e| c.write().url = e.value() }
                        }
                        Field { label: "Topic",
                            input { class: input_class(), value: "{cur.topic}", placeholder: "mein-umbrel", oninput: move |e| c.write().topic = e.value() }
                        }
                    }
                    Field { label: "Zugriffstoken (optional)", hint: secret_hint,
                        input { class: input_class(), value: "{cur.token}", autocomplete: "off", oninput: move |e| c.write().token = e.value() }
                    }
                },
                NotifyKind::Discord => rsx! {
                    Field { label: "Webhook-URL", hint: secret_hint,
                        input { class: input_class(), value: "{cur.url}", placeholder: "https://discord.com/api/webhooks/…", autocomplete: "off", oninput: move |e| c.write().url = e.value() }
                    }
                },
                NotifyKind::Telegram => rsx! {
                    div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                        Field { label: "Bot-Token", hint: secret_hint,
                            input { class: input_class(), value: "{cur.token}", placeholder: "123456:ABC…", autocomplete: "off", oninput: move |e| c.write().token = e.value() }
                        }
                        Field { label: "Chat-ID",
                            input { class: input_class(), value: "{cur.chat_id}", placeholder: "-100123…", oninput: move |e| c.write().chat_id = e.value() }
                        }
                    }
                },
                NotifyKind::Webhook => rsx! {
                    Field { label: "URL", hint: "JSON-POST mit title, message, status, monitor, heartbeat",
                        input { class: input_class(), value: "{cur.url}", placeholder: "https://example.com/hook", oninput: move |e| c.write().url = e.value() }
                    }
                },
            }
            Check { label: "Aktiv", checked: cur.enabled, onchange: move |v| c.write().enabled = v }
            if let Some(e) = error() {
                ErrorBox { message: e }
            }
            div { class: "flex flex-wrap justify-end gap-2",
                Button { variant: ButtonVariant::Ghost, button_type: "button", onclick: move |_| oncancel.call(()), "Abbrechen" }
                Button {
                    variant: ButtonVariant::Outline,
                    button_type: "button",
                    onclick: move |_| ontest.call(c.read().clone()),
                    Send { class: "size-4" }
                    "Test senden"
                }
                Button { button_type: "submit",
                    Save { class: "size-4" }
                    "Speichern"
                }
            }
        }
    }
}
