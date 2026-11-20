//! Settings – only the essentials: name, app links (user/admin), password, 2FA,
//! hidden apps and the two connections (nrzNetwork for the domains, umbrelOS).

use dioxus::prelude::*;
use icons::{Check, Eye, Globe, KeyRound, Link, ShieldCheck, User};

use super::{AppIcon, HomeCtx, HomeWindow};
use crate::api::apps::app_set_hidden;
use crate::api::settings::{
    SettingsView, TotpSetup, password_change, settings_save_general, settings_save_monitor, settings_save_network, settings_save_umbrel, totp_begin, totp_disable,
    totp_enable,
};
use crate::utils::err_msg;

const INPUT: &str = "h-11 w-full rounded-xl border border-white/10 bg-white/[0.06] px-3.5 text-[15px] text-white placeholder:text-white/35 outline-none focus:border-white/30";
const BTN: &str = "inline-flex h-10 items-center justify-center gap-2 rounded-full bg-white px-5 text-sm font-medium text-black transition hover:bg-white/90 disabled:opacity-40";
const BTN_GHOST: &str = "inline-flex h-10 items-center justify-center gap-2 rounded-full bg-white/10 px-5 text-sm text-white transition hover:bg-white/20 disabled:opacity-40";

type Msg = Signal<Option<Result<String, String>>>;

#[component]
pub fn SettingsWindow() -> Element {
    let ctx = use_context::<HomeCtx>();
    let data = ctx.settings.read().clone();
    rsx! {
        HomeWindow { title: "Einstellungen", narrow: true,
            match data {
                None => rsx! { div { class: "h-64 animate-pulse rounded-3xl bg-white/[0.05]" } },
                Some(Err(e)) => rsx! { p { class: "text-sm text-red-300", {err_msg(&e)} } },
                Some(Ok(s)) => rsx! {
                    div { class: "flex flex-col gap-4",
                        General { key: "{s.name}{s.link_mode}", s: s.clone() }
                        Security { s: s.clone() }
                        Hidden { s: s.clone() }
                        Connections { s }
                    }
                },
            }
        }
    }
}

#[component]
fn Section(icon: Element, title: String, #[props(default)] hint: String, children: Element) -> Element {
    rsx! {
        section { class: "rounded-3xl bg-white/[0.05] p-5 md:p-6",
            h3 { class: "flex items-center gap-2.5 text-lg font-semibold [&_svg]:size-5 [&_svg]:text-white/60", {icon} "{title}" }
            if !hint.is_empty() {
                p { class: "mt-1 text-sm text-white/50", "{hint}" }
            }
            div { class: "mt-4", {children} }
        }
    }
}

#[component]
fn Feedback(msg: Option<Result<String, String>>) -> Element {
    rsx! {
        match msg {
            Some(Ok(m)) => rsx! { p { class: "mt-3 flex items-center gap-2 text-sm text-emerald-300", Check { class: "size-4" } "{m}" } },
            Some(Err(m)) => rsx! { p { class: "mt-3 text-sm text-red-300", "{m}" } },
            None => rsx! {},
        }
    }
}

#[component]
fn General(s: SettingsView) -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut name = use_signal(|| s.name.clone());
    let mut mode = use_signal(|| s.link_mode.clone());
    let mut msg: Msg = use_signal(|| None);
    let mut save = move |m: String| {
        mode.set(m.clone());
        spawn(async move {
            let r = settings_save_general(name(), m).await;
            msg.set(Some(r.map(|_| "Gespeichert".to_string()).map_err(|e| err_msg(&e))));
            ctx.reload();
        });
    };
    let option = |key: &'static str, title: &'static str, example: &'static str, icon: Element| {
        let active = mode() == key;
        rsx! {
            button {
                class: if active { "flex flex-1 items-start gap-3 rounded-2xl border border-white/60 bg-white/10 p-4 text-left" } else { "flex flex-1 items-start gap-3 rounded-2xl border border-white/10 p-4 text-left hover:bg-white/[0.06]" },
                onclick: move |_| save(key.to_string()),
                span { class: "mt-0.5 text-white/70 [&_svg]:size-5", {icon} }
                span { class: "flex flex-col",
                    span { class: "font-medium", "{title}" }
                    span { class: "text-sm text-white/50", "{example}" }
                }
                if active { Check { class: "ml-auto size-5 shrink-0 text-white" } }
            }
        }
    };
    rsx! {
        Section { icon: rsx! { User {} }, title: "Allgemein",
            label { class: "mb-1.5 block text-sm text-white/60", "Dein Name für die Begrüßung" }
            div { class: "flex gap-2",
                input { class: INPUT, value: "{name}", maxlength: "40", oninput: move |e| name.set(e.value()) }
                button { class: BTN, disabled: name() == s.name, onclick: move |_| save(mode()), "Sichern" }
            }
            label { class: "mb-1.5 mt-5 block text-sm text-white/60", "Apps öffnen als" }
            div { class: "flex flex-col gap-2 sm:flex-row",
                {option("user", "Benutzer", "bench.zweers.ch", rsx! { Globe {} })}
                {option("admin", "Admin", "admin.bench.zweers.ch", rsx! { ShieldCheck {} })}
            }
            p { class: "mt-2 text-xs text-white/40", "Die andere Adresse findest du jederzeit per Rechtsklick (Handy: lange drücken) auf eine App." }
            Feedback { msg: msg() }
        }
    }
}

