//! Create/edit dialog for a monitor (fields depend on the type).

use dioxus::prelude::*;
use icons::Save;

use super::parts::{Check, Dialog, Field, origin, select_class};
use crate::api::monitor::{Monitor, MonitorKind, monitor_app_choices, save_monitor};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::widgets::{ErrorBox, input_class};
use crate::utils::err_msg;

const METHODS: [&str; 7] = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];

#[component]
pub fn MonitorForm(
    initial: Monitor,
    onclose: EventHandler<()>,
    onsaved: EventHandler<String>,
) -> Element {
    let is_new = initial.id.is_empty();
    let mut m = use_signal(|| initial.clone());
    let mut error = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);
    let apps = use_resource(monitor_app_choices);

    let kind = m.read().kind;
    let cur = m.read().clone();
    let app_list: Vec<(String, String)> = match &*apps.read() {
        Some(Ok(l)) => l.clone(),
        _ => Vec::new(),
    };

    let submit = move |e: FormEvent| {
        e.prevent_default();
        let monitor = m.read().clone();
        spawn(async move {
            saving.set(true);
            match save_monitor(monitor).await {
                Ok(id) => onsaved.call(id),
                Err(e) => error.set(Some(err_msg(&e))),
            }
            saving.set(false);
        });
    };

    rsx! {
        Dialog { title: if is_new { "Neuer Monitor" } else { "Monitor bearbeiten" }, onclose,
            form { class: "flex flex-col gap-4", onsubmit: submit,
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    Field { label: "Typ",
                        select {
                            class: select_class(),
                            onchange: move |e| m.write().kind = MonitorKind::parse(&e.value()),
                            for k in MonitorKind::ALL {
                                option { value: k.key(), selected: k == kind, "{k.label()}" }
                            }
                        }
                    }
                    Field { label: "Name",
                        input {
                            class: input_class(),
                            value: "{cur.name}",
                            placeholder: "z.B. Nextcloud",
                            oninput: move |e| m.write().name = e.value(),
                        }
                    }
                }

                if kind != MonitorKind::Push {
                    Field { label: kind.target_label(),
                        input {
                            class: input_class(),
                            value: "{cur.target}",
                            placeholder: kind.placeholder(),
                            list: if kind == MonitorKind::App { "nrz-umbrel-apps" } else { "" },
                            autocapitalize: "off",
                            spellcheck: "false",
                            oninput: move |e| m.write().target = e.value(),
                        }
                        if kind == MonitorKind::App {
                            datalist { id: "nrz-umbrel-apps",
                                for (id, name) in app_list.iter() {
                                    option { value: "{id}", "{name}" }
                                }
                            }
                        }
                    }
                }

                match kind {
                    MonitorKind::Http => rsx! {
                        div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                            Field { label: "Methode",
                                select {
                                    class: select_class(),
                                    onchange: move |e| m.write().method = e.value(),
                                    for meth in METHODS {
                                        option { value: meth, selected: cur.method == meth, "{meth}" }
                                    }
                                }
                            }
                            Field { label: "Erwartete Statuscodes", hint: "z.B. 200-299, 301",
                                input {
                                    class: input_class(),
                                    value: "{cur.accepted_codes}",
                                    oninput: move |e| m.write().accepted_codes = e.value(),
                                }
                            }
                        }
                        Field { label: "Keyword (optional)", hint: "Gross-/Kleinschreibung wird beachtet",
                            input {
                                class: input_class(),
                                value: "{cur.keyword}",
                                placeholder: "Text im Antworttext",
                                oninput: move |e| m.write().keyword = e.value(),
                            }
                        }
                        div { class: "flex flex-wrap gap-x-6 gap-y-2",
                            Check { label: "Keyword darf nicht enthalten sein", checked: cur.keyword_invert, onchange: move |v| m.write().keyword_invert = v }
                            Check { label: "TLS-Fehler ignorieren", checked: cur.ignore_tls, onchange: move |v| m.write().ignore_tls = v }
                        }
                    },
                    MonitorKind::Dns => rsx! {
                        div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                            Field { label: "Eintrag",
                                select {
                                    class: select_class(),
                                    onchange: move |e| m.write().dns_record = e.value(),
                                    for (v, l) in [("", "A oder AAAA"), ("A", "A (IPv4)"), ("AAAA", "AAAA (IPv6)")] {
                                        option { value: v, selected: cur.dns_record == v, "{l}" }
                                    }
                                }
                            }
                            Field { label: "Erwartete IP (optional)",
                                input {
                                    class: input_class(),
                                    value: "{cur.dns_expected}",
                                    placeholder: "z.B. 93.184.216.34",
                                    oninput: move |e| m.write().dns_expected = e.value(),
                                }
                            }
                        }
                    },
                    MonitorKind::Push => rsx! {
                        div { class: "rounded-lg border bg-muted/30 p-3 text-sm text-muted-foreground break-all",
                            if cur.push_token.is_empty() {
                                "Nach dem Speichern wird eine geheime Push-URL erzeugt. Der Monitor ist down, wenn sie nicht innerhalb des Intervalls aufgerufen wird."
                            } else {
                                "Push-URL: {origin()}/monitors/push/{cur.push_token}"
                            }
                        }
                    },
                    MonitorKind::App => rsx! {
                        p { class: "text-xs text-muted-foreground", "Up, wenn umbrelOS die App als „ready“ bzw. „running“ meldet." }
                    },
                    _ => rsx! {},
                }

                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    NumField { label: "Intervall (s)", hint: "mind. 20", value: cur.interval, oninput: move |v| m.write().interval = v }
                    if kind != MonitorKind::Push {
                        NumField { label: "Zeitlimit (s)", value: cur.timeout, oninput: move |v| m.write().timeout = v }
                    }
                    NumField { label: "Wiederholungen bis down", hint: "0 = sofort down", value: cur.retries, oninput: move |v| m.write().retries = v }
                    NumField { label: "Wiederholungsintervall (s)", value: cur.retry_interval, oninput: move |v| m.write().retry_interval = v }
                }

                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    Field { label: "Gruppe (optional)",
                        input {
                            class: input_class(),
                            value: "{cur.group}",
                            placeholder: "z.B. Netzwerk",
                            oninput: move |e| m.write().group = e.value(),
                        }
                    }
                    div { class: "flex flex-col justify-end gap-2 pb-1",
                        Check { label: "Benachrichtigungen bei Statuswechsel", checked: cur.notify, onchange: move |v| m.write().notify = v }
                        Check { label: "Pausiert", checked: cur.paused, onchange: move |v| m.write().paused = v }
                    }
                }
                Field { label: "Beschreibung (optional)",
                    textarea {
                        class: "{input_class()} h-20 py-2",
                        value: "{cur.description}",
                        oninput: move |e| m.write().description = e.value(),
                    }
                }

                if let Some(e) = error() {
                    ErrorBox { message: e }
                }
                div { class: "flex justify-end gap-2",
                    Button { variant: ButtonVariant::Ghost, button_type: "button", onclick: move |_| onclose.call(()), "Abbrechen" }
                    Button { button_type: "submit", disabled: saving(),
                        Save { class: "size-4" }
                        if saving() { "Speichert …" } else { "Speichern" }
                    }
                }
            }
        }
    }
}

#[component]
fn NumField(
    label: String,
    #[props(into, default)] hint: String,
    value: u32,
    oninput: EventHandler<u32>,
) -> Element {
    rsx! {
        Field { label, hint,
            input {
                class: input_class(),
                r#type: "number",
                inputmode: "numeric",
                min: "0",
                value: "{value}",
                oninput: move |e| {
                    if let Ok(v) = e.value().trim().parse::<u32>() {
                        oninput.call(v);
                    }
                },
            }
        }
    }
}
