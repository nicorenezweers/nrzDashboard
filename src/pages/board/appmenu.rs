//! Right-click menu for apps on the board (Apps widget + app tiles): open, start/stop/restart/update,
//! logs, credentials, customise (name, icon, URL) and hide.

use dioxus::prelude::*;
use icons::{ArrowUp, ExternalLink, Eye, EyeOff, KeyRound, Pencil, Play, RotateCw, ScrollText, Square, X};

use crate::api::board::{AppOverride, AppTile, app_override_get, app_override_set};
use crate::api::{app_action, app_details, app_logs};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::widgets::{ErrorBox, input_class};
use crate::utils::{err_msg, sleep_ms};

use super::widgets::{AppIcon, browser_url};

#[derive(Clone, PartialEq)]
pub struct AppMenuReq {
    pub tile: AppTile,
    pub x: f64,
    pub y: f64,
}

/// Shared between the board and its widgets.
#[derive(Clone, Copy)]
pub struct AppMenuCtx {
    pub menu: Signal<Option<AppMenuReq>>,
    /// Bumped after every change so app widgets reload at once.
    pub refresh: Signal<u32>,
}

impl AppMenuCtx {
    pub fn open(mut self, tile: AppTile, e: &MouseEvent) {
        e.prevent_default();
        let p = e.client_coordinates();
        self.menu.set(Some(AppMenuReq { tile, x: p.x, y: p.y }));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Dialog {
    Customize,
    Logs,
    Credentials,
}

fn viewport() -> (f64, f64) {
    #[cfg(feature = "web")]
    if let Some(w) = web_sys::window() {
        let wd = w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1200.0);
        let ht = w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(800.0);
        return (wd, ht);
    }
    (1200.0, 800.0)
}

fn is_running(state: &str) -> bool {
    matches!(state, "ready" | "running")
}

#[component]
pub fn AppMenuHost() -> Element {
    let mut ctx = use_context::<AppMenuCtx>();
    let mut dialog = use_signal(|| None::<(Dialog, AppTile)>);
    let mut notice = use_signal(|| None::<Result<String, String>>);
    let mut confirm_stop = use_signal(|| false);

    let req = (ctx.menu)();
    let act = move |id: String, action: &'static str| {
        spawn(async move {
            let r = app_action(id, action.to_string()).await.map_err(|e| err_msg(&e));
            notice.set(Some(r.map(|_| match action {
                "start" => "Wird gestartet…".to_string(),
                "stop" => "Wird gestoppt…".to_string(),
                "restart" => "Wird neu gestartet…".to_string(),
                _ => "Update läuft…".to_string(),
            })));
            for _ in 0..4 {
                sleep_ms(2_500).await;
                ctx.refresh += 1;
            }
        });
    };
    let toggle_hidden = move |tile: AppTile| {
        spawn(async move {
            let mut ov = app_override_get(tile.id.clone()).await.unwrap_or_default();
            ov.hidden = !tile.hidden;
            if let Err(e) = app_override_set(tile.id.clone(), ov).await {
                notice.set(Some(Err(err_msg(&e))));
            }
            ctx.refresh += 1;
        });
    };

