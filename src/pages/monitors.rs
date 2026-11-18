//! Monitore (Uptime-Kuma-artig): Liste links, Detail rechts, Formular- und Benachrichtigungs-Dialog.
//! Auf dem Handy öffnet ein Tippen das Detail als eigene Ansicht.

mod detail;
mod form;
mod notify;
mod parts;

use dioxus::prelude::*;
use icons::{ArrowLeft, Bell, Plus, Search};

pub use parts::HeartbeatBar;
#[allow(unused_imports)]
pub use parts::{StatusPip, UptimePill, fmt_ms_opt, fmt_pct};

use crate::api::monitor::{Monitor, MonitorSummary, Status, monitor_detail, monitor_overview};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::skeleton::Skeleton;
use crate::components::widgets::{ErrorBox, PageHeader, input_class};
use crate::utils::{err_msg, sleep_ms};
use parts::KindIcon;

const POLL_MS: u32 = 3_000;

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Up,
    Down,
    Paused,
}

impl Filter {
    const ALL: [Self; 4] = [Self::All, Self::Up, Self::Down, Self::Paused];

    const fn label(self) -> &'static str {
        match self {
            Self::All => "Alle",
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Paused => "Pausiert",
        }
    }

    fn matches(self, s: Status) -> bool {
        match self {
            Self::All => true,
            Self::Up => s == Status::Up,
            Self::Down => matches!(s, Status::Down | Status::Pending),
            Self::Paused => s == Status::Paused,
        }
    }
}

