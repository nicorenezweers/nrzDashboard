//! The agent loop: runs as a tokio task per conversation (independent of the browser).
//! Model turn → tool calls (with confirmation for critical tools) → next turn, until done.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use serde_json::Value;
use tokio::sync::oneshot;
use tokio::task::AbortHandle;

use super::llm::{self, Msg, Stop, ToolDef, ToolResult};
use super::{settings, store};
use crate::api::agent::{Item, Mode, RunState, ToolStatus, ToolView};
use crate::server::mcp;

pub const MAX_ROUNDS: u32 = 25;
const UI_RESULT_CHARS: usize = 6_000;
const MODEL_RESULT_CHARS: usize = 60_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Decision {
    Allow,
    Always,
    Deny,
}

fn runs() -> MutexGuard<'static, HashMap<String, AbortHandle>> {
    static R: OnceLock<Mutex<HashMap<String, AbortHandle>>> = OnceLock::new();
    R.get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

type ApprovalMap = HashMap<(String, String), oneshot::Sender<Decision>>;

fn approvals() -> MutexGuard<'static, ApprovalMap> {
    static A: OnceLock<Mutex<ApprovalMap>> = OnceLock::new();
    A.get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Critical tools: the fixed list plus anything that deletes, resets or restarts.
pub fn is_destructive(name: &str) -> bool {
    const PATTERNS: &[&str] = &[
        "delete",
        "uninstall",
        "reset",
        "trash",
        "restart",
        "remove",
        "shutdown",
        "force",
        "wipe",
        "format",
        "kill",
    ];
    crate::model::DESTRUCTIVE_TOOLS.contains(&name) || PATTERNS.iter().any(|p| name.contains(p))
}

fn system_prompt() -> String {
    let date = crate::utils::fmt_time_ms(super::now_ms() as f64);
    let date = date.split(' ').next().unwrap_or_default();
    format!(
        "Du bist der KI-Agent im nrzDashboard und betreust das umbrelOS-Gerät (Home-Server) dieses Nutzers. \
Über die Werkzeuge des umbrelOS-MCP-Servers kannst du Apps verwalten (Status, Logs, starten, stoppen, installieren …), \
Dateien ansehen und organisieren, Systeminfos und Ressourcen lesen sowie virtuelle Maschinen steuern.\n\n\
Regeln:\n\
- Antworte auf Deutsch, knapp und konkret. Nutze Markdown (Listen, Tabellen, `Code`), wo es hilft.\n\
- Arbeite selbstständig: rufe Werkzeuge auf, bis die Aufgabe erledigt ist, statt den Nutzer nachschauen zu lassen. \
Erfinde keine Werte – prüfe sie mit Werkzeugen. Fasse am Ende das Ergebnis kurz zusammen.\n\
- Vor riskanten Schritten (löschen, deinstallieren, zurücksetzen, stoppen, neu starten, Systemeinstellungen ändern) \
erkläre in einem Satz, was du tun wirst und warum. Solche Aufrufe muss der Nutzer eventuell bestätigen. \
Lehnt er ab, versuche es nicht auf anderem Weg, sondern frage nach.\n\
- Deinstallieren löscht App-Daten unwiderruflich; Dateien können nur in den Papierkorb verschoben werden.\n\
- Fehlt eine Berechtigung, sag dem Nutzer, dass er sie in umbrelOS unter Einstellungen → AI agents (MCP) freigeben kann.\n\n\
Heutiges Datum (UTC): {date}"
    )
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!(
        "{head}\n… (gekürzt, {} Zeichen insgesamt)",
        s.chars().count()
    )
}

/// Starts the agent for a conversation whose state was already set to `Running`.
pub fn start(id: &str) {
    let mut r = runs();
    let handle = tokio::spawn(run_guarded(id.to_string()));
    r.insert(id.to_string(), handle.abort_handle());
}

async fn run_guarded(id: String) {
    if let Err(e) = run(&id).await {
        store::update(&id, false, |s| {
            store::repair(s, "Abgebrochen wegen eines Fehlers");
            s.conv.items.push(Item::Notice {
                text: e,
                error: true,
            });
        });
    }
    runs().remove(&id);
    approvals().retain(|(c, _), _| c != &id);
    store::update(&id, true, |s| {
        s.conv.state = RunState::Idle;
        s.conv.streaming.clear();
    });
}