    rsx! {
        if let Some(r) = req {
            {
                let (vw, vh) = viewport();
                let x = r.x.min(vw - 248.0).max(8.0);
                let y = r.y.min(vh - 380.0).max(8.0);
                let t = r.tile.clone();
                let running = is_running(&t.state);
                let href = t.url.as_deref().map(browser_url);
                rsx! {
                    div {
                        class: "fixed inset-0 z-[80]",
                        onclick: move |_| {
                            ctx.menu.set(None);
                            confirm_stop.set(false);
                        },
                        oncontextmenu: move |e| {
                            e.prevent_default();
                            ctx.menu.set(None);
                        },
                    }
                    div {
                        class: "fixed z-[81] w-60 rounded-xl border bg-popover p-1.5 text-sm shadow-xl",
                        style: "left: {x}px; top: {y}px;",
                        div { class: "flex items-center gap-2.5 px-2 py-2",
                            AppIcon { key: "{t.id}{t.icon.clone().unwrap_or_default()}", id: t.id.clone(), name: t.name.clone(), url: t.icon.clone().unwrap_or_default(), class: "size-8" }
                            div { class: "min-w-0",
                                div { class: "truncate font-medium", "{t.name}" }
                                div { class: "text-xs text-muted-foreground", if running { "Aktiv" } else { "{t.state}" } }
                            }
                        }
                        div { class: "my-1 h-px bg-border" }
                        if let Some(h) = href {
                            MenuLink { href: h, ExternalLink { class: "size-4" } "Öffnen" }
                        }
                        if running {
                            MenuItem {
                                onclick: {
                                    let id = t.id.clone();
                                    move |_| {
                                        ctx.menu.set(None);
                                        act(id.clone(), "restart");
                                    }
                                },
                                RotateCw { class: "size-4" }
                                "Neu starten"
                            }
                            MenuItem {
                                danger: confirm_stop(),
                                onclick: {
                                    let id = t.id.clone();
                                    move |e: MouseEvent| {
                                        e.stop_propagation();
                                        if confirm_stop() {
                                            confirm_stop.set(false);
                                            ctx.menu.set(None);
                                            act(id.clone(), "stop");
                                        } else {
                                            confirm_stop.set(true);
                                        }
                                    }
                                },
                                Square { class: "size-4" }
                                if confirm_stop() { "Wirklich stoppen?" } else { "Stoppen" }
                            }
                        } else {
                            MenuItem {
                                onclick: {
                                    let id = t.id.clone();
                                    move |_| {
                                        ctx.menu.set(None);
                                        act(id.clone(), "start");
                                    }
                                },
                                Play { class: "size-4" }
                                "Starten"
                            }
                        }
                        if t.update_available {
                            MenuItem {
                                onclick: {
                                    let id = t.id.clone();
                                    move |_| {
                                        ctx.menu.set(None);
                                        act(id.clone(), "update");
                                    }
                                },
                                ArrowUp { class: "size-4" }
                                "Aktualisieren"
                            }
                        }
                        div { class: "my-1 h-px bg-border" }
                        MenuItem {
                            onclick: {
                                let t = t.clone();
                                move |_| {
                                    ctx.menu.set(None);
                                    dialog.set(Some((Dialog::Logs, t.clone())));
                                }
                            },
                            ScrollText { class: "size-4" }
                            "Logs anzeigen"
                        }
                        MenuItem {
                            onclick: {
                                let t = t.clone();
                                move |_| {
                                    ctx.menu.set(None);
                                    dialog.set(Some((Dialog::Credentials, t.clone())));
                                }
                            },
                            KeyRound { class: "size-4" }
                            "Zugangsdaten"
                        }
                        MenuItem {
                            onclick: {
                                let t = t.clone();
                                move |_| {
                                    ctx.menu.set(None);
                                    dialog.set(Some((Dialog::Customize, t.clone())));
                                }
                            },
                            Pencil { class: "size-4" }
                            "Anpassen…"
                        }
                        MenuItem {
                            onclick: {
                                let t = t.clone();
                                move |_| {
                                    ctx.menu.set(None);
                                    toggle_hidden(t.clone());
                                }
                            },
                            if t.hidden {
                                Eye { class: "size-4" }
                                "Einblenden"
                            } else {
                                EyeOff { class: "size-4" }
                                "Ausblenden"
                            }
                        }
                    }
                }
            }
        }

        match dialog() {
            Some((Dialog::Customize, t)) => rsx! {
                CustomizeDialog {
                    tile: t,
                    onclose: move |_| dialog.set(None),
                    onsaved: move |_| {
                        dialog.set(None);
                        ctx.refresh += 1;
                    },
                }
            },
            Some((Dialog::Logs, t)) => rsx! { LogsDialog { tile: t, onclose: move |_| dialog.set(None) } },
            Some((Dialog::Credentials, t)) => rsx! { CredentialsDialog { tile: t, onclose: move |_| dialog.set(None) } },
            None => rsx! {},
        }

        if let Some(n) = notice() {
            div { class: "fixed bottom-20 md:bottom-6 left-1/2 z-[90] -translate-x-1/2 max-w-[90vw]",
                div { class: "flex items-center gap-3 rounded-full border bg-popover px-4 py-2 text-sm shadow-lg",
                    match n {
                        Ok(m) => rsx! { span { "{m}" } },
                        Err(e) => rsx! { span { class: "text-destructive", "{e}" } },
                    }
                    button { class: "text-muted-foreground hover:text-foreground", onclick: move |_| notice.set(None), X { class: "size-3.5" } }
                }
            }
        }
    }
}

