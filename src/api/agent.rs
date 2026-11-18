//! KI-Agent: shared types for the UI and the server functions behind them.
//!
//! The agent loop itself runs on the server (`crate::server::agent`); the page only polls the
//! conversation state while the agent is working.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use super::err;
#[cfg(feature = "server")]
use crate::server::agent as srv;

// ---------- Types ----------

pub const DEFAULT_CLAUDE_MODEL: &str = "claude-opus-5-5";
pub const DEFAULT_GROK_MODEL: &str = "grok-4.7";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Claude,
    Grok,
}

impl Provider {
    pub const ALL: [Self; 2] = [Self::Claude, Self::Grok];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Grok => "Grok",
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Grok => "grok",
        }
    }

    pub fn parse(s: &str) -> Self {
        if s == "grok" {
            Self::Grok
        } else {
            Self::Claude
        }
    }
}

/// Permission mode, like Claude Code.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Critical tools need a confirmation (default).
    #[default]
    Ask,
    /// No confirmations.
    Auto,
    /// Critical tools are not offered at all.
    ReadOnly,
}

impl Mode {
    pub const ALL: [Self; 3] = [Self::Ask, Self::Auto, Self::ReadOnly];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ask => "Fragen",
            Self::Auto => "Automatisch",
            Self::ReadOnly => "Nur lesen",
        }
    }

    pub const fn hint(self) -> &'static str {
        match self {
            Self::Ask => {
                "Kritische Werkzeuge (löschen, stoppen, neu starten …) brauchen deine Bestätigung"
            }
            Self::Auto => "Der Agent führt alle Werkzeuge ohne Rückfrage aus",
            Self::ReadOnly => "Kritische Werkzeuge werden dem Agenten gar nicht angeboten",
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Auto => "auto",
            Self::ReadOnly => "read_only",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "auto" => Self::Auto,
            "read_only" => Self::ReadOnly,
            _ => Self::Ask,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    #[default]
    Idle,
    Running,
    /// Waiting for the user to allow/deny a tool call.
    Waiting,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    #[default]
    Pending,
    Approval,
    Running,
    Done,
    Error,
    Denied,
    Cancelled,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct ToolView {
    pub id: String,
    pub name: String,
    /// Pretty-printed JSON arguments.
    pub args: String,
    /// Result text, shortened for the UI.
    pub result: Option<String>,
    pub status: ToolStatus,
    pub duration_ms: Option<u64>,
    pub destructive: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    User { text: String },
    Assistant { text: String },
    Tool(ToolView),
    Notice { text: String, error: bool },
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Usage {
    #[cfg(feature = "server")]
    pub fn add(&mut self, o: &Self) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
    }

    pub const fn total_in(&self) -> u64 {
        self.input + self.cache_read + self.cache_write
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub provider: Provider,
    pub mode: Mode,
    /// Model used for the last request.
    pub model: String,
    pub state: RunState,
    pub items: Vec<Item>,
    /// Partial answer while the model is still writing.
    pub streaming: String,
    pub usage: Usage,
    /// Tools the user allowed for the whole conversation.
    pub allowed_tools: Vec<String>,
    /// Current tool round of the running turn.
    pub round: u32,
    /// Bumped on every change (lets the client skip unchanged polls).
    pub version: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct ConvSummary {
    pub id: String,
    pub title: String,
    pub updated_ms: u64,
    pub state: RunState,
    pub provider: Provider,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct AgentSettings {
    pub provider: Provider,
    pub mode: Mode,
    pub claude_model: String,
    pub grok_model: String,
    /// Masked key, `None` when no key is set.
    pub claude_key_hint: Option<String>,
    pub grok_key_hint: Option<String>,
    /// Key comes from the environment (`ANTHROPIC_API_KEY` / `XAI_API_KEY`).
    pub claude_key_env: bool,
    pub grok_key_env: bool,
}

impl AgentSettings {
    pub fn model(&self, p: Provider) -> &str {
        match p {
            Provider::Claude => &self.claude_model,
            Provider::Grok => &self.grok_model,
        }
    }

    pub const fn has_key(&self, p: Provider) -> bool {
        match p {
            Provider::Claude => self.claude_key_hint.is_some(),
            Provider::Grok => self.grok_key_hint.is_some(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct AgentSettingsInput {
    pub provider: Provider,
    pub mode: Mode,
    pub claude_model: String,
    pub grok_model: String,
    /// Empty keeps the stored key.
    pub claude_key: String,
    pub grok_key: String,
    pub clear_claude_key: bool,
    pub clear_grok_key: bool,
}

// ---------- Server functions ----------

#[server]
pub async fn agent_settings() -> Result<AgentSettings, ServerFnError> {
    Ok(srv::settings::view())
}

#[server]
pub async fn agent_save_settings(
    input: AgentSettingsInput,
) -> Result<AgentSettings, ServerFnError> {
    srv::settings::save(input).map_err(err)?;
    Ok(srv::settings::view())
}

#[server]
pub async fn agent_list() -> Result<Vec<ConvSummary>, ServerFnError> {
    Ok(srv::store::list())
}

/// Returns the conversation, or `None` when `version` is still current.
#[server]
pub async fn agent_get(id: String, version: u64) -> Result<Option<Conversation>, ServerFnError> {
    let conv = srv::store::get(&id).ok_or_else(|| err("Unterhaltung nicht gefunden"))?;
    Ok((conv.version != version || version == 0).then_some(conv))
}

/// Sends a message (an empty `id` starts a new conversation) and starts the agent. Returns the id.
#[server]
pub async fn agent_send(
    id: String,
    text: String,
    provider: Provider,
    mode: Mode,
) -> Result<String, ServerFnError> {
    srv::send(&id, &text, provider, mode).map_err(err)
}

#[server]
pub async fn agent_set_options(
    id: String,
    provider: Provider,
    mode: Mode,
) -> Result<(), ServerFnError> {
    srv::set_options(&id, provider, mode).map_err(err)
}

#[server]
pub async fn agent_cancel(id: String) -> Result<(), ServerFnError> {
    srv::runner::cancel(&id);
    Ok(())
}

/// `decision`: `allow`, `always` or `deny`.
#[server]
pub async fn agent_decide(
    id: String,
    call_id: String,
    decision: String,
) -> Result<(), ServerFnError> {
    srv::runner::decide(&id, &call_id, &decision).map_err(err)
}

#[server]
pub async fn agent_rename(id: String, title: String) -> Result<(), ServerFnError> {
    srv::store::rename(&id, &title).map_err(err)
}

#[server]
pub async fn agent_delete(id: String) -> Result<(), ServerFnError> {
    srv::runner::cancel(&id);
    srv::store::delete(&id).map_err(err)
}