/// Stops a running agent (no-op when idle).
pub fn cancel(id: &str) {
    let handle = runs().remove(id);
    if let Some(h) = handle {
        h.abort();
    }
    approvals().retain(|(c, _), _| c != id);
    store::update(id, true, |s| {
        if s.conv.state != RunState::Idle {
            store::repair(s, "Vom Nutzer abgebrochen");
            s.conv.items.push(Item::Notice {
                text: "Abgebrochen.".into(),
                error: false,
            });
            s.conv.state = RunState::Idle;
        }
    });
}

pub fn decide(id: &str, call_id: &str, decision: &str) -> Result<(), String> {
    let d = match decision {
        "allow" => Decision::Allow,
        "always" => Decision::Always,
        "deny" => Decision::Deny,
        _ => return Err("Unbekannte Entscheidung".into()),
    };
    let tx = approvals()
        .remove(&(id.to_string(), call_id.to_string()))
        .ok_or("Diese Anfrage ist nicht mehr offen")?;
    let _ = tx.send(d);
    Ok(())
}

fn set_tool(id: &str, call_id: &str, persist: bool, f: impl FnOnce(&mut ToolView)) {
    store::update(id, persist, |s| {
        if let Some(Item::Tool(t)) = s
            .conv
            .items
            .iter_mut()
            .rev()
            .find(|i| matches!(i, Item::Tool(t) if t.id == call_id))
        {
            f(t);
        }
    });
}

fn push_result(id: &str, result: ToolResult) {
    store::update(id, false, |s| {
        if let Some(Msg::ToolResults { results }) = s.history.last_mut() {
            results.push(result);
        }
    });
}