#[component]
fn Security(s: SettingsView) -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut current = use_signal(String::new);
    let mut new = use_signal(String::new);
    let mut repeat = use_signal(String::new);
    let mut pw_msg: Msg = use_signal(|| None);
    let mut setup = use_signal(|| None::<TotpSetup>);
    let mut code = use_signal(String::new);
    let mut otp_msg: Msg = use_signal(|| None);
    let mut disabling = use_signal(|| false);

    let pw_ok = !current().is_empty() && new().chars().count() >= 10 && new() == repeat();
    let code_ok = code().chars().filter(char::is_ascii_digit).count() == 6;

    rsx! {
        Section { icon: rsx! { KeyRound {} }, title: "Anmeldung", hint: "Benutzer: {s.user}",
            // Password
            div { class: "grid gap-2 sm:grid-cols-3",
                input { class: INPUT, r#type: "password", placeholder: "Aktuelles Passwort", autocomplete: "current-password", value: "{current}", oninput: move |e| current.set(e.value()) }
                input { class: INPUT, r#type: "password", placeholder: "Neues Passwort", autocomplete: "new-password", value: "{new}", oninput: move |e| new.set(e.value()) }
                input { class: INPUT, r#type: "password", placeholder: "Wiederholen", autocomplete: "new-password", value: "{repeat}", oninput: move |e| repeat.set(e.value()) }
            }
            div { class: "mt-2 flex items-center gap-3",
                button {
                    class: BTN,
                    disabled: !pw_ok,
                    onclick: move |_| {
                        spawn(async move {
                            let r = password_change(current(), new()).await;
                            if r.is_ok() {
                                current.set(String::new());
                                new.set(String::new());
                                repeat.set(String::new());
                            }
                            pw_msg.set(Some(r.map(|_| "Passwort geändert".to_string()).map_err(|e| err_msg(&e))));
                        });
                    },
                    "Passwort ändern"
                }
                if !new().is_empty() && new().chars().count() < 10 {
                    span { class: "text-xs text-white/40", "mindestens 10 Zeichen" }
                } else if !repeat().is_empty() && new() != repeat() {
                    span { class: "text-xs text-amber-300", "stimmt nicht überein" }
                }
            }
            Feedback { msg: pw_msg() }

            div { class: "my-5 h-px bg-white/10" }

            // Two-factor
            div { class: "flex items-center justify-between gap-3",
                div {
                    div { class: "flex items-center gap-2 font-medium",
                        ShieldCheck { class: "size-4 text-white/60" }
                        "Zwei-Faktor-Anmeldung"
                    }
                    p { class: "text-sm text-white/50",
                        if s.totp_enabled { "Aktiv – bei jeder Anmeldung wird ein Code aus deiner Authenticator-App abgefragt." } else { "Aus – schütze den Zugang zusätzlich mit einer Authenticator-App." }
                    }
                }
                if s.totp_enabled {
                    if !disabling() {
                        button { class: BTN_GHOST, onclick: move |_| { disabling.set(true); code.set(String::new()); otp_msg.set(None); }, "Ausschalten" }
                    }
                } else if setup().is_none() {
                    button {
                        class: BTN,
                        onclick: move |_| {
                            spawn(async move {
                                match totp_begin().await {
                                    Ok(t) => { setup.set(Some(t)); code.set(String::new()); otp_msg.set(None); }
                                    Err(e) => otp_msg.set(Some(Err(err_msg(&e)))),
                                }
                            });
                        },
                        "Einrichten"
                    }
                }
            }
            if let Some(t) = setup() {
                div { class: "mt-4 flex flex-col gap-4 rounded-2xl bg-black/25 p-4 sm:flex-row sm:items-center",
                    div { class: "size-44 shrink-0 overflow-hidden rounded-xl bg-white p-2 [&_svg]:h-full [&_svg]:w-full", dangerous_inner_html: "{t.qr_svg}" }
                    div { class: "flex min-w-0 flex-col gap-2",
                        p { class: "text-sm text-white/70", "1. QR-Code mit einer Authenticator-App scannen (z. B. Apple Passwörter, Google Authenticator, 1Password)." }
                        p { class: "text-xs text-white/40 break-all", "Oder Schlüssel eingeben: {t.secret}" }
                        p { class: "text-sm text-white/70", "2. Den 6-stelligen Code eingeben:" }
                        div { class: "flex gap-2",
                            input { class: "{INPUT} max-w-40 text-center tracking-[0.3em]", inputmode: "numeric", autocomplete: "one-time-code", maxlength: "6", placeholder: "000000", value: "{code}", oninput: move |e| code.set(e.value()) }
                            button {
                                class: BTN,
                                disabled: !code_ok,
                                onclick: move |_| {
                                    spawn(async move {
                                        match totp_enable(code()).await {
                                            Ok(()) => { setup.set(None); otp_msg.set(Some(Ok("Zwei-Faktor-Anmeldung ist aktiv".into()))); ctx.reload(); }
                                            Err(e) => otp_msg.set(Some(Err(err_msg(&e)))),
                                        }
                                    });
                                },
                                "Aktivieren"
                            }
                            button { class: BTN_GHOST, onclick: move |_| setup.set(None), "Abbrechen" }
                        }
                    }
                }
            }
            if disabling() {
                div { class: "mt-4 flex flex-wrap items-center gap-2",
                    input { class: "{INPUT} max-w-40 text-center tracking-[0.3em]", inputmode: "numeric", autocomplete: "one-time-code", maxlength: "6", placeholder: "Code", value: "{code}", oninput: move |e| code.set(e.value()) }
                    button {
                        class: BTN,
                        disabled: !code_ok,
                        onclick: move |_| {
                            spawn(async move {
                                match totp_disable(code()).await {
                                    Ok(()) => { disabling.set(false); otp_msg.set(Some(Ok("Zwei-Faktor-Anmeldung ist aus".into()))); ctx.reload(); }
                                    Err(e) => otp_msg.set(Some(Err(err_msg(&e)))),
                                }
                            });
                        },
                        "Ausschalten bestätigen"
                    }
                    button { class: BTN_GHOST, onclick: move |_| disabling.set(false), "Abbrechen" }
                }
            }
            Feedback { msg: otp_msg() }
        }
    }
}

#[component]
fn Hidden(s: SettingsView) -> Element {
    let ctx = use_context::<HomeCtx>();
    if s.hidden.is_empty() {
        return rsx! {};
    }
    let names: Vec<(String, String)> = s
        .hidden
        .iter()
        .map(|id| {
            let name = match &*ctx.tiles.read() {
                Some(Ok(t)) => t.iter().find(|t| &t.id == id).map(|t| t.name.clone()),
                _ => None,
            };
            (id.clone(), name.unwrap_or_else(|| id.clone()))
        })
        .collect();
    rsx! {
        Section { icon: rsx! { Eye {} }, title: "Ausgeblendete Apps",
            div { class: "flex flex-col gap-2",
                for (id, name) in names {
                    div { key: "{id}", class: "flex items-center gap-3",
                        AppIcon { id: id.clone(), name: name.clone(), class: "size-9" }
                        span { class: "flex-1 truncate", "{name}" }
                        button {
                            class: BTN_GHOST,
                            onclick: move |_| {
                                let id = id.clone();
                                spawn(async move {
                                    let _ = app_set_hidden(id, false).await;
                                    ctx.reload();
                                });
                            },
                            "Einblenden"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Connections(s: SettingsView) -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut net_url = use_signal(|| s.network_url.clone());
    let mut net_token = use_signal(String::new);
    let mut net_msg: Msg = use_signal(|| None);
    let mut mon_url = use_signal(|| s.monitor_url.clone());
    let mut mon_token = use_signal(String::new);
    let mut mon_msg: Msg = use_signal(|| None);
    let mut um_url = use_signal(|| s.umbrel_url.clone());
    let mut um_token = use_signal(String::new);
    let mut um_msg: Msg = use_signal(|| None);
    let open = s.network_token_hint.is_none() || s.umbrel_token_hint.is_none();
    rsx! {
        Section { icon: rsx! { Link {} }, title: "Verbindungen", hint: "Einmal einrichten – danach musst du hier nichts mehr tun.",
            details { class: "group", open,
                summary { class: "cursor-pointer select-none text-sm text-white/60 hover:text-white", "Anzeigen" }
                div { class: "mt-4 flex flex-col gap-6",
                    div {
                        div { class: "font-medium", "nrzNetwork (Domains)" }
                        p { class: "mb-2 text-sm text-white/50", "Liefert die Adressen deiner Apps. Token: nrzNetwork → Einstellungen → MCP." }
                        div { class: "grid gap-2 sm:grid-cols-2",
                            input { class: INPUT, value: "{net_url}", placeholder: "http://nrz-dns_app_1:8380/mcp", oninput: move |e| net_url.set(e.value()) }
                            input { class: INPUT, r#type: "password", value: "{net_token}", placeholder: s.network_token_hint.clone().unwrap_or_else(|| "Token".into()), oninput: move |e| net_token.set(e.value()) }
                        }
                        button {
                            class: "{BTN} mt-2",
                            onclick: move |_| {
                                spawn(async move {
                                    let r = settings_save_network(net_url(), net_token()).await;
                                    net_msg.set(Some(r.map_err(|e| err_msg(&e))));
                                    net_token.set(String::new());
                                    ctx.reload();
                                });
                            },
                            "Verbinden"
                        }
                        Feedback { msg: net_msg() }
                    }
                    div {
                        div { class: "font-medium", "nrzMonitor (Container & System)" }
                        p { class: "mb-2 text-sm text-white/50", "Auf demselben Gerät reicht die interne Adresse ohne Schlüssel. Für eine öffentliche Adresse: Schlüssel aus nrzMonitor → Einstellungen → MCP (\"-\" entfernt ihn)." }
                        div { class: "grid gap-2 sm:grid-cols-2",
                            input { class: INPUT, value: "{mon_url}", placeholder: "http://10.21.0.1:8571", oninput: move |e| mon_url.set(e.value()) }
                            input { class: INPUT, r#type: "password", value: "{mon_token}", placeholder: s.monitor_token_hint.clone().unwrap_or_else(|| "Schlüssel (optional)".into()), oninput: move |e| mon_token.set(e.value()) }
                        }
                        button {
                            class: "{BTN} mt-2",
                            onclick: move |_| {
                                spawn(async move {
                                    let r = settings_save_monitor(mon_url(), mon_token()).await;
                                    mon_msg.set(Some(r.map_err(|e| err_msg(&e))));
                                    mon_token.set(String::new());
                                    ctx.reload();
                                });
                            },
                            "Verbinden"
                        }
                        Feedback { msg: mon_msg() }
                    }
                    div {
                        div { class: "font-medium", "umbrelOS" }
                        p { class: "mb-2 text-sm text-white/50", "Für Apps, App Store und Aktivität. Token: umbrelOS → Einstellungen → AI agents (MCP)." }
                        div { class: "grid gap-2 sm:grid-cols-2",
                            input { class: INPUT, value: "{um_url}", placeholder: "http://umbrel.local", oninput: move |e| um_url.set(e.value()) }
                            input { class: INPUT, r#type: "password", value: "{um_token}", placeholder: s.umbrel_token_hint.clone().unwrap_or_else(|| "Token".into()), oninput: move |e| um_token.set(e.value()) }
                        }
                        button {
                            class: "{BTN} mt-2",
                            onclick: move |_| {
                                spawn(async move {
                                    let r = settings_save_umbrel(um_url(), um_token()).await;
                                    um_msg.set(Some(r.map_err(|e| err_msg(&e))));
                                    um_token.set(String::new());
                                    ctx.reload();
                                });
                            },
                            "Verbinden"
                        }
                        Feedback { msg: um_msg() }
                    }
                }
            }
        }
    }
}
