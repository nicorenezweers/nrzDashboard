//! Per-widget settings dialog.

use dioxus::prelude::*;
use icons::{Search, X};
use serde_json::{Value, json};

use crate::api::board::{Place, Widget, WidgetKind, app_tiles, geocode};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::widgets::input_class;
use crate::utils::err_msg;

use super::widgets::ENGINES;

fn set(opts: &mut Signal<Value>, key: &str, v: Value) {
    let mut o = opts.write();
    if !o.is_object() {
        *o = json!({});
    }
    if let Some(m) = o.as_object_mut() {
        m.insert(key.to_string(), v);
    }
}

fn get_str(opts: &Signal<Value>, key: &str) -> String {
    opts.read().get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn get_bool(opts: &Signal<Value>, key: &str, default: bool) -> bool {
    opts.read().get(key).and_then(Value::as_bool).unwrap_or(default)
}

#[component]
pub fn WidgetSettings(widget: Widget, onclose: EventHandler<()>, onsave: EventHandler<Widget>) -> Element {
    let mut title = use_signal(|| widget.title.clone());
    let mut opts = use_signal(|| widget.options.clone());
    let kind = widget.kind;

    let save = {
        let widget = widget.clone();
        move |e: FormEvent| {
            e.prevent_default();
            let mut w = widget.clone();
            w.title = title().trim().to_string();
            w.options = opts();
            onsave.call(w);
        }
    };

    rsx! {
        div { class: "fixed inset-0 z-[70] flex items-end sm:items-center justify-center bg-black/60 p-0 sm:p-4", onclick: move |_| onclose.call(()),
            form {
                class: "flex w-full sm:max-w-lg max-h-[90dvh] flex-col gap-4 overflow-y-auto rounded-t-2xl sm:rounded-2xl border bg-card p-4 sm:p-6 pb-[calc(env(safe-area-inset-bottom,0px)+1rem)]",
                onclick: move |e| e.stop_propagation(),
                onsubmit: save,
                div { class: "flex items-center justify-between",
                    h2 { class: "text-lg font-semibold", "{kind.label()}" }
                    button { r#type: "button", class: "p-2 text-muted-foreground hover:text-foreground", onclick: move |_| onclose.call(()), X { class: "size-4" } }
                }
                Field { label: "Titel (optional)",
                    input { class: input_class(), value: "{title}", oninput: move |e| title.set(e.value()) }
                }

                match kind {
                    WidgetKind::App => rsx! { AppFields { opts } },
                    WidgetKind::Apps => rsx! {
                        Toggle { label: "Gestoppte Apps anzeigen", checked: get_bool(&opts, "stopped", true), onchange: move |v| set(&mut opts, "stopped", json!(v)) }
                        HiddenApps {}
                        p { class: "text-xs text-muted-foreground", "Tipp: Rechtsklick auf eine App zum Anpassen, Ausblenden, Starten/Stoppen und für Logs." }
                    },
                    WidgetKind::Dns => rsx! {
                        Field { label: "Zeitraum",
                            select {
                                class: input_class(),
                                onchange: move |e| set(&mut opts, "range", json!(e.value())),
                                for (key, name) in [("hour", "Letzte Stunde"), ("day", "24 Stunden"), ("week", "7 Tage"), ("month", "30 Tage")] {
                                    option { value: "{key}", selected: get_str(&opts, "range") == key || (key == "day" && get_str(&opts, "range").is_empty()), "{name}" }
                                }
                            }
                        }
                    },
                    WidgetKind::Finance => rsx! { FinanceFields { opts } },
                    WidgetKind::Clock => rsx! {
                        Toggle { label: "Sekunden anzeigen", checked: get_bool(&opts, "seconds", false), onchange: move |v| set(&mut opts, "seconds", json!(v)) }
                        Toggle { label: "Datum anzeigen", checked: get_bool(&opts, "date", true), onchange: move |v| set(&mut opts, "date", json!(v)) }
                    },
                    WidgetKind::Weather => rsx! { WeatherFields { opts } },
                    WidgetKind::News => rsx! {
                        Field { label: "Schlagzeilen (neueste ungelesene, 0 = keine)",
                            input {
                                class: input_class(),
                                r#type: "number",
                                min: "0",
                                max: "20",
                                value: "{opts.read().get(\"headlines\").and_then(Value::as_u64).unwrap_or(5)}",
                                oninput: move |e| {
                                    if let Ok(n) = e.value().trim().parse::<u64>() {
                                        set(&mut opts, "headlines", json!(n.min(20)));
                                    }
                                },
                            }
                        }
                        Toggle { label: "Kennzahlen anzeigen", checked: get_bool(&opts, "metrics", true), onchange: move |v| set(&mut opts, "metrics", json!(v)) }
                        p { class: "text-xs text-muted-foreground", "Die Verbindung zu nrzNews (Adresse und API-Token) wird unter System → nrzNews eingerichtet." }
                    },
                    WidgetKind::Notes => rsx! {
                        Field { label: "Text (Markdown)",
                            textarea {
                                class: "min-h-48 w-full rounded-md border border-input dark:bg-input/30 bg-transparent p-3 font-mono text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                                value: "{get_str(&opts, \"text\")}",
                                oninput: move |e| set(&mut opts, "text", json!(e.value())),
                            }
                        }
                    },
                    WidgetKind::Bookmarks => rsx! {
                        Field { label: "Links – eine pro Zeile: Titel | https://…",
                            textarea {
                                class: "min-h-40 w-full rounded-md border border-input dark:bg-input/30 bg-transparent p-3 font-mono text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                                value: "{get_str(&opts, \"items\")}",
                                placeholder: "nrzGit | http://umbrel.local:8390",
                                oninput: move |e| set(&mut opts, "items", json!(e.value())),
                            }
                        }
                    },
                    WidgetKind::Search => rsx! {
                        Field { label: "Suchmaschine",
                            select {
                                class: input_class(),
                                onchange: move |e| set(&mut opts, "engine", json!(e.value())),
                                for (key, name, _) in ENGINES {
                                    option { value: "{key}", selected: get_str(&opts, "engine") == key, "{name}" }
                                }
                            }
                        }
                    },
                    WidgetKind::Iframe => rsx! {
                        Field { label: "URL",
                            input {
                                class: "{input_class()} font-mono",
                                value: "{get_str(&opts, \"url\")}",
                                placeholder: "https://…",
                                oninput: move |e| set(&mut opts, "url", json!(e.value().trim())),
                            }
                        }
                        p { class: "text-xs text-muted-foreground", "Manche Seiten verbieten das Einbetten (X-Frame-Options) und bleiben dann leer." }
                    },
                    WidgetKind::System | WidgetKind::Monitors => rsx! {},
                }

                div { class: "flex justify-end gap-2 pt-2",
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, onclick: move |_| onclose.call(()), "Abbrechen" }
                    Button { button_type: "submit", size: ButtonSize::Sm, "Speichern" }
                }
            }
        }
    }
}

#[component]
fn Field(label: String, children: Element) -> Element {
    rsx! {
        label { class: "flex flex-col gap-1.5 text-sm",
            span { class: "text-muted-foreground", "{label}" }
            {children}
        }
    }
}

#[component]
fn Toggle(label: String, checked: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "flex items-center justify-between gap-3 text-sm",
            span { "{label}" }
            input { r#type: "checkbox", class: "size-4 accent-current", checked, onchange: move |e| onchange.call(e.checked()) }
        }
    }
}

#[component]
fn AppFields(opts: Signal<Value>) -> Element {
    let tiles = use_resource(app_tiles);
    let current = get_str(&opts, "app");
    rsx! {
        Field { label: "Umbrel-App",
            select {
                class: input_class(),
                onchange: move |e| set(&mut opts, "app", json!(e.value())),
                option { value: "", selected: current.is_empty(), "– eigener Link –" }
                if let Some(Ok(list)) = tiles.read().clone() {
                    for t in list {
                        option { value: "{t.id}", selected: current == t.id, "{t.name}" }
                    }
                }
            }
        }
        Field { label: "Name (optional, überschreibt den App-Namen)",
            input { class: input_class(), value: "{get_str(&opts, \"name\")}", oninput: move |e| set(&mut opts, "name", json!(e.value())) }
        }
        Field { label: "URL (für eigenen Link oder abweichende Adresse)",
            input { class: "{input_class()} font-mono", value: "{get_str(&opts, \"url\")}", placeholder: "https://…", oninput: move |e| set(&mut opts, "url", json!(e.value().trim())) }
        }
        Field { label: "Icon-URL (optional)",
            input { class: "{input_class()} font-mono", value: "{get_str(&opts, \"icon\")}", placeholder: "https://…/icon.png", oninput: move |e| set(&mut opts, "icon", json!(e.value().trim())) }
        }
    }
}

#[component]
fn WeatherFields(opts: Signal<Value>) -> Element {
    let mut query = use_signal(String::new);
    let mut results = use_signal(|| None::<Result<Vec<Place>, String>>);
    let city = get_str(&opts, "city");
    rsx! {
        p { class: "text-sm", "Ort: ", span { class: "font-medium", "{city}" } }
        div { class: "flex gap-2",
            input {
                class: input_class(),
                placeholder: "Ort suchen, z.B. Bern",
                value: "{query}",
                oninput: move |e| query.set(e.value()),
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Enter {
                        e.prevent_default();
                        let q = query();
                        spawn(async move { results.set(Some(geocode(q).await.map_err(|e| err_msg(&e)))) });
                    }
                },
            }
            Button {
                size: ButtonSize::Sm,
                variant: ButtonVariant::Outline,
                class: "h-9",
                onclick: move |_| {
                    let q = query();
                    spawn(async move { results.set(Some(geocode(q).await.map_err(|e| err_msg(&e)))) });
                },
                Search { class: "size-4" }
            }
        }
        match results() {
            Some(Err(e)) => rsx! { p { class: "text-sm text-destructive", "{e}" } },
            Some(Ok(list)) => rsx! {
                div { class: "flex flex-col gap-1",
                    if list.is_empty() {
                        p { class: "text-sm text-muted-foreground", "Nichts gefunden." }
                    }
                    for p in list {
                        button {
                            r#type: "button",
                            class: "rounded-md px-2 py-1.5 text-left text-sm hover:bg-accent/60",
                            onclick: {
                                let p = p.clone();
                                move |_| {
                                    set(&mut opts, "city", json!(p.name));
                                    set(&mut opts, "lat", json!(p.lat));
                                    set(&mut opts, "lon", json!(p.lon));
                                    results.set(None);
                                }
                            },
                            span { class: "font-medium", "{p.name}" }
                            span { class: "text-muted-foreground", " · {p.region}" }
                        }
                    }
                }
            },
            None => rsx! {},
        }
    }
}

#[component]
fn HiddenApps() -> Element {
    use crate::api::board::{app_override_get, app_override_set};
    let mut tiles = use_resource(app_tiles);
    let hidden: Vec<(String, String)> = tiles
        .read()
        .clone()
        .and_then(Result::ok)
        .map(|l| l.into_iter().filter(|t| t.hidden).map(|t| (t.id, t.name)).collect())
        .unwrap_or_default();
    if hidden.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "flex flex-col gap-1.5 text-sm",
            span { class: "text-muted-foreground", "Ausgeblendete Apps" }
            for (id, name) in hidden {
                div { key: "{id}", class: "flex items-center justify-between gap-2 rounded-md bg-muted/40 px-2.5 py-1.5",
                    span { class: "truncate", "{name}" }
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Ghost,
                        class: "h-7",
                        onclick: move |_| {
                            let id = id.clone();
                            spawn(async move {
                                let mut ov = app_override_get(id.clone()).await.unwrap_or_default();
                                ov.hidden = false;
                                let _ = app_override_set(id, ov).await;
                                tiles.restart();
                            });
                        },
                        "Einblenden"
                    }
                }
            }
        }
    }
}

#[component]
fn FinanceFields(opts: Signal<Value>) -> Element {
    let data = use_resource(crate::api::integrations::budget_overview);
    let current = get_str(&opts, "organization");
    rsx! {
        Field { label: "Organisation (nrzBudget)",
            select {
                class: input_class(),
                onchange: move |e| set(&mut opts, "organization", json!(e.value())),
                option { value: "", selected: current.is_empty(), "Erste Organisation" }
                if let Some(Ok(o)) = data.read().clone() {
                    for g in o.organizations {
                        option { value: "{g.name}", selected: current == g.name, "{g.name} ({g.currency})" }
                    }
                }
            }
        }
        if let Some(Err(e)) = data.read().clone() {
            p { class: "text-xs text-destructive", {err_msg(&e)} }
        }
    }
}
