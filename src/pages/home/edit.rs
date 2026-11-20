//! Edit an app (right click → Bearbeiten): name, public link and admin link.
//! Empty fields use the automatic values (umbrelOS name, domains from nrzNetwork).

use dioxus::prelude::*;
use icons::{ExternalLink, Globe, ShieldCheck, X};

use super::{AppIcon, HomeCtx, open_url};
use crate::api::apps::{AppLinks, AppTile, app_links_set, normalize_link};
use crate::utils::err_msg;

const INPUT: &str = "h-11 w-full rounded-xl border border-white/10 bg-white/[0.06] px-3.5 text-[15px] text-white placeholder:text-white/35 outline-none focus:border-white/30";

#[component]
pub fn EditDialog() -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut editing = ctx.editing;
    rsx! {
        if let Some(t) = editing() {
            div { class: "fixed inset-0 z-[85] flex items-end justify-center bg-black/50 backdrop-blur-sm sm:items-center sm:p-4", onclick: move |_| editing.set(None),
                div {
                    class: "w-full max-w-lg rounded-t-[1.75rem] border border-white/10 bg-[#1c1c1e]/95 p-6 text-white shadow-2xl backdrop-blur-2xl pb-[max(env(safe-area-inset-bottom,0px),1.5rem)] sm:rounded-[1.75rem]",
                    onclick: move |e| e.stop_propagation(),
                    Form { key: "{t.id}", tile: t }
                }
            }
        }
    }
}

#[component]
fn Form(tile: AppTile) -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut editing = ctx.editing;
    let mut name = use_signal(|| tile.custom.name.clone());
    let mut user = use_signal(|| tile.custom.user.clone());
    let mut admin = use_signal(|| tile.custom.admin.clone());
    let mut error = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);

    let auto_user = tile
        .auto_user_url
        .clone()
        .unwrap_or_else(|| "nicht gefunden".into());
    let auto_admin = tile
        .auto_admin_url
        .clone()
        .unwrap_or_else(|| "nicht gefunden".into());
    let user_err = normalize_link(&user()).err();
    let admin_err = normalize_link(&admin()).err();
    let valid = user_err.is_none() && admin_err.is_none();
    // Link that would be used, for the "test" buttons.
    let eff_user = normalize_link(&user())
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| tile.auto_user_url.clone());
    let eff_admin = normalize_link(&admin())
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| tile.auto_admin_url.clone());
    let id = tile.id.clone();

    let save = move |links: AppLinks| {
        let id = id.clone();
        saving.set(true);
        spawn(async move {
            match app_links_set(id, links).await {
                Ok(()) => {
                    editing.set(None);
                    ctx.reload();
                }
                Err(e) => error.set(Some(err_msg(&e))),
            }
            saving.set(false);
        });
    };
    let mut save_now = save.clone();
    let mut reset = save;

    rsx! {
        div { class: "mb-5 flex items-center gap-4",
            AppIcon { id: tile.id.clone(), name: tile.name.clone(), class: "size-14" }
            div { class: "min-w-0 flex-1",
                div { class: "text-xs uppercase tracking-wider text-white/40", "App bearbeiten" }
                div { class: "truncate text-xl font-semibold", "{tile.name}" }
            }
            button { class: "flex size-9 items-center justify-center rounded-full bg-white/10 text-white/70 hover:bg-white/20", onclick: move |_| editing.set(None), X { class: "size-4" } }
        }

        label { class: "mb-1.5 block text-sm text-white/60", "Name" }
        input { class: INPUT, value: "{name}", maxlength: "40", placeholder: "{tile.auto_name}", oninput: move |e| name.set(e.value()) }

        LinkField {
            icon: rsx! { Globe {} },
            label: "Öffentlicher Link",
            value: user(),
            placeholder: auto_user,
            error: user_err,
            test: eff_user,
            oninput: move |v| user.set(v),
        }
        LinkField {
            icon: rsx! { ShieldCheck {} },
            label: "Admin-Link",
            value: admin(),
            placeholder: auto_admin,
            error: admin_err,
            test: eff_admin,
            oninput: move |v| admin.set(v),
        }
        p { class: "mt-3 text-xs text-white/40", "Leer lassen = automatisch aus nrzNetwork. Ohne https:// wird es ergänzt." }

        if let Some(e) = error() {
            p { class: "mt-3 text-sm text-red-300", "{e}" }
        }
        div { class: "mt-6 flex flex-wrap items-center justify-between gap-2",
            button {
                class: "h-10 rounded-full px-4 text-sm text-white/60 hover:bg-white/10 hover:text-white disabled:opacity-40",
                disabled: tile.custom.is_empty() || saving(),
                onclick: move |_| reset(AppLinks::default()),
                "Auf automatisch zurücksetzen"
            }
            div { class: "flex gap-2",
                button { class: "h-10 rounded-full bg-white/10 px-5 text-sm hover:bg-white/20", onclick: move |_| editing.set(None), "Abbrechen" }
                button {
                    class: "h-10 rounded-full bg-white px-5 text-sm font-semibold text-black hover:bg-white/90 disabled:opacity-40",
                    disabled: !valid || saving(),
                    onclick: move |_| save_now(AppLinks { name: name(), user: user(), admin: admin() }),
                    "Sichern"
                }
            }
        }
    }
}

#[component]
fn LinkField(
    icon: Element,
    label: String,
    value: String,
    placeholder: String,
    error: Option<String>,
    test: Option<String>,
    oninput: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "mb-1.5 mt-4 flex items-center gap-2 text-sm text-white/60 [&_svg]:size-4", {icon} "{label}" }
        div { class: "flex gap-2",
            input {
                class: if error.is_some() { "{INPUT} border-red-400/60" } else { "{INPUT}" },
                value: "{value}",
                placeholder: "{placeholder}",
                inputmode: "url",
                autocapitalize: "none",
                spellcheck: "false",
                oninput: move |e| oninput.call(e.value()),
            }
            button {
                class: "flex size-11 shrink-0 items-center justify-center rounded-xl bg-white/10 text-white/70 hover:bg-white/20 disabled:opacity-30",
                title: "Link testen",
                disabled: test.is_none(),
                onclick: move |_| {
                    if let Some(u) = &test {
                        open_url(u);
                    }
                },
                ExternalLink { class: "size-4" }
            }
        }
        if let Some(e) = &error {
            p { class: "mt-1 text-xs text-red-300", "{e}" }
        }
    }
}
