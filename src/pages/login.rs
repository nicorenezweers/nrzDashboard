use dioxus::prelude::*;
use icons::Lock;

use crate::components::ui::button::Button;
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::widgets::{ErrorBox, input_class};

/// Plain HTML form → `POST /auth/login` (works even before the WASM bundle has loaded).
#[component]
pub fn LoginPage(error: String) -> Element {
    let message = match error.as_str() {
        "" => None,
        "locked" => Some("Zu viele Fehlversuche – bitte eine Minute warten."),
        _ => Some("Benutzername oder Passwort ist falsch."),
    };
    rsx! {
        div { class: "flex min-h-full items-center justify-center p-4",
            Card { class: "w-full max-w-sm",
                CardHeader {
                    div { class: "mb-2 flex size-10 items-center justify-center rounded-lg bg-primary text-primary-foreground",
                        Lock { class: "size-5" }
                    }
                    CardTitle { class: "text-xl", "nrzDashboard" }
                    CardDescription { "Melde dich an, um dein Umbrel zu verwalten." }
                }
                CardContent {
                    form { method: "post", action: "/auth/login", class: "flex flex-col gap-4",
                        if let Some(m) = message {
                            ErrorBox { message: m }
                        }
                        label { class: "flex flex-col gap-1.5 text-sm",
                            span { class: "text-muted-foreground", "Benutzername" }
                            input { class: input_class(), name: "username", autocomplete: "username", autocapitalize: "none", required: true, autofocus: true }
                        }
                        label { class: "flex flex-col gap-1.5 text-sm",
                            span { class: "text-muted-foreground", "Passwort" }
                            input { class: input_class(), name: "password", r#type: "password", autocomplete: "current-password", required: true }
                        }
                        Button { button_type: "submit", class: "w-full mt-2", "Anmelden" }
                    }
                }
            }
        }
    }
}
