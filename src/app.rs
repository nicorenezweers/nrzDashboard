use dioxus::prelude::*;
use icons::{Activity, Bot, FolderOpen, House, LayoutDashboard, LogOut, Server};

use crate::pages::agent::AgentPage;
use crate::pages::board::BoardPage;
use crate::pages::files::FilesPage;
use crate::pages::home::{HomePage, Prefs, wallpaper_style};
use crate::pages::login::LoginPage;
use crate::pages::monitors::MonitorsPage;
use crate::pages::system::SystemPage;

const TAILWIND: Asset = asset!("/assets/tailwind.generated.css");
const FAVICON: Asset = asset!("/assets/favicon.svg");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[route("/?:w")]
    HomePage { w: String },
    #[layout(Shell)]
        #[route("/board")]
        BoardPage {},
        #[route("/monitors")]
        MonitorsPage {},
        #[route("/agent")]
        AgentPage {},
        #[route("/files?:path&:msg")]
        FilesPage { path: String, msg: String },
        #[route("/system")]
        SystemPage {},
    #[end_layout]
    #[route("/login?:error")]
    LoginPage { error: String },
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

#[component]
pub fn App() -> Element {
    rsx! {
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1, viewport-fit=cover" }
        document::Meta { name: "theme-color", content: "#1f1f1f" }
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
    Board,
    Monitors,
    Files,
    Agent,
    System,
}

impl Nav {
    fn of(route: &Route) -> Option<Self> {
        Some(match route {
            Route::HomePage { .. } => Self::Home,
            Route::BoardPage {} => Self::Board,
            Route::MonitorsPage {} => Self::Monitors,
            Route::FilesPage { .. } => Self::Files,
            Route::AgentPage {} => Self::Agent,
            Route::SystemPage {} => Self::System,
            _ => return None,
        })
    }

    fn route(self) -> Route {
        match self {
            Self::Home => Route::HomePage { w: String::new() },
            Self::Board => Route::BoardPage {},
            Self::Monitors => Route::MonitorsPage {},
            Self::Files => Route::FilesPage { path: "/Home".into(), msg: String::new() },
            Self::Agent => Route::AgentPage {},
            Self::System => Route::SystemPage {},
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Home => "Start",
            Self::Board => "Board",
            Self::Monitors => "Monitore",
            Self::Files => "Dateien",
            Self::Agent => "Agent",
            Self::System => "System",
        }
    }

    fn icon(self, class: &str) -> Element {
        let class = class.to_string();
        match self {
            Self::Home => rsx! { House { class } },
            Self::Board => rsx! { LayoutDashboard { class } },
            Self::Monitors => rsx! { Activity { class } },
            Self::Files => rsx! { FolderOpen { class } },
            Self::Agent => rsx! { Bot { class } },
            Self::System => rsx! { Server { class } },
        }
    }
}

const NAV: [Nav; 6] = [Nav::Home, Nav::Board, Nav::Monitors, Nav::Files, Nav::Agent, Nav::System];

#[component]
fn Logo() -> Element {
    rsx! {
        div { class: "flex items-center gap-2 font-semibold tracking-tight",
            div { class: "flex size-7 items-center justify-center rounded-md bg-primary text-primary-foreground text-xs font-bold",
                "nrz"
            }
            span { "nrzDashboard" }
        }
    }
}

#[component]
fn Shell() -> Element {
    let route = use_route::<Route>();
    let current = Nav::of(&route);
    // Files and boards use the full width; other pages stay readable.
    let width = match current {
        Some(Nav::Files) => "max-w-none",
        Some(Nav::Board) => "max-w-[1800px]",
        _ => "max-w-6xl",
    };

    let mut prefs = use_signal(Prefs::default);
    use_effect(move || prefs.set(Prefs::load()));

    rsx! {
        div { class: "pointer-events-none fixed inset-0 -z-0", style: wallpaper_style(&prefs.read()),
            div { class: "absolute inset-0 bg-background/80" }
        }
        div { class: "relative flex h-full flex-col",
            // Top bar (desktop: navigation, mobile: logo + logout)
            header { class: "z-30 flex h-14 shrink-0 items-center gap-4 border-b border-sidenav-border bg-sidenav/80 px-4 backdrop-blur md:px-6",
                Link { to: Route::HomePage { w: String::new() }, Logo {} }
                nav { class: "hidden md:flex flex-1 items-center justify-center gap-1",
                    for item in NAV {
                        Link {
                            to: item.route(),
                            class: if current == Some(item) { "flex items-center gap-2 rounded-full px-3.5 py-1.5 text-sm bg-sidenav-accent text-sidenav-accent-foreground font-medium" } else { "flex items-center gap-2 rounded-full px-3.5 py-1.5 text-sm text-muted-foreground hover:bg-sidenav-accent/60 hover:text-foreground" },
                            {item.icon("size-4")}
                            "{item.label()}"
                        }
                    }
                }
                div { class: "flex-1 md:flex-none" }
                a {
                    href: "/auth/logout",
                    class: "inline-flex size-9 items-center justify-center rounded-full text-muted-foreground hover:bg-accent hover:text-foreground",
                    title: "Abmelden",
                    "aria-label": "Abmelden",
                    LogOut { class: "size-4" }
                }
            }

            main { class: "min-h-0 flex-1 overflow-y-auto",
                div { class: "mx-auto w-full {width} p-4 pb-28 md:p-6 md:pb-8", Outlet::<Route> {} }
            }

            // Mobile bottom navigation
            nav { class: "md:hidden fixed inset-x-0 bottom-0 z-40 grid grid-cols-6 border-t bg-sidenav/95 backdrop-blur pb-[env(safe-area-inset-bottom,0px)]",
                for item in NAV {
                    Link {
                        to: item.route(),
                        class: if current == Some(item) { "flex h-16 flex-col items-center justify-center gap-1 text-[11px] text-foreground" } else { "flex h-16 flex-col items-center justify-center gap-1 text-[11px] text-muted-foreground" },
                        {item.icon("size-5")}
                        "{item.label()}"
                    }
                }
            }
        }
    }
}
