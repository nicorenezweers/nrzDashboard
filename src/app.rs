use dioxus::prelude::*;

use crate::pages::home::HomePage;
use crate::pages::login::LoginPage;

const TAILWIND: Asset = asset!("/assets/tailwind.generated.css");
/// Umbrella icon as data URI (no extra file).
const FAVICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'%3E%3Crect width='64' height='64' rx='15' fill='%231c1c1e'/%3E%3Cpath d='M10 40C10 26 20 16 32 16s22 10 22 24' fill='none' stroke='white' stroke-width='6' stroke-linecap='round'/%3E%3C/svg%3E";

/// Everything lives on the home screen; windows (App Store, files, …) open via `?w=`.
#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[route("/?:w&:path&:msg")]
    HomePage { w: String, path: String, msg: String },
    #[route("/login?:error&:step")]
    LoginPage { error: String, step: String },
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

impl Route {
    /// Home screen without an open window.
    pub fn home() -> Self {
        Self::HomePage {
            w: String::new(),
            path: String::new(),
            msg: String::new(),
        }
    }

    /// File window at `path`.
    pub fn files(path: &str) -> Self {
        Self::HomePage {
            w: "files".into(),
            path: path.into(),
            msg: String::new(),
        }
    }
}

#[component]
pub fn App() -> Element {
    rsx! {
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1, viewport-fit=cover" }
        document::Meta { name: "theme-color", content: "#111111" }
        document::Meta { name: "apple-mobile-web-app-capable", content: "yes" }
        document::Meta { name: "apple-mobile-web-app-status-bar-style", content: "black-translucent" }
        document::Title { "nrzDashboard" }
        document::Link { rel: "icon", href: FAVICON }
        document::Stylesheet { href: TAILWIND }
        div { class: "dark h-full bg-background text-foreground antialiased", Router::<Route> {} }
    }
}

#[component]
fn NotFound(segments: Vec<String>) -> Element {
    let _ = segments;
    navigator().replace(Route::home());
    rsx! {}
}
