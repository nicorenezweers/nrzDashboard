//! Conversations: in memory, persisted as `$DATA_DIR/agent/<id>.json` (0600).

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde::{Deserialize, Serialize};

use super::llm::{Msg, ToolResult};
use super::{now_ms, write_private};
use crate::api::agent::{ConvSummary, Conversation, Item, RunState, ToolStatus};
use crate::server::config;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Stored {
    pub conv: Conversation,
    /// Provider-neutral model history (never sent to the browser).
    pub history: Vec<Msg>,
}

fn dir() -> std::path::PathBuf {
    config().data_dir.join("agent")
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

fn map() -> MutexGuard<'static, HashMap<String, Stored>> {
    static M: OnceLock<Mutex<HashMap<String, Stored>>> = OnceLock::new();
    let m = M.get_or_init(|| Mutex::new(load_all()));
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn load_all() -> HashMap<String, Stored> {
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir()) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Some(mut s) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Stored>(&t).ok())
        else {
            continue;
        };
        if !valid_id(&s.conv.id) {
            continue;
        }
        if s.conv.state != RunState::Idle {
            // The server stopped while the agent was working.
            repair(&mut s, "Abgebrochen (Server wurde neu gestartet)");
            s.conv.items.push(Item::Notice {
                text: "Unterbrochen – der Server wurde neu gestartet.".into(),
                error: true,
            });
            s.conv.state = RunState::Idle;
        }
        s.conv.streaming.clear();
        out.insert(s.conv.id.clone(), s);
    }
    out
}

/// Makes the history valid again after an interruption: open tool calls get an error result,
/// partial text is kept, unfinished tool cards are marked as cancelled.
pub fn repair(s: &mut Stored, reason: &str) {
    let partial = std::mem::take(&mut s.conv.streaming);
    if !partial.trim().is_empty() {
        s.conv.items.push(Item::Assistant {
            text: partial.clone(),
        });
    }
    for item in &mut s.conv.items {
        if let Item::Tool(t) = item
            && matches!(
                t.status,
                ToolStatus::Pending | ToolStatus::Approval | ToolStatus::Running
            )
        {
            t.status = ToolStatus::Cancelled;
        }
    }
    // Find the last assistant message with tool calls and complete its results.
    if let Some(pos) = s
        .history
        .iter()
        .rposition(|m| matches!(m, Msg::Assistant { .. }))
    {
        let ids: Vec<String> = match &s.history[pos] {
            Msg::Assistant { calls, .. } => calls.iter().map(|c| c.id.clone()).collect(),
            _ => vec![],
        };
        if !ids.is_empty() {
            if !matches!(s.history.get(pos + 1), Some(Msg::ToolResults { .. })) {
                s.history
                    .insert(pos + 1, Msg::ToolResults { results: vec![] });
            }
            if let Some(Msg::ToolResults { results }) = s.history.get_mut(pos + 1) {
                for id in ids {
                    if !results.iter().any(|r| r.id == id) {
                        results.push(ToolResult {
                            id,
                            content: reason.into(),
                            is_error: true,
                        });
                    }
                }
            }
        }
    }
    if !partial.trim().is_empty() {
        s.history.push(Msg::Assistant {
            text: partial,
            calls: vec![],
            raw: None,
        });
    }
}

fn file(id: &str) -> std::path::PathBuf {
    dir().join(format!("{id}.json"))
}

fn write(s: &Stored) {
    match serde_json::to_vec(s) {
        Ok(json) => {
            if let Err(e) = write_private(&file(&s.conv.id), &json) {
                crate::server::tracing_warn(&e);
            }
        }
        Err(e) => crate::server::tracing_warn(&format!("Unterhaltung nicht speicherbar: {e}")),
    }
}

pub fn insert(s: Stored) {
    let mut m = map();
    write(&s);
    m.insert(s.conv.id.clone(), s);
}

/// Mutates a conversation (bumps version/updated time). `persist` writes it to disk.
pub fn update<R>(id: &str, persist: bool, f: impl FnOnce(&mut Stored) -> R) -> Option<R> {
    let mut m = map();
    let s = m.get_mut(id)?;
    let r = f(s);
    s.conv.version += 1;
    if persist {
        s.conv.updated_ms = now_ms();
        write(s);
    }
    Some(r)
}

pub fn read<R>(id: &str, f: impl FnOnce(&Stored) -> R) -> Option<R> {
    map().get(id).map(f)
}

pub fn get(id: &str) -> Option<Conversation> {
    read(id, |s| s.conv.clone())
}

pub fn list() -> Vec<ConvSummary> {
    let mut v: Vec<ConvSummary> = map()
        .values()
        .map(|s| ConvSummary {
            id: s.conv.id.clone(),
            title: s.conv.title.clone(),
            updated_ms: s.conv.updated_ms,
            state: s.conv.state,
            provider: s.conv.provider,
        })
        .collect();
    v.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms));
    v
}

pub fn rename(id: &str, title: &str) -> Result<(), String> {
    let title: String = title.trim().chars().take(80).collect();
    if title.is_empty() {
        return Err("Titel darf nicht leer sein".into());
    }
    update(id, true, |s| s.conv.title = title)
        .ok_or_else(|| "Unterhaltung nicht gefunden".to_string())
}

pub fn delete(id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("Ungültige Unterhaltung".into());
    }
    map().remove(id);
    match std::fs::remove_file(file(id)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Konnte Unterhaltung nicht löschen: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::agent::ToolView;
    use crate::server::agent::llm::Call;
    use serde_json::json;

    #[test]
    fn repair_closes_open_tool_calls() {
        let mut s = Stored::default();
        s.history = vec![
            Msg::User { text: "x".into() },
            Msg::Assistant {
                text: String::new(),
                calls: vec![
                    Call {
                        id: "a".into(),
                        name: "n".into(),
                        input: json!({}),
                        invalid: None,
                    },
                    Call {
                        id: "b".into(),
                        name: "n".into(),
                        input: json!({}),
                        invalid: None,
                    },
                ],
                raw: None,
            },
            Msg::ToolResults {
                results: vec![ToolResult {
                    id: "a".into(),
                    content: "ok".into(),
                    is_error: false,
                }],
            },
        ];
        s.conv.items.push(Item::Tool(ToolView {
            id: "b".into(),
            status: ToolStatus::Approval,
            ..Default::default()
        }));
        repair(&mut s, "Abgebrochen");
        let Msg::ToolResults { results } = &s.history[2] else {
            panic!()
        };
        assert_eq!(results.len(), 2);
        assert!(results[1].is_error && results[1].id == "b");
        let Item::Tool(t) = &s.conv.items[0] else {
            panic!()
        };
        assert_eq!(t.status, ToolStatus::Cancelled);

        // no results message yet + partial streamed text
        let mut s = Stored::default();
        s.history = vec![Msg::Assistant {
            text: String::new(),
            calls: vec![Call {
                id: "c".into(),
                name: "n".into(),
                input: json!({}),
                invalid: None,
            }],
            raw: None,
        }];
        s.conv.streaming = "Hallo".into();
        repair(&mut s, "x");
        assert!(matches!(&s.history[1], Msg::ToolResults { results } if results.len() == 1));
        assert!(matches!(&s.history[2], Msg::Assistant { text, .. } if text == "Hallo"));
    }

    #[test]
    fn ids_are_validated() {
        assert!(valid_id("a1B2"));
        assert!(!valid_id("../x"));
        assert!(!valid_id(""));
    }
}
