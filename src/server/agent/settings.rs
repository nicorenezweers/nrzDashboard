//! Agent settings (`$DATA_DIR/agent.json`, 0600): provider, mode, API keys and models.
//! Keys never leave the server – the UI only gets a masked hint.

use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use super::write_private;
use crate::api::agent::{
    AgentSettings, AgentSettingsInput, DEFAULT_CLAUDE_MODEL, DEFAULT_GROK_MODEL, Mode, Provider,
};
use crate::server::{config, env};

#[derive(Clone, Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    provider: Option<Provider>,
    #[serde(default)]
    mode: Option<Mode>,
    #[serde(default)]
    claude_key: String,
    #[serde(default)]
    claude_model: String,
    #[serde(default)]
    grok_key: String,
    #[serde(default)]
    grok_model: String,
}

fn file() -> std::path::PathBuf {
    config().data_dir.join("agent.json")
}

fn store() -> &'static RwLock<Saved> {
    static S: OnceLock<RwLock<Saved>> = OnceLock::new();
    S.get_or_init(|| {
        let saved = std::fs::read_to_string(file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        RwLock::new(saved)
    })
}

fn saved() -> Saved {
    store().read().map(|s| s.clone()).unwrap_or_default()
}

const fn env_name(p: Provider) -> &'static str {
    match p {
        Provider::Claude => "ANTHROPIC_API_KEY",
        Provider::Grok => "XAI_API_KEY",
    }
}

/// API key for a provider: saved key, otherwise the environment variable.
pub fn key(p: Provider) -> Option<String> {
    let s = saved();
    let k = match p {
        Provider::Claude => s.claude_key,
        Provider::Grok => s.grok_key,
    };
    Some(k)
        .filter(|k| !k.trim().is_empty())
        .or_else(|| env(env_name(p)))
        .map(|k| k.trim().to_string())
}

pub fn model(p: Provider) -> String {
    let s = saved();
    match p {
        Provider::Claude => Some(s.claude_model)
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_CLAUDE_MODEL.into()),
        Provider::Grok => Some(s.grok_model)
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_GROK_MODEL.into()),
    }
}

pub fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() > 16 {
        let head: String = chars.iter().take(7).collect();
        let tail: String = chars.iter().skip(chars.len() - 4).collect();
        format!("{head}…{tail}")
    } else {
        "••••••••".into()
    }
}

pub fn view() -> AgentSettings {
    let s = saved();
    let claude = key(Provider::Claude);
    let grok = key(Provider::Grok);
    let provider = s.provider.unwrap_or(if claude.is_none() && grok.is_some() {
        Provider::Grok
    } else {
        Provider::Claude
    });
    AgentSettings {
        provider,
        mode: s.mode.unwrap_or_default(),
        claude_model: model(Provider::Claude),
        grok_model: model(Provider::Grok),
        claude_key_hint: claude.as_deref().map(mask),
        grok_key_hint: grok.as_deref().map(mask),
        claude_key_env: s.claude_key.trim().is_empty() && claude.is_some(),
        grok_key_env: s.grok_key.trim().is_empty() && grok.is_some(),
    }
}

fn clean_model(m: &str) -> Result<String, String> {
    let m = m.trim();
    if m.len() > 100 || m.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("Ungültiger Modellname".into());
    }
    Ok(m.to_string())
}

fn clean_key(k: &str) -> Result<String, String> {
    let k = k.trim();
    if k.len() > 400 || k.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("Ungültiger API-Schlüssel".into());
    }
    Ok(k.to_string())
}

pub fn save(input: AgentSettingsInput) -> Result<(), String> {
    let mut cur = store().write().map_err(|_| "Interner Fehler".to_string())?;
    let mut next = cur.clone();
    next.provider = Some(input.provider);
    next.mode = Some(input.mode);
    next.claude_model = clean_model(&input.claude_model)?;
    next.grok_model = clean_model(&input.grok_model)?;
    if input.clear_claude_key {
        next.claude_key.clear();
    }
    if input.clear_grok_key {
        next.grok_key.clear();
    }
    let ck = clean_key(&input.claude_key)?;
    if !ck.is_empty() {
        next.claude_key = ck;
    }
    let gk = clean_key(&input.grok_key)?;
    if !gk.is_empty() {
        next.grok_key = gk;
    }
    let json = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
    write_private(&file(), json.as_bytes())?;
    *cur = next;
    Ok(())
}
