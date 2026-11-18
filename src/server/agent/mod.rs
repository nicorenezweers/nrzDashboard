//! Server side of the KI-Agent: settings, conversation store, LLM clients (Claude/Grok) and the
//! tool loop that drives the umbrelOS MCP tools.

pub mod llm;
pub mod runner;
pub mod settings;
pub mod store;

use crate::api::agent::{Conversation, Item, Mode, Provider, RunState};

/// Extra HTTP routes of this feature (merged into the main router, behind the auth middleware).
pub fn routes() -> axum::Router {
    axum::Router::new()
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Writes a file atomically with mode 0600 (creates the parent directory).
pub fn write_private(path: &std::path::Path, data: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("Konnte {} nicht anlegen: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("tmp");
    let fail = |e: std::io::Error| format!("Konnte {} nicht schreiben: {e}", path.display());
    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp).map_err(fail)?;
        f.write_all(data).map_err(fail)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path).map_err(fail)
}

fn new_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| char::from_digit(rng.gen_range(0..16), 16).unwrap_or('0'))
        .collect()
}

fn title_from(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("Neue Unterhaltung")
        .trim();
    let t: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        format!("{t}…")
    } else {
        t
    }
}

/// Adds a user message (new conversation when `id` is empty) and starts the agent.
pub fn send(id: &str, text: &str, provider: Provider, mode: Mode) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Nachricht ist leer".into());
    }
    if text.chars().count() > 20_000 {
        return Err("Nachricht ist zu lang (max. 20 000 Zeichen)".into());
    }
    if settings::key(provider).is_none() {
        return Err(format!(
            "Kein API-Schlüssel für {} hinterlegt – in den Agent-Einstellungen (Zahnrad) eintragen.",
            provider.label()
        ));
    }
    let id = if id.is_empty() {
        let now = now_ms();
        let id = new_id();
        store::insert(store::Stored {
            conv: Conversation {
                id: id.clone(),
                title: title_from(text),
                created_ms: now,
                updated_ms: now,
                provider,
                mode,
                model: settings::model(provider),
                version: 1,
                ..Default::default()
            },
            history: vec![],
        });
        id
    } else {
        id.to_string()
    };
    store::update(&id, true, |s| {
        if s.conv.state != RunState::Idle {
            return Err("Der Agent arbeitet noch – warte oder brich ab.".to_string());
        }
        s.conv.provider = provider;
        s.conv.mode = mode;
        s.conv.state = RunState::Running;
        s.conv.round = 0;
        s.conv.items.push(Item::User {
            text: text.to_string(),
        });
        s.history.push(llm::Msg::User {
            text: text.to_string(),
        });
        Ok(())
    })
    .ok_or_else(|| "Unterhaltung nicht gefunden".to_string())??;
    runner::start(&id);
    Ok(id)
}

/// Changes provider/mode of a conversation (mode changes apply to the running agent immediately).
pub fn set_options(id: &str, provider: Provider, mode: Mode) -> Result<(), String> {
    store::update(id, true, |s| {
        s.conv.provider = provider;
        s.conv.mode = mode;
    })
    .ok_or_else(|| "Unterhaltung nicht gefunden".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_shortened() {
        assert_eq!(title_from("\n  Hallo Welt  \nzweite"), "Hallo Welt");
        assert!(title_from(&"a".repeat(80)).ends_with('…'));
        assert_eq!(new_id().len(), 16);
    }
}