#[component]
pub fn MonitorsPage() -> Element {
    let mut overview = use_resource(monitor_overview);
    let mut selected = use_signal(|| None::<String>);
    let range = use_signal(|| 86_400u64);
    let mut query = use_signal(String::new);
    let mut filter = use_signal(|| Filter::All);
    let mut editing = use_signal(|| None::<Monitor>);
    let mut show_notify = use_signal(|| false);

    // Desktop shows the first monitor when nothing is selected; phones show the list.
    let detail_id = use_memo(move || {
        selected().or_else(|| {
            overview
                .read()
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .and_then(|l| l.first().map(|m| m.id.clone()))
        })
    });
    let mut detail = use_resource(move || {
        let id = detail_id();
        let r = range();
        async move {
            match id {
                Some(id) => monitor_detail(id, r).await.map(Some),
                None => Ok(None),
            }
        }
    });

    use_future(move || async move {
        loop {
            sleep_ms(POLL_MS).await;
            overview.restart();
            detail.restart();
        }
    });

    let data = overview.read().clone();
    let list: Vec<MonitorSummary> = match &data {
        Some(Ok(l)) => l.clone(),
        _ => Vec::new(),
    };
    let count = |f: &dyn Fn(Status) -> bool| list.iter().filter(|m| f(m.status)).count();
    let (up, down, pending, paused) = (
        count(&|s| s == Status::Up),
        count(&|s| s == Status::Down),
        count(&|s| s == Status::Pending),
        count(&|s| s == Status::Paused),
    );
    let q = query().to_lowercase();
    let shown: Vec<MonitorSummary> = list
        .iter()
        .filter(|m| filter().matches(m.status))
        .filter(|m| {
            q.is_empty()
                || m.name.to_lowercase().contains(&q)
                || m.target.to_lowercase().contains(&q)
                || m.group.to_lowercase().contains(&q)
        })
        .cloned()
        .collect();
    let current = detail_id();
    let mobile_detail = selected().is_some();

    let mut on_changed = move |id: Option<String>| {
        if let Some(id) = id {
            selected.set(Some(id));
        }
        overview.restart();
        detail.restart();
    };

    rsx! {
        PageHeader { title: "Monitore", subtitle: "Erreichbarkeit von Diensten, Hosts und Umbrel-Apps",
            Button {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Sm,
                onclick: move |_| show_notify.set(true),
                Bell { class: "size-4" }
                span { class: "hidden sm:inline", "Benachrichtigungen" }
            }
            Button {
                size: ButtonSize::Sm,
                onclick: move |_| editing.set(Some(Monitor::default())),
                Plus { class: "size-4" }
                "Neuer Monitor"
            }
        }

        // Summary
        div { class: if mobile_detail { "hidden lg:flex mb-4 flex-wrap gap-2" } else { "mb-4 flex flex-wrap gap-2" },
            SummaryChip { label: "Up", value: up, status: Status::Up }
            SummaryChip { label: "Down", value: down, status: Status::Down }
            if pending > 0 {
                SummaryChip { label: "Ausstehend", value: pending, status: Status::Pending }
            }
            SummaryChip { label: "Pausiert", value: paused, status: Status::Paused }
        }

        if let Some(Err(e)) = &data {
            ErrorBox { message: err_msg(e) }
        }

        div { class: "grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,22rem)_minmax(0,1fr)] lg:items-start",
            // List
            section { class: if mobile_detail { "hidden lg:flex flex-col gap-3 rounded-xl border bg-card p-3" } else { "flex flex-col gap-3 rounded-xl border bg-card p-3" },
                div { class: "relative",
                    Search { class: "pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground" }
                    input {
                        class: "{input_class()} pl-8",
                        r#type: "search",
                        placeholder: "Suchen …",
                        value: "{query}",
                        oninput: move |e| query.set(e.value()),
                    }
                }
                div { class: "flex flex-wrap gap-1",
                    for f in Filter::ALL {
                        Button {
                            size: ButtonSize::Sm,
                            variant: if filter() == f { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                            onclick: move |_| filter.set(f),
                            "{f.label()}"
                        }
                    }
                }
                if data.is_none() {
                    for _ in 0..4 {
                        Skeleton { class: "h-14 w-full" }
                    }
                } else if shown.is_empty() {
                    p { class: "py-6 text-center text-sm text-muted-foreground",
                        if list.is_empty() { "Noch keine Monitore" } else { "Keine Treffer" }
                    }
                }
                div { class: "flex flex-col gap-1 lg:max-h-[calc(100dvh-16rem)] lg:overflow-y-auto",
                    for m in shown {
                        ListItem {
                            key: "{m.id}",
                            active: current.as_deref() == Some(m.id.as_str()),
                            onselect: move |id| selected.set(Some(id)),
                            monitor: m,
                        }
                    }
                }
            }

            // Detail
            section { class: if mobile_detail { "min-w-0" } else { "hidden lg:block min-w-0" },
                match (&*detail.read(), current.clone()) {
                    (Some(Ok(Some(d))), Some(id)) if d.monitor.id == id => rsx! {
                        detail::Detail {
                            detail: d.clone(),
                            range,
                            onback: move |_| selected.set(None),
                            onedit: move |m| editing.set(Some(m)),
                            onchanged: move |_| on_changed(None),
                            ondeleted: move |_| {
                                selected.set(None);
                                on_changed(None);
                            },
                        }
                    },
                    (Some(Err(e)), _) => rsx! {
                        div { class: "flex flex-col gap-3",
                            Button {
                                class: "lg:hidden -ml-2 self-start",
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                onclick: move |_| selected.set(None),
                                ArrowLeft { class: "size-4" }
                                "Zurück"
                            }
                            ErrorBox { message: err_msg(e) }
                        }
                    },
                    (_, None) if data.is_some() => rsx! {
                        div { class: "rounded-xl border border-dashed p-10 text-center text-sm text-muted-foreground",
                            "Lege mit „Neuer Monitor“ den ersten Monitor an."
                        }
                    },
                    _ => rsx! {
                        div { class: "flex flex-col gap-3",
                            Skeleton { class: "h-24 w-full" }
                            Skeleton { class: "h-64 w-full" }
                        }
                    },
                }
            }
        }

        if let Some(m) = editing() {
            form::MonitorForm {
                initial: m,
                onclose: move |_| editing.set(None),
                onsaved: move |id: String| {
                    editing.set(None);
                    on_changed(Some(id));
                },
            }
        }
        if show_notify() {
            notify::NotifySettings { onclose: move |_| show_notify.set(false) }
        }
    }
}

#[component]
fn SummaryChip(label: String, value: usize, status: Status) -> Element {
    rsx! {
        div { class: "flex items-center gap-2 rounded-lg border bg-card px-3 py-2 text-sm",
            parts::StatusPip { status }
            span { class: "text-muted-foreground", "{label}" }
            span { class: "font-semibold tabular-nums {status.text()}", "{value}" }
        }
    }
}

#[component]
fn ListItem(monitor: MonitorSummary, active: bool, onselect: EventHandler<String>) -> Element {
    let m = monitor;
    let id = m.id.clone();
    let tip = if m.msg.is_empty() {
        m.target.clone()
    } else {
        format!("{}\n{}", m.target, m.msg)
    };
    rsx! {
        button {
            class: if active { "flex w-full flex-col gap-2 rounded-lg bg-accent px-2.5 py-2 text-left" } else { "flex w-full flex-col gap-2 rounded-lg px-2.5 py-2 text-left hover:bg-accent/50" },
            title: "{tip}",
            onclick: move |_| onselect.call(id.clone()),
            div { class: "flex min-w-0 items-center gap-2",
                UptimePill { pct: m.uptime_24h, status: m.status }
                span { class: "min-w-0 flex-1 truncate text-sm font-medium", "{m.name}" }
                if !m.group.is_empty() {
                    span { class: "hidden shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground sm:inline", "{m.group}" }
                }
                span { class: "shrink-0 text-muted-foreground", KindIcon { kind: m.kind, class: "size-3.5" } }
            }
            HeartbeatBar { beats: m.beats.clone(), max: 30, class: "h-4" }
        }
    }
}
