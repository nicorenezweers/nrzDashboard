use dioxus::prelude::*;
use icons::{Bot, FolderOpen, House, LogOut};

use crate::pages::agent::AgentPage;
use crate::pages::files::FilesPage;
use crate::pages::home::{HomePage, wallpaper_style};
use crate::pages::login::LoginPage;

const TAILWIND: Asset = asset!("/assets/tailwind.generated.css");
/// Umbrella icon as data URI (no extra file).
const FAVICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'%3E%3Crect width='64' height='64' rx='15' fill='%231c1c1e'/%3E%3Cpath d='M10 40C10 26 20 16 32 16s22 10 22 24' fill='none' stroke='white' stroke-width='6' stroke-linecap='round'/%3E%3C/svg%3E";

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[route("/?:w")]
    HomePage { w: String },
    #[layout(Shell)]
        #[route("/agent")]
        AgentPage {},
        #[route("/files?:path&:msg")]
        FilesPage { path: String, msg: String },
    #[end_layout]
    #[route("/login?:error&:step")]
    LoginPage { error: String, step: String },
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
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
    navigator().replace(Route::HomePage { w: String::new() });
    rsx! {}
}

#[derive(Clone, Copy, PartialEq)]
enum Nav {
    Home,
    Files,
    Agent,
}

impl Nav {
    const ALL: [Self; 3] = [Self::Home, Self::Files, Self::Agent];

    fn of(route: &Route) -> Option<Self> {
        Some(match route {
            Route::HomePage { .. } => Self::Home,
            Route::FilesPage { .. } => Self::Files,
            Route::AgentPage {} => Self::Agent,
            _ => return None,
        })
    }

    fn route(self) -> Route {
        match self {
            Self::Home => Route::HomePage { w: String::new() },
            Self::Files => Route::FilesPage { path: "/Home".into(), msg: String::new() },
            Self::Agent => Route::AgentPage {},
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Home => "Start",
            Self::Files => "Dateien",
            Self::Agent => "Agent",
        }
    }

    fn icon(self, class: &str) -> Element {
        let class = class.to_string();
        match self {
            Self::Home => rsx! { House { class } },
            Self::Files => rsx! { FolderOpen { class } },
            Self::Agent => rsx! { Bot { class } },
        }
    }
}

/// Frame for files and agent: wallpaper, slim glass bar, mobile tab bar.
#[component]
fn Shell() -> Element {
    let route = use_route::<Route>();
    let current = Nav::of(&route);
    let width = if current == Some(Nav::Files) { "max-w-none" } else { "max-w-6xl" };

    rsx! {
        div { class: "pointer-events-none fixed inset-0", style: wallpaper_style(),
            div { class: "absolute inset-0 bg-background/85" }
        }
        div { class: "relative flex h-full flex-col",
            header { class: "z-30 flex h-14 shrink-0 items-center gap-4 border-b border-white/10 bg-black/40 px-4 backdrop-blur-xl md:px-6",
                Link { to: Route::HomePage { w: String::new() }, class: "flex items-center gap-2 font-semibold tracking-tight",
                    House { class: "size-4" }
                    "nrzDashboard"
                }
                nav { class: "hidden md:flex flex-1 items-center justify-center gap-1",
                    for item in Nav::ALL {
                        Link {
                            to: item.route(),
                            class: if current == Some(item) { "flex items-center gap-2 rounded-full bg-white/15 px-3.5 py-1.5 text-sm font-medium text-white" } else { "flex items-center gap-2 rounded-full px-3.5 py-1.5 text-sm text-white/60 hover:bg-white/10 hover:text-white" },
                            {item.icon("size-4")}
                            "{item.label()}"
                        }
                    }
                }
                div { class: "flex-1 md:flex-none" }
                a {
                    href: "/auth/logout",
                    class: "inline-flex size-9 items-center justify-center rounded-full text-white/60 hover:bg-white/10 hover:text-white",
                    title: "Abmelden",
                    "aria-label": "Abmelden",
                    LogOut { class: "size-4" }
                }
            }

            main { class: "min-h-0 flex-1 overflow-y-auto",
                div { class: "mx-auto w-full {width} p-4 pb-28 md:p-6 md:pb-8", Outlet::<Route> {} }
            }

            nav { class: "md:hidden fixed inset-x-0 bottom-0 z-40 grid grid-cols-3 border-t border-white/10 bg-black/60 backdrop-blur-xl pb-[env(safe-area-inset-bottom,0px)]",
                for item in Nav::ALL {
                    Link {
                        to: item.route(),
                        class: if current == Some(item) { "flex h-16 flex-col items-center justify-center gap-1 text-[11px] text-white" } else { "flex h-16 flex-col items-center justify-center gap-1 text-[11px] text-white/50" },
                        {item.icon("size-5")}
                        "{item.label()}"
                    }
                }
            }
        }
    }
}