#[component]
fn MenuItem(onclick: EventHandler<MouseEvent>, #[props(default)] danger: bool, children: Element) -> Element {
    let color = if danger { "text-destructive" } else { "" };
    rsx! {
        button {
            class: "flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left hover:bg-accent {color} [&_svg]:text-muted-foreground",
            onclick: move |e| onclick.call(e),
            {children}
        }
    }
}

#[component]
fn MenuLink(href: String, children: Element) -> Element {
    rsx! {
        a {
            href: "{href}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 hover:bg-accent [&_svg]:text-muted-foreground",
            {children}
        }
    }
}

#[component]
fn Modal(title: String, onclose: EventHandler<()>, #[props(default)] wide: bool, children: Element) -> Element {
    let w = if wide { "sm:max-w-3xl" } else { "sm:max-w-md" };
    rsx! {
        div { class: "fixed inset-0 z-[85] flex items-end sm:items-center justify-center bg-black/60 sm:p-4", onclick: move |_| onclose.call(()),
            div {
                class: "flex w-full {w} max-h-[90dvh] flex-col gap-4 overflow-y-auto rounded-t-2xl sm:rounded-2xl border bg-card p-4 sm:p-6 pb-[calc(env(safe-area-inset-bottom,0px)+1rem)]",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between gap-2",
                    h2 { class: "truncate text-lg font-semibold", "{title}" }
                    button { class: "p-2 text-muted-foreground hover:text-foreground", onclick: move |_| onclose.call(()), X { class: "size-4" } }
                }
                {children}
            }
        }
    }
}

