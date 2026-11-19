//! MCP console: every tool of the umbrelOS MCP server, with arguments and result.

use dioxus::prelude::*;
use icons::{Play, Search, TriangleAlert};

use super::{HomeCtx, HomeWindow};
use crate::api::home::{McpTool, mcp_run, mcp_tools};
use crate::components::widgets::ConfirmButton;
use crate::utils::err_msg;

#[component]
pub fn ConsoleWindow() -> Element {
    let tools = use_resource(mcp_tools);
    let mut filter = use_signal(String::new);
    let mut selected = use_signal(|| None::<McpTool>);

    rsx! {
        HomeWindow { title: "MCP-Konsole",
            p { class: "-mt-1 mb-4 text-sm text-white/55",
                "Alle Werkzeuge, die der umbrelOS-MCP-Server deinem Token freigibt – direkt ausführbar. Kritische Werkzeuge brauchen einen zweiten Klick."
            }
            match tools.read().clone() {
                None => rsx! { div { class: "h-64 animate-pulse rounded-3xl bg-white/[0.05]" } },
                Some(Err(e)) => rsx! { p { class: "text-sm text-red-300", {err_msg(&e)} } },
                Some(Ok(list)) => {
                    let f = filter().to_lowercase();
                    let shown: Vec<McpTool> = list.iter().filter(|t| f.is_empty() || t.name.contains(&f) || t.description.to_lowercase().contains(&f)).cloned().collect();
                    rsx! {
                        div { class: "grid gap-4 md:h-[calc(100%-2.5rem)] md:grid-cols-[300px_1fr]",
                            div { class: "flex min-h-0 flex-col rounded-3xl bg-white/[0.04] p-2",
                                div { class: "relative mb-2",
                                    Search { class: "pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-white/40" }
                                    input {
                                        class: "h-9 w-full rounded-xl bg-white/[0.06] pl-9 pr-3 text-sm text-white placeholder:text-white/35 outline-none",
                                        placeholder: "{list.len()} Werkzeuge filtern",
                                        value: "{filter}",
                                        oninput: move |e| filter.set(e.value()),
                                    }
                                }
                                div { class: "max-h-72 min-h-0 flex-1 overflow-y-auto md:max-h-none",
                                    for t in shown {
                                        {
                                            let active = selected.read().as_ref().is_some_and(|s| s.name == t.name);
                                            let t2 = t.clone();
                                            rsx! {
                                                button {
                                                    key: "{t.name}",
                                                    class: if active { "flex w-full items-center gap-2 rounded-lg bg-white/15 px-3 py-1.5 text-left font-mono text-[13px]" } else { "flex w-full items-center gap-2 rounded-lg px-3 py-1.5 text-left font-mono text-[13px] text-white/75 hover:bg-white/[0.07]" },
                                                    onclick: move |_| selected.set(Some(t2.clone())),
                                                    span { class: "truncate", "{t.name}" }
                                                    if t.destructive { TriangleAlert { class: "ml-auto size-3.5 shrink-0 text-amber-300" } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(t) = selected() {
                                ToolRunner { key: "{t.name}", tool: t }
                            } else {
                                div { class: "flex items-center justify-center rounded-3xl bg-white/[0.03] p-10 text-sm text-white/40", "Werkzeug links auswählen" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ToolRunner(tool: McpTool) -> Element {
    let ctx = use_context::<HomeCtx>();
    let mut args = use_signal(|| tool.example.clone());
    let mut running = use_signal(|| false);
    let mut out = use_signal(|| None::<Result<String, String>>);
    let name = tool.name.clone();
    let exec = use_callback(move |_: ()| {
        let name = name.clone();
        let a = args();
        running.set(true);
        spawn(async move {
            let r = mcp_run(name.clone(), a).await.map_err(|e| err_msg(&e));
            out.set(Some(r));
            running.set(false);
            // State may have changed (apps, settings): refresh the home screen.
            if !name.starts_with("get_") && !name.starts_with("list_") && !name.starts_with("search_") {
                ctx.reload();
            }
        });
    });
    rsx! {
        div { class: "flex min-h-0 flex-col gap-3 overflow-y-auto rounded-3xl bg-white/[0.04] p-5",
            div {
                h3 { class: "font-mono text-lg", "{tool.name}" }
                p { class: "mt-1 whitespace-pre-line text-sm text-white/60", "{tool.description}" }
                if tool.destructive {
                    p { class: "mt-2 flex items-center gap-2 text-sm text-amber-300", TriangleAlert { class: "size-4" } "Kritisch: löscht Daten, stoppt oder startet neu." }
                }
            }
            details { class: "text-xs text-white/50",
                summary { class: "cursor-pointer select-none", "Schema" }
                pre { class: "mt-2 max-h-48 overflow-auto rounded-xl bg-black/30 p-3 font-mono", "{tool.schema}" }
            }
            label { class: "text-xs text-white/50", "Argumente (JSON)" }
            textarea {
                class: "min-h-28 w-full rounded-xl border border-white/10 bg-black/30 p-3 font-mono text-[13px] text-white outline-none focus:border-white/30",
                spellcheck: "false",
                value: "{args}",
                oninput: move |e| args.set(e.value()),
            }
            div { class: "flex items-center gap-2",
                if tool.destructive {
                    ConfirmButton { label: "Ausführen", confirm_label: "Wirklich ausführen?", disabled: running(), onconfirm: move |_| exec.call(()), Play { class: "size-4" } }
                } else {
                    button {
                        class: "inline-flex items-center gap-1.5 rounded-full bg-sky-500 px-4 py-1.5 text-sm font-medium text-white hover:bg-sky-400 disabled:opacity-50",
                        disabled: running(),
                        onclick: move |_| exec.call(()),
                        Play { class: "size-4" }
                        if running() { "Läuft…" } else { "Ausführen" }
                    }
                }
            }
            match out() {
                Some(Ok(t)) => rsx! { pre { class: "max-h-[50vh] overflow-auto whitespace-pre-wrap rounded-xl bg-black/40 p-3 font-mono text-xs text-emerald-100", "{t}" } },
                Some(Err(e)) => rsx! { pre { class: "whitespace-pre-wrap rounded-xl bg-red-500/10 p-3 font-mono text-xs text-red-200", "{e}" } },
                None => rsx! {},
            }
        }
    }
}