async fn run(id: &str) -> Result<(), String> {
    let system = system_prompt();
    let all_tools: Vec<ToolDef> = mcp::list_tools()
        .await
        .map_err(|e| format!("Werkzeuge vom umbrelOS-MCP-Server nicht abrufbar: {e}"))?
        .iter()
        .filter_map(ToolDef::from_mcp)
        .collect();

    for round in 1..=MAX_ROUNDS {
        let (provider, mode, history) =
            store::read(id, |s| (s.conv.provider, s.conv.mode, s.history.clone()))
                .ok_or("Unterhaltung gelöscht")?;
        let key = settings::key(provider).ok_or_else(|| {
            format!("Kein API-Schlüssel für {} hinterlegt – in den Agent-Einstellungen (Zahnrad) eintragen.", provider.label())
        })?;
        let model = settings::model(provider);
        let tools: Vec<ToolDef> = all_tools
            .iter()
            .filter(|t| mode != Mode::ReadOnly || !is_destructive(&t.name))
            .cloned()
            .collect();
        store::update(id, false, |s| {
            s.conv.model = model.clone();
            s.conv.round = round;
            s.conv.streaming.clear();
        });

        let req = llm::Request {
            provider,
            key: &key,
            model: &model,
            system: &system,
            tools: &tools,
            history: &history,
        };
        let mut on_text = |t: &str| {
            store::update(id, false, |s| s.conv.streaming.push_str(t));
        };
        let turn = llm::stream_turn(&req, &mut on_text).await?;

        let calls = turn.calls.clone();
        store::update(id, true, |s| {
            s.conv.usage.add(&turn.usage);
            s.conv.streaming.clear();
            if !turn.text.trim().is_empty() {
                s.conv.items.push(Item::Assistant {
                    text: turn.text.clone(),
                });
            }
            s.history.push(Msg::Assistant {
                text: turn.text.clone(),
                calls: turn.calls.clone(),
                raw: turn.raw.clone(),
            });
            if !calls.is_empty() {
                s.history.push(Msg::ToolResults { results: vec![] });
                for c in &calls {
                    s.conv.items.push(Item::Tool(ToolView {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        args: serde_json::to_string_pretty(&c.input).unwrap_or_default(),
                        result: None,
                        status: ToolStatus::Pending,
                        duration_ms: None,
                        destructive: is_destructive(&c.name),
                    }));
                }
            }
        });

        match &turn.stop {
            Stop::Refusal(cat) => {
                let cat = cat
                    .as_deref()
                    .map(|c| format!(" (Kategorie: {c})"))
                    .unwrap_or_default();
                store::update(id, true, |s| {
                    store::repair(s, "Nicht ausgeführt (Anfrage abgelehnt)");
                    s.conv.items.push(Item::Notice {
                        text: format!("Das Modell hat die Anfrage abgelehnt{cat}."),
                        error: true,
                    });
                });
                return Ok(());
            }
            Stop::MaxTokens => {
                store::update(id, true, |s| {
                    store::repair(s, "Nicht ausgeführt (Antwort abgeschnitten)");
                    s.conv.items.push(Item::Notice {
                        text: "Die Antwort wurde wegen des Token-Limits abgeschnitten.".into(),
                        error: true,
                    });
                });
                return Ok(());
            }
            _ => {}
        }
        if calls.is_empty() {
            return Ok(());
        }

        let mut denied = false;
        for call in &calls {
            let destructive = is_destructive(&call.name);
            let (mode, allowed) = store::read(id, |s| {
                (s.conv.mode, s.conv.allowed_tools.contains(&call.name))
            })
            .ok_or("Unterhaltung gelöscht")?;

            let refuse = |text: &str, status: ToolStatus| {
                set_tool(id, &call.id, true, |t| {
                    t.status = status;
                    t.result = Some(text.to_string());
                });
                push_result(
                    id,
                    ToolResult {
                        id: call.id.clone(),
                        content: text.to_string(),
                        is_error: true,
                    },
                );
            };

            if denied {
                refuse(
                    "Übersprungen, weil der Nutzer einen vorherigen Aufruf abgelehnt hat.",
                    ToolStatus::Cancelled,
                );
                continue;
            }
            if let Some(raw) = &call.invalid {
                refuse(
                    &format!("{{\"INVALID_JSON\": {}}}", Value::String(raw.clone())),
                    ToolStatus::Error,
                );
                continue;
            }
            if destructive && mode == Mode::ReadOnly {
                refuse(
                    "Im Modus „Nur lesen“ sind kritische Werkzeuge nicht erlaubt.",
                    ToolStatus::Denied,
                );
                continue;
            }
            if destructive && mode == Mode::Ask && !allowed {
                let (tx, rx) = oneshot::channel();
                approvals().insert((id.to_string(), call.id.clone()), tx);
                set_tool(id, &call.id, true, |t| t.status = ToolStatus::Approval);
                store::update(id, true, |s| s.conv.state = RunState::Waiting);
                let decision = rx.await.unwrap_or(Decision::Deny);
                store::update(id, true, |s| {
                    s.conv.state = RunState::Running;
                    if decision == Decision::Always && !s.conv.allowed_tools.contains(&call.name) {
                        s.conv.allowed_tools.push(call.name.clone());
                    }
                });
                if decision == Decision::Deny {
                    denied = true;
                    refuse(
                        "Der Nutzer hat diesen Aufruf abgelehnt.",
                        ToolStatus::Denied,
                    );
                    continue;
                }
            }

            set_tool(id, &call.id, false, |t| t.status = ToolStatus::Running);
            let started = Instant::now();
            let res = mcp::call(&call.name, call.input.clone()).await;
            let ms = started.elapsed().as_millis() as u64;
            let (text, is_error) = match res {
                Ok(v) => (crate::api::value_to_text(&v), false),
                Err(e) => (e, true),
            };
            set_tool(id, &call.id, true, |t| {
                t.status = if is_error {
                    ToolStatus::Error
                } else {
                    ToolStatus::Done
                };
                t.duration_ms = Some(ms);
                t.result = Some(truncate(&text, UI_RESULT_CHARS));
            });
            push_result(
                id,
                ToolResult {
                    id: call.id.clone(),
                    content: truncate(&text, MODEL_RESULT_CHARS),
                    is_error,
                },
            );
        }
    }

    store::update(id, true, |s| {
        s.conv.items.push(Item::Notice {
            text: format!("Limit von {MAX_ROUNDS} Werkzeug-Runden erreicht – schreib „weiter“, um fortzufahren."),
            error: false,
        });
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_detection() {
        assert!(is_destructive("uninstall_app"));
        assert!(is_destructive("stop_app"));
        assert!(is_destructive("restart_app"));
        assert!(is_destructive("delete_machine"));
        assert!(!is_destructive("list_apps"));
        assert!(!is_destructive("get_app_logs"));
    }

    #[test]
    fn truncate_marks_cut_text() {
        assert_eq!(truncate("abc", 5), "abc");
        assert!(truncate(&"x".repeat(10), 4).starts_with("xxxx\n… (gekürzt, 10"));
    }
}