#[component]
fn CustomizeDialog(tile: AppTile, onclose: EventHandler<()>, onsaved: EventHandler<()>) -> Element {
    let mut ov = use_signal(AppOverride::default);
    let mut error = use_signal(|| None::<String>);
    let id = tile.id.clone();
    use_future({
        let id = id.clone();
        move || {
            let id = id.clone();
            async move {
                if let Ok(v) = app_override_get(id).await {
                    ov.set(v);
                }
            }
        }
    });
    let save = {
        let id = id.clone();
        move |e: FormEvent| {
            e.prevent_default();
            let id = id.clone();
            spawn(async move {
                match app_override_set(id, ov()).await {
                    Ok(()) => onsaved.call(()),
                    Err(e) => error.set(Some(err_msg(&e))),
                }
            });
        }
    };
    let preview_name = if ov.read().name.trim().is_empty() { tile.original_name.clone() } else { ov.read().name.clone() };
    let preview_icon = ov.read().icon.clone();
    rsx! {
        Modal { title: format!("{} anpassen", tile.original_name), onclose,
            form { class: "flex flex-col gap-4", onsubmit: save,
                div { class: "flex items-center gap-3 rounded-xl bg-muted/40 p-3",
                    AppIcon { key: "{preview_icon}", id: tile.id.clone(), name: preview_name.clone(), url: preview_icon.clone(), class: "size-12" }
                    div { class: "min-w-0",
                        div { class: "truncate font-medium", "{preview_name}" }
                        div { class: "text-xs text-muted-foreground font-mono", "{tile.id}" }
                    }
                }
                label { class: "flex flex-col gap-1.5 text-sm",
                    span { class: "text-muted-foreground", "Anzeigename" }
                    input { class: input_class(), value: "{ov.read().name}", placeholder: "{tile.original_name}", oninput: move |e| ov.write().name = e.value() }
                }
                label { class: "flex flex-col gap-1.5 text-sm",
                    span { class: "text-muted-foreground", "Icon-URL" }
                    input { class: "{input_class()} font-mono", value: "{ov.read().icon}", placeholder: "https://…/icon.png", oninput: move |e| ov.write().icon = e.value() }
                }
                label { class: "flex flex-col gap-1.5 text-sm",
                    span { class: "text-muted-foreground", "Adresse beim Öffnen" }
                    input { class: "{input_class()} font-mono", value: "{ov.read().url}", placeholder: tile.url.clone().unwrap_or_else(|| "https://…".into()), oninput: move |e| ov.write().url = e.value() }
                    span { class: "text-xs text-muted-foreground", "Z.B. eine öffentliche Domain statt der lokalen Adresse." }
                }
                label { class: "flex items-center justify-between gap-3 text-sm",
                    span { "Im Apps-Widget ausblenden" }
                    input { r#type: "checkbox", class: "size-4", checked: ov.read().hidden, onchange: move |e| ov.write().hidden = e.checked() }
                }
                if let Some(e) = error() {
                    ErrorBox { message: e }
                }
                div { class: "flex justify-between gap-2 pt-1",
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| ov.set(AppOverride::default()),
                        "Zurücksetzen"
                    }
                    div { class: "flex gap-2",
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, onclick: move |_| onclose.call(()), "Abbrechen" }
                        Button { button_type: "submit", size: ButtonSize::Sm, "Speichern" }
                    }
                }
            }
        }
    }
}

#[component]
fn LogsDialog(tile: AppTile, onclose: EventHandler<()>) -> Element {
    let id = tile.id.clone();
    let mut logs = use_resource(move || app_logs(id.clone(), 300));
    use_future(move || async move {
        loop {
            sleep_ms(5_000).await;
            logs.restart();
        }
    });
    rsx! {
        Modal { title: format!("Logs – {}", tile.name), onclose, wide: true,
            match logs.read().clone() {
                None => rsx! { p { class: "text-sm text-muted-foreground", "Lade…" } },
                Some(Err(e)) => rsx! { ErrorBox { message: err_msg(&e) } },
                Some(Ok(t)) => rsx! {
                    pre { class: "max-h-[65vh] overflow-auto rounded-lg bg-background p-3 text-[11px] leading-relaxed font-mono whitespace-pre-wrap break-all",
                        if t.trim().is_empty() { "(keine Logs)" } else { "{t}" }
                    }
                },
            }
        }
    }
}

#[component]
fn CredentialsDialog(tile: AppTile, onclose: EventHandler<()>) -> Element {
    let id = tile.id.clone();
    let details = use_resource(move || app_details(id.clone()));
    rsx! {
        Modal { title: format!("Zugangsdaten – {}", tile.name), onclose,
            match details.read().clone() {
                None => rsx! { p { class: "text-sm text-muted-foreground", "Lade…" } },
                Some(Err(e)) => rsx! { ErrorBox { message: err_msg(&e) } },
                Some(Ok(d)) => {
                    if d.username.is_none() && d.password.is_none() {
                        rsx! { p { class: "text-sm text-muted-foreground", "Diese App hat keine Standard-Zugangsdaten." } }
                    } else {
                        rsx! {
                            div { class: "grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm",
                                span { class: "text-muted-foreground", "Benutzer" }
                                span { class: "font-mono break-all select-all", {d.username.clone().unwrap_or_else(|| "–".into())} }
                                span { class: "text-muted-foreground", "Passwort" }
                                span { class: "font-mono break-all select-all", {d.password.clone().unwrap_or_else(|| "–".into())} }
                            }
                        }
                    }
                }
            }
        }
    }
}
