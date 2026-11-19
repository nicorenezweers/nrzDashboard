//! Login: password, then (when switched on) the 6-digit code from the authenticator app.
//! Plain HTML forms (`POST /auth/login`, `POST /auth/code`) – work before the WASM bundle loads.

use dioxus::prelude::*;
use icons::{KeyRound, ShieldCheck};

use crate::pages::home::wallpaper_style;

const INPUT: &str = "h-12 w-full rounded-xl border border-white/10 bg-white/[0.08] px-4 text-[15px] text-white placeholder:text-white/40 outline-none transition focus:border-white/40 focus:bg-white/[0.12]";
const BTN: &str = "mt-2 h-12 w-full rounded-xl bg-white text-[15px] font-semibold text-black transition hover:bg-white/90 active:scale-[0.99]";

#[component]
pub fn LoginPage(error: String, step: String) -> Element {
    let code_step = step == "code";
    let message = match error.as_str() {
        "" => None,
        "locked" => Some("Zu viele Fehlversuche – bitte eine Minute warten."),
        "code" => Some("Der Code stimmt nicht. Bitte den aktuellen Code eingeben."),
        "expired" => Some("Die Anmeldung ist abgelaufen – bitte erneut anmelden."),
        _ => Some("Benutzername oder Passwort ist falsch."),
    };
    rsx! {
        div { class: "fixed inset-0 flex items-center justify-center p-4 text-white", style: wallpaper_style(),
            div { class: "absolute inset-0 bg-black/30" }
            div { class: "relative w-full max-w-sm rounded-[1.75rem] border border-white/10 bg-black/40 p-7 shadow-2xl backdrop-blur-2xl",
                div { class: "mb-6 flex flex-col items-center text-center",
                    div { class: "mb-4 flex size-14 items-center justify-center rounded-[24%] bg-gradient-to-b from-[#2c2c2e] to-[#1c1c1e] shadow-lg",
                        if code_step { ShieldCheck { class: "size-7" } } else { KeyRound { class: "size-7" } }
                    }
                    h1 { class: "text-2xl font-bold", if code_step { "Bestätigungscode" } else { "Willkommen zurück" } }
                    p { class: "mt-1 text-sm text-white/60",
                        if code_step { "Gib den 6-stelligen Code aus deiner Authenticator-App ein." } else { "Melde dich bei deinem Umbrel an." }
                    }
                }
                if let Some(m) = message {
                    div { class: "mb-4 rounded-xl bg-red-500/15 px-4 py-3 text-sm text-red-100", "{m}" }
                }
                if code_step {
                    form { method: "post", action: "/auth/code", class: "flex flex-col gap-3",
                        input {
                            class: "{INPUT} h-14 text-center text-2xl font-semibold tracking-[0.5em]",
                            name: "code",
                            inputmode: "numeric",
                            autocomplete: "one-time-code",
                            pattern: "[0-9 ]*",
                            maxlength: "7",
                            placeholder: "••••••",
                            required: true,
                            autofocus: true,
                        }
                        button { r#type: "submit", class: BTN, "Bestätigen" }
                        a { href: "/login", class: "mt-1 text-center text-sm text-white/50 hover:text-white", "Zurück zur Anmeldung" }
                    }
                } else {
                    form { method: "post", action: "/auth/login", class: "flex flex-col gap-3",
                        input { class: INPUT, name: "username", placeholder: "Benutzername", autocomplete: "username", autocapitalize: "none", required: true, autofocus: true }
                        input { class: INPUT, name: "password", r#type: "password", placeholder: "Passwort", autocomplete: "current-password", required: true }
                        button { r#type: "submit", class: BTN, "Anmelden" }
                    }
                }
            }
        }
    }
}
