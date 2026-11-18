//! Provider-neutral chat history plus the two wire formats:
//! Anthropic Messages API (raw HTTP + SSE) and xAI Chat Completions (OpenAI-compatible, SSE).

use std::collections::BTreeMap;
use std::time::Duration;

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::api::agent::{Provider, Usage};
use crate::server::http;

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// Models that accept server-side refusal fallbacks (`fallbacks: "default"`).
const FALLBACK_MODELS: &[&str] = &[
    "claude-fable-5-2",
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5-5",
];
const XAI_URL: &str = "https://api.x.ai/v1/chat/completions";
const MAX_TOKENS: u32 = 32_000;
const CHUNK_TIMEOUT: Duration = Duration::from_secs(180);

// ---------- Neutral history ----------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Call {
    pub id: String,
    pub name: String,
    pub input: Value,
    /// Raw argument text when the model sent invalid JSON (the call is not executed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalid: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub id: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Msg {
    User {
        text: String,
    },
    Assistant {
        text: String,
        #[serde(default)]
        calls: Vec<Call>,
        /// Anthropic content blocks exactly as received (incl. thinking blocks), echoed back unchanged.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw: Option<Vec<Value>>,
    },
    ToolResults {
        results: Vec<ToolResult>,
    },
}

/// A tool as offered to the model (taken 1:1 from the MCP server).
#[derive(Clone, Debug)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

impl ToolDef {
    pub fn from_mcp(t: &Value) -> Option<Self> {
        let name = t.get("name")?.as_str()?.to_string();
        let description = t
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut schema = t
            .get("inputSchema")
            .cloned()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        if let Some(o) = schema.as_object_mut() {
            o.remove("$schema");
            o.entry("type").or_insert_with(|| json!("object"));
            if o.get("type").and_then(Value::as_str) == Some("object") {
                o.entry("properties").or_insert_with(|| json!({}));
            }
        }
        Some(Self {
            name,
            description,
            schema,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    End,
    ToolUse,
    MaxTokens,
    Refusal(Option<String>),
    Other(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    pub text: String,
    pub calls: Vec<Call>,
    pub raw: Option<Vec<Value>>,
    pub stop: Stop,
    pub usage: Usage,
}

pub struct Request<'a> {
    pub provider: Provider,
    pub key: &'a str,
    pub model: &'a str,
    pub system: &'a str,
    pub tools: &'a [ToolDef],
    pub history: &'a [Msg],
}

// ---------- Anthropic: request ----------

/// Tool ids must match `^[a-zA-Z0-9_-]+$` (ids from xAI may differ).
fn anthropic_id(id: &str) -> String {
    let s: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "call".into() } else { s }
}

fn non_empty(s: &str) -> &str {
    if s.is_empty() { "(leer)" } else { s }
}

pub fn anthropic_messages(history: &[Msg]) -> Vec<Value> {
    let mut out: Vec<(String, Vec<Value>)> = Vec::new();
    for m in history {
        let (role, blocks): (&str, Vec<Value>) = match m {
            Msg::User { text } => (
                "user",
                vec![json!({ "type": "text", "text": non_empty(text) })],
            ),
            Msg::Assistant { text, calls, raw } => {
                let blocks = match raw {
                    Some(raw) if !raw.is_empty() => raw.clone(),
                    _ => {
                        let mut b = Vec::new();
                        if !text.trim().is_empty() {
                            b.push(json!({ "type": "text", "text": text }));
                        }
                        for c in calls {
                            b.push(json!({ "type": "tool_use", "id": anthropic_id(&c.id), "name": c.name, "input": c.input }));
                        }
                        b
                    }
                };
                ("assistant", blocks)
            }
            Msg::ToolResults { results } => (
                "user",
                results
                    .iter()
                    .map(|r| {
                        json!({
                            "type": "tool_result",
                            "tool_use_id": anthropic_id(&r.id),
                            "content": non_empty(&r.content),
                            "is_error": r.is_error,
                        })
                    })
                    .collect(),
            ),
        };
        if blocks.is_empty() {
            continue;
        }
        match out.last_mut() {
            // Merge consecutive turns of the same role (e.g. tool results followed by a new user message).
            Some((r, b)) if r == role => b.extend(blocks),
            _ => out.push((role.to_string(), blocks)),
        }
    }
    out.into_iter()
        .map(|(role, content)| json!({ "role": role, "content": content }))
        .collect()
}

pub fn anthropic_body(model: &str, system: &str, tools: &[ToolDef], history: &[Msg]) -> Value {
    let tools: Vec<Value> = tools
        .iter()
        .map(|t| {
            let mut o = Map::new();
            o.insert("name".into(), json!(t.name));
            if !t.description.is_empty() {
                o.insert("description".into(), json!(t.description));
            }
            o.insert("input_schema".into(), t.schema.clone());
            o.insert("eager_input_streaming".into(), json!(true));
            Value::Object(o)
        })
        .collect();
    let mut body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "system": system,
        "messages": anthropic_messages(history),
        "cache_control": { "type": "ephemeral" },
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
    if uses_fallback(model) {
        body["fallbacks"] = json!("default");
    }
    body
}

fn uses_fallback(model: &str) -> bool {
    FALLBACK_MODELS.contains(&model)
}

// ---------- Anthropic: stream accumulator ----------

#[derive(Default)]
pub struct AnthropicAcc {
    blocks: BTreeMap<u64, Value>,
    partial_json: BTreeMap<u64, String>,
    invalid: BTreeMap<u64, String>,
    stop: Option<String>,
    refusal_category: Option<String>,
    usage: Usage,
    done: bool,
}

impl AnthropicAcc {
    /// Applies one SSE `data` payload; returns new answer text (for live display).
    pub fn apply(&mut self, v: &Value) -> Result<Option<String>, String> {
        let idx = v.get("index").and_then(Value::as_u64).unwrap_or(0);
        match v.get("type").and_then(Value::as_str).unwrap_or_default() {
            "message_start" => {
                if let Some(u) = v.pointer("/message/usage") {
                    self.read_usage(u);
                }
            }
            "content_block_start" => {
                let block = v.get("content_block").cloned().unwrap_or_else(|| json!({}));
                let text = (block.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| {
                        block
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    })
                    .filter(|t| !t.is_empty());
                self.blocks.insert(idx, block);
                return Ok(text);
            }
            "content_block_delta" => {
                let delta = v.get("delta").cloned().unwrap_or_default();
                let Some(block) = self.blocks.get_mut(&idx) else {
                    return Ok(None);
                };
                let s = |k: &str| {
                    delta
                        .get(k)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                match delta
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                {
                    "text_delta" => {
                        let t = s("text");
                        append(block, "text", &t);
                        return Ok(Some(t).filter(|t| !t.is_empty()));
                    }
                    "thinking_delta" => append(block, "thinking", &s("thinking")),
                    "signature_delta" => block["signature"] = json!(s("signature")),
                    "input_json_delta" => self
                        .partial_json
                        .entry(idx)
                        .or_default()
                        .push_str(&s("partial_json")),
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let Some(block) = self.blocks.get_mut(&idx)
                    && block.get("type").and_then(Value::as_str) == Some("tool_use")
                {
                    let raw = self.partial_json.remove(&idx).unwrap_or_default();
                    if raw.trim().is_empty() {
                        if !block.get("input").is_some_and(Value::is_object) {
                            block["input"] = json!({});
                        }
                    } else {
                        match serde_json::from_str::<Value>(&raw) {
                            Ok(input) if input.is_object() => block["input"] = input,
                            _ => {
                                block["input"] = json!({});
                                self.invalid.insert(idx, raw);
                            }
                        }
                    }
                }
            }
            "message_delta" => {
                if let Some(r) = v.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop = Some(r.to_string());
                }
                if let Some(c) = v
                    .pointer("/delta/stop_details/category")
                    .and_then(Value::as_str)
                {
                    self.refusal_category = Some(c.to_string());
                }
                if let Some(u) = v.get("usage") {
                    self.read_usage(u);
                }
            }
            "message_stop" => self.done = true,
            "error" => return Err(anthropic_error_text(None, v)),
            _ => {}
        }
        Ok(None)
    }

    /// Usage fields are cumulative; later events overwrite earlier values.
    fn read_usage(&mut self, u: &Value) {
        let get = |k: &str| u.get(k).and_then(Value::as_u64);
        if let Some(n) = get("input_tokens") {
            self.usage.input = n;
        }
        if let Some(n) = get("output_tokens") {
            self.usage.output = n;
        }
        if let Some(n) = get("cache_read_input_tokens") {
            self.usage.cache_read = n;
        }
        if let Some(n) = get("cache_creation_input_tokens") {
            self.usage.cache_write = n;
        }
    }

    pub fn finish(self) -> Result<Turn, String> {
        if !self.done && self.stop.is_none() {
            return Err("Verbindung zur Claude-API vorzeitig beendet".into());
        }
        let mut invalid = self.invalid;
        let blocks: Vec<(Option<String>, Value)> = self
            .blocks
            .into_iter()
            .map(|(i, b)| (invalid.remove(&i), b))
            .collect();
        let blocks = sanitize_fallback(blocks);
        let text = blocks
            .iter()
            .filter(|(_, b)| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|(_, b)| b.get("text").and_then(Value::as_str))
            .filter(|t| !t.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        let calls = blocks
            .iter()
            .filter(|(_, b)| b.get("type").and_then(Value::as_str) == Some("tool_use"))
            .map(|(inv, b)| Call {
                id: b
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                name: b
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                input: b.get("input").cloned().unwrap_or_else(|| json!({})),
                invalid: inv.clone(),
            })
            .collect();
        let stop = match self.stop.as_deref() {
            Some("end_turn" | "stop_sequence") | None => Stop::End,
            Some("tool_use") => Stop::ToolUse,
            Some("max_tokens") => Stop::MaxTokens,
            Some("refusal") => Stop::Refusal(self.refusal_category),
            Some(o) => Stop::Other(o.to_string()),
        };
        Ok(Turn {
            text,
            calls,
            raw: Some(blocks.into_iter().map(|(_, b)| b).collect()),
            stop,
            usage: self.usage,
        })
    }
}

fn append(block: &mut Value, key: &str, s: &str) {
    let cur = block.get(key).and_then(Value::as_str).unwrap_or_default();
    block[key] = json!(format!("{cur}{s}"));
}

/// After a mid-output refusal fallback, blocks before the last `fallback` block that are not text
/// (thinking, tool_use, unknown model-internal blocks) must not be echoed back or executed.
fn sanitize_fallback(blocks: Vec<(Option<String>, Value)>) -> Vec<(Option<String>, Value)> {
    let ty = |b: &Value| {
        b.get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let Some(boundary) = blocks.iter().rposition(|(_, b)| ty(b) == "fallback") else {
        return blocks;
    };
    blocks
        .into_iter()
        .enumerate()
        .filter(|(i, (_, b))| *i >= boundary || matches!(ty(b).as_str(), "text" | "fallback"))
        .map(|(_, b)| b)
        .collect()
}

fn anthropic_error_text(status: Option<reqwest::StatusCode>, v: &Value) -> String {
    let kind = v
        .pointer("/error/type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let msg = v
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let code = status.map(|s| s.as_u16());
    let hint = match (code, kind) {
        (Some(401), _) | (_, "authentication_error") => {
            "API-Schlüssel ungültig – in den Agent-Einstellungen prüfen"
        }
        (Some(403), _) | (_, "permission_error") => {
            "Keine Berechtigung für dieses Modell oder diese Funktion"
        }
        (Some(404), _) | (_, "not_found_error") => {
            "Modell nicht gefunden – Modellnamen in den Einstellungen prüfen"
        }
        (Some(429), _) | (_, "rate_limit_error") => "Rate-Limit erreicht – bitte kurz warten",
        (Some(529), _) | (_, "overloaded_error") => {
            "Claude ist gerade überlastet – bitte später erneut versuchen"
        }
        (Some(402), _) | (_, "billing_error") => "Abrechnungsproblem beim Anthropic-Konto",
        _ => "",
    };
    let status = code.map(|c| format!(" (HTTP {c})")).unwrap_or_default();
    match (hint.is_empty(), msg.is_empty()) {
        (false, false) => format!("Claude: {hint}{status} – {msg}"),
        (false, true) => format!("Claude: {hint}{status}"),
        (true, false) => format!("Claude-Fehler{status}: {msg}"),
        (true, true) => format!("Claude-Fehler{status}"),
    }
}

// ---------- xAI: request ----------

pub fn xai_messages(system: &str, history: &[Msg]) -> Vec<Value> {
    let mut out = vec![json!({ "role": "system", "content": system })];
    for m in history {
        match m {
            Msg::User { text } => out.push(json!({ "role": "user", "content": non_empty(text) })),
            Msg::Assistant { text, calls, .. } => {
                if text.trim().is_empty() && calls.is_empty() {
                    continue;
                }
                let mut msg = json!({ "role": "assistant", "content": if text.trim().is_empty() { Value::Null } else { json!(text) } });
                if !calls.is_empty() {
                    msg["tool_calls"] = calls
                        .iter()
                        .map(|c| {
                            json!({
                                "id": c.id,
                                "type": "function",
                                "function": { "name": c.name, "arguments": c.input.to_string() },
                            })
                        })
                        .collect();
                }
                out.push(msg);
            }
            Msg::ToolResults { results } => {
                for r in results {
                    out.push(json!({ "role": "tool", "tool_call_id": r.id, "content": non_empty(&r.content) }));
                }
            }
        }
    }
    out
}

pub fn xai_body(model: &str, system: &str, tools: &[ToolDef], history: &[Msg]) -> Value {
    let mut body = json!({
        "model": model,
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": xai_messages(system, history),
    });
    if !tools.is_empty() {
        body["tools"] = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": { "name": t.name, "description": t.description, "parameters": t.schema },
                })
            })
            .collect();
        body["tool_choice"] = json!("auto");
    }
    body
}

// ---------- xAI: stream accumulator ----------

#[derive(Default)]
struct PartialCall {
    id: String,
    name: String,
    args: String,
}

#[derive(Default)]
pub struct XaiAcc {
    text: String,
    calls: BTreeMap<u64, PartialCall>,
    finish: Option<String>,
    usage: Usage,
    done: bool,
}

impl XaiAcc {
    pub fn apply(&mut self, v: &Value) -> Result<Option<String>, String> {
        if let Some(e) = v.get("error") {
            return Err(xai_error_text(None, e));
        }
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            let get = |p: &str| u.pointer(p).and_then(Value::as_u64).unwrap_or(0);
            let cached = get("/prompt_tokens_details/cached_tokens");
            self.usage.input = get("/prompt_tokens").saturating_sub(cached);
            self.usage.cache_read = cached;
            self.usage.output = get("/completion_tokens");
        }
        let Some(choice) = v.pointer("/choices/0") else {
            return Ok(None);
        };
        if let Some(f) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish = Some(f.to_string());
            self.done = true;
        }
        let delta = choice.get("delta").or_else(|| choice.get("message"));
        let Some(delta) = delta else { return Ok(None) };
        if let Some(tcs) = delta.get("tool_calls").and_then(Value::as_array) {
            for (pos, tc) in tcs.iter().enumerate() {
                let i = tc
                    .get("index")
                    .and_then(Value::as_u64)
                    .unwrap_or(pos as u64);
                let p = self.calls.entry(i).or_default();
                if let Some(id) = tc
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    p.id = id.to_string();
                }
                if let Some(n) = tc
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    p.name = n.to_string();
                }
                match tc.pointer("/function/arguments") {
                    Some(Value::String(a)) => p.args.push_str(a),
                    Some(o @ Value::Object(_)) => p.args = o.to_string(),
                    _ => {}
                }
            }
        }
        let t = delta
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if t.is_empty() {
            return Ok(None);
        }
        self.text.push_str(t);
        Ok(Some(t.to_string()))
    }

    pub fn finish(self) -> Result<Turn, String> {
        if !self.done && self.text.is_empty() && self.calls.is_empty() {
            return Err("Verbindung zur xAI-API vorzeitig beendet".into());
        }
        let calls: Vec<Call> = self
            .calls
            .into_values()
            .enumerate()
            .map(|(i, p)| {
                let (input, invalid) = if p.args.trim().is_empty() {
                    (json!({}), None)
                } else {
                    match serde_json::from_str::<Value>(&p.args) {
                        Ok(v) if v.is_object() => (v, None),
                        _ => (json!({}), Some(p.args)),
                    }
                };
                Call {
                    id: if p.id.is_empty() {
                        format!("call_{i}")
                    } else {
                        p.id
                    },
                    name: p.name,
                    input,
                    invalid,
                }
            })
            .collect();
        let stop = match self.finish.as_deref() {
            Some("tool_calls") => Stop::ToolUse,
            Some("length") => Stop::MaxTokens,
            Some("content_filter") => Stop::Refusal(None),
            Some("stop") | None => {
                if calls.is_empty() {
                    Stop::End
                } else {
                    Stop::ToolUse
                }
            }
            Some(o) => Stop::Other(o.to_string()),
        };
        Ok(Turn {
            text: self.text.trim().to_string(),
            calls,
            raw: None,
            stop,
            usage: self.usage,
        })
    }
}

fn xai_error_text(status: Option<reqwest::StatusCode>, e: &Value) -> String {
    let msg = match e {
        Value::String(s) => s.clone(),
        o => o
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| o.to_string()),
    };
    let code = status.map(|s| s.as_u16());
    let hint = match code {
        Some(400) if msg.to_lowercase().contains("api key") => {
            "API-Schlüssel ungültig – in den Agent-Einstellungen prüfen"
        }
        Some(401 | 403) => "API-Schlüssel ungültig oder ohne Berechtigung",
        Some(404) => "Modell nicht gefunden – Modellnamen in den Einstellungen prüfen",
        Some(429) => "Rate-Limit erreicht oder Guthaben aufgebraucht",
        _ => "",
    };
    let status = code.map(|c| format!(" (HTTP {c})")).unwrap_or_default();
    let msg: String = msg.chars().take(300).collect();
    if hint.is_empty() {
        format!("Grok-Fehler{status}: {msg}")
    } else {
        format!("Grok: {hint}{status} – {msg}")
    }
}

// ---------- SSE ----------

/// Incremental Server-Sent-Events parser; yields the `data` payload of each complete event.
#[derive(Default)]
pub struct Sse {
    buf: Vec<u8>,
}

impl Sse {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf
            .extend(bytes.iter().copied().filter(|b| *b != b'\r'));
        let mut out = Vec::new();
        while let Some(pos) = self.buf.windows(2).position(|w| w == b"\n\n") {
            let event: Vec<u8> = self.buf.drain(..pos + 2).collect();
            let text = String::from_utf8_lossy(&event);
            let data: Vec<&str> = text
                .lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .map(|d| d.strip_prefix(' ').unwrap_or(d))
                .collect();
            if !data.is_empty() {
                out.push(data.join("\n"));
            }
        }
        out
    }
}

// ---------- HTTP ----------

enum Acc {
    Anthropic(AnthropicAcc),
    Xai(XaiAcc),
}

impl Acc {
    fn apply(&mut self, v: &Value) -> Result<Option<String>, String> {
        match self {
            Self::Anthropic(a) => a.apply(v),
            Self::Xai(a) => a.apply(v),
        }
    }

    fn finish(self) -> Result<Turn, String> {
        match self {
            Self::Anthropic(a) => a.finish(),
            Self::Xai(a) => a.finish(),
        }
    }
}

async fn send(req: &Request<'_>) -> Result<reqwest::Response, String> {
    let name = req.provider.label();
    let mut attempt = 0;
    loop {
        attempt += 1;
        let builder = match req.provider {
            Provider::Claude => {
                let mut b = http()
                    .post(ANTHROPIC_URL)
                    .header("x-api-key", req.key)
                    .header("anthropic-version", ANTHROPIC_VERSION)
                    .json(&anthropic_body(
                        req.model,
                        req.system,
                        req.tools,
                        req.history,
                    ));
                if uses_fallback(req.model) {
                    b = b.header("anthropic-beta", FALLBACK_BETA);
                }
                b
            }
            Provider::Grok => http().post(XAI_URL).bearer_auth(req.key).json(&xai_body(
                req.model,
                req.system,
                req.tools,
                req.history,
            )),
        };
        let res = builder.header("accept", "text/event-stream").send().await;
        let retry_in = Duration::from_secs(2u64.pow(attempt));
        match res {
            Ok(r) if r.status().is_success() => return Ok(r),
            Ok(r) => {
                let status = r.status();
                let retryable = matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504 | 529);
                if retryable && attempt < 3 {
                    tokio::time::sleep(retry_in).await;
                    continue;
                }
                let text = r.text().await.unwrap_or_default();
                let v: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "error": { "message": text.chars().take(300).collect::<String>() } }));
                return Err(match req.provider {
                    Provider::Claude => anthropic_error_text(Some(status), &v),
                    Provider::Grok => xai_error_text(Some(status), v.get("error").unwrap_or(&v)),
                });
            }
            Err(e) => {
                if attempt < 3 && (e.is_connect() || e.is_timeout()) {
                    tokio::time::sleep(retry_in).await;
                    continue;
                }
                return Err(format!("{name}-API nicht erreichbar: {e}"));
            }
        }
    }
}

/// Runs one model turn with streaming; `on_text` receives answer text as it arrives.
pub async fn stream_turn(
    req: &Request<'_>,
    on_text: &mut (dyn FnMut(&str) + Send),
) -> Result<Turn, String> {
    let res = send(req).await?;
    let mut acc = match req.provider {
        Provider::Claude => Acc::Anthropic(AnthropicAcc::default()),
        Provider::Grok => Acc::Xai(XaiAcc::default()),
    };
    let mut sse = Sse::default();
    let mut stream = res.bytes_stream();
    loop {
        let chunk = match tokio::time::timeout(CHUNK_TIMEOUT, stream.next()).await {
            Err(_) => {
                return Err(format!(
                    "{}-API antwortet nicht mehr (Zeitüberschreitung)",
                    req.provider.label()
                ));
            }
            Ok(None) => break,
            Ok(Some(Err(e))) => {
                return Err(format!(
                    "Verbindung zur {}-API unterbrochen: {e}",
                    req.provider.label()
                ));
            }
            Ok(Some(Ok(b))) => b,
        };
        for data in sse.push(&chunk) {
            if data.trim() == "[DONE]" {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            if let Some(t) = acc.apply(&v)? {
                on_text(&t);
            }
        }
    }
    acc.finish()
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> Vec<Msg> {
        vec![
            Msg::User {
                text: "Welche Apps laufen?".into(),
            },
            Msg::Assistant {
                text: "Ich sehe nach.".into(),
                calls: vec![Call {
                    id: "call.1".into(),
                    name: "list_apps".into(),
                    input: json!({}),
                    invalid: None,
                }],
                raw: None,
            },
            Msg::ToolResults {
                results: vec![ToolResult {
                    id: "call.1".into(),
                    content: "[]".into(),
                    is_error: false,
                }],
            },
            Msg::User {
                text: "Danke".into(),
            },
        ]
    }

    fn tools() -> Vec<ToolDef> {
        vec![ToolDef::from_mcp(&json!({
            "name": "get_app_logs",
            "description": "Logs einer App",
            "inputSchema": { "$schema": "http://json-schema.org/draft-07/schema#", "type": "object", "properties": { "appId": { "type": "string" } }, "required": ["appId"] }
        }))
        .unwrap()]
    }

    #[test]
    fn tool_def_strips_schema_key() {
        let t = &tools()[0];
        assert!(t.schema.get("$schema").is_none());
        assert_eq!(t.schema["required"], json!(["appId"]));
        let bare = ToolDef::from_mcp(&json!({ "name": "x" })).unwrap();
        assert_eq!(bare.schema, json!({ "type": "object", "properties": {} }));
    }

    #[test]
    fn anthropic_request_format() {
        let body = anthropic_body("claude-opus-5-5", "SYS", &tools(), &history());
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["system"], "SYS");
        assert_eq!(body["fallbacks"], "default");
        assert_eq!(body["tools"][0]["name"], "get_app_logs");
        assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(body["tools"][0]["eager_input_streaming"], true);
        let msgs = body["messages"].as_array().unwrap();
        // tool results and the following user message are merged into one user turn
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[1]["role"], "assistant");
        assert_eq!(msgs[1]["content"][1]["type"], "tool_use");
        assert_eq!(msgs[1]["content"][1]["id"], "call_1");
        assert_eq!(msgs[2]["role"], "user");
        assert_eq!(msgs[2]["content"][0]["type"], "tool_result");
        assert_eq!(msgs[2]["content"][0]["tool_use_id"], "call_1");
        assert_eq!(msgs[2]["content"][1]["text"], "Danke");
        // other models: no fallback parameter
        assert!(
            anthropic_body("claude-haiku-4-5", "S", &[], &history())
                .get("fallbacks")
                .is_none()
        );
    }

    #[test]
    fn anthropic_raw_blocks_are_echoed_unchanged() {
        let raw = vec![
            json!({ "type": "thinking", "thinking": "", "signature": "sig" }),
            json!({ "type": "text", "text": "Hi" }),
        ];
        let h = vec![
            Msg::User { text: "a".into() },
            Msg::Assistant {
                text: "Hi".into(),
                calls: vec![],
                raw: Some(raw.clone()),
            },
        ];
        let msgs = anthropic_messages(&h);
        assert_eq!(msgs[1]["content"], Value::Array(raw));
    }

    #[test]
    fn anthropic_stream_with_tool_use() {
        let events = [
            json!({"type":"message_start","message":{"id":"msg_1","model":"claude-opus-5-5","usage":{"input_tokens":100,"cache_read_input_tokens":50,"output_tokens":1}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"abc"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Ich "}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"prüfe."}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"get_app_logs","input":{}}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"appId\":"}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"\"nrzgit\"}"}}),
            json!({"type":"content_block_stop","index":2}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":42}}),
            json!({"type":"message_stop"}),
        ];
        let mut acc = AnthropicAcc::default();
        let mut streamed = String::new();
        for e in &events {
            if let Some(t) = acc.apply(e).unwrap() {
                streamed.push_str(&t);
            }
        }
        assert_eq!(streamed, "Ich prüfe.");
        let turn = acc.finish().unwrap();
        assert_eq!(turn.text, "Ich prüfe.");
        assert_eq!(turn.stop, Stop::ToolUse);
        assert_eq!(turn.calls.len(), 1);
        assert_eq!(turn.calls[0].id, "toolu_1");
        assert_eq!(turn.calls[0].input, json!({ "appId": "nrzgit" }));
        assert_eq!(
            turn.usage,
            Usage {
                input: 100,
                output: 42,
                cache_read: 50,
                cache_write: 0
            }
        );
        let raw = turn.raw.unwrap();
        assert_eq!(
            raw[0],
            json!({ "type": "thinking", "thinking": "", "signature": "abc" })
        );
        assert_eq!(raw[2]["input"], json!({ "appId": "nrzgit" }));
    }

    #[test]
    fn anthropic_invalid_tool_json_and_errors() {
        let mut acc = AnthropicAcc::default();
        acc.apply(&json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t","name":"x","input":{}}})).unwrap();
        acc.apply(&json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"a\": tru"}})).unwrap();
        acc.apply(&json!({"type":"content_block_stop","index":0}))
            .unwrap();
        acc.apply(&json!({"type":"message_delta","delta":{"stop_reason":"max_tokens"}}))
            .unwrap();
        let turn = acc.finish().unwrap();
        assert_eq!(turn.stop, Stop::MaxTokens);
        assert_eq!(turn.calls[0].invalid.as_deref(), Some("{\"a\": tru"));
        assert_eq!(turn.calls[0].input, json!({}));

        let mut acc = AnthropicAcc::default();
        let e = acc
            .apply(
                &json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}),
            )
            .unwrap_err();
        assert!(e.contains("überlastet"), "{e}");
        assert!(AnthropicAcc::default().finish().is_err());
    }

    #[test]
    fn anthropic_refusal_and_fallback_boundary() {
        let events = [
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":"s"}}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t0","name":"x","input":{}}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"content_block_start","index":2,"content_block":{"type":"text","text":"Teil"}}),
            json!({"type":"content_block_start","index":3,"content_block":{"type":"fallback","from":{"model":"a"},"to":{"model":"b"}}}),
            json!({"type":"content_block_start","index":4,"content_block":{"type":"text","text":"Weiter"}}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}),
        ];
        let mut acc = AnthropicAcc::default();
        for e in &events {
            acc.apply(e).unwrap();
        }
        let turn = acc.finish().unwrap();
        let types: Vec<&str> = turn
            .raw
            .as_ref()
            .unwrap()
            .iter()
            .map(|b| b["type"].as_str().unwrap())
            .collect();
        assert_eq!(types, ["text", "fallback", "text"]);
        assert!(turn.calls.is_empty());
        assert_eq!(turn.text, "Teil\n\nWeiter");

        let mut acc = AnthropicAcc::default();
        acc.apply(&json!({"type":"message_delta","delta":{"stop_reason":"refusal","stop_details":{"type":"refusal","category":"cyber"}}})).unwrap();
        assert_eq!(
            acc.finish().unwrap().stop,
            Stop::Refusal(Some("cyber".into()))
        );
    }

    #[test]
    fn xai_request_format() {
        let body = xai_body("grok-4.7", "SYS", &tools(), &history());
        assert_eq!(body["model"], "grok-4.7");
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "get_app_logs");
        assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0], json!({ "role": "system", "content": "SYS" }));
        assert_eq!(msgs[2]["role"], "assistant");
        assert_eq!(msgs[2]["tool_calls"][0]["id"], "call.1");
        assert_eq!(msgs[2]["tool_calls"][0]["function"]["arguments"], "{}");
        assert_eq!(
            msgs[3],
            json!({ "role": "tool", "tool_call_id": "call.1", "content": "[]" })
        );
        assert_eq!(msgs[4], json!({ "role": "user", "content": "Danke" }));
    }

    #[test]
    fn xai_stream_with_tool_calls() {
        let events = [
            json!({"choices":[{"index":0,"delta":{"role":"assistant","content":"Moment"}}]}),
            json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"get_app_logs","arguments":"{\"appId\""}}]}}]}),
            json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\"nrzgit\"}"}}]}}]}),
            json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"id":"call_b","function":{"name":"list_apps","arguments":"{}"}}]}}]}),
            json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
            json!({"choices":[],"usage":{"prompt_tokens":200,"completion_tokens":30,"prompt_tokens_details":{"cached_tokens":120}}}),
        ];
        let mut acc = XaiAcc::default();
        let mut streamed = String::new();
        for e in &events {
            if let Some(t) = acc.apply(e).unwrap() {
                streamed.push_str(&t);
            }
        }
        assert_eq!(streamed, "Moment");
        let turn = acc.finish().unwrap();
        assert_eq!(turn.stop, Stop::ToolUse);
        assert_eq!(turn.calls.len(), 2);
        assert_eq!(
            turn.calls[0],
            Call {
                id: "call_a".into(),
                name: "get_app_logs".into(),
                input: json!({"appId":"nrzgit"}),
                invalid: None
            }
        );
        assert_eq!(turn.calls[1].name, "list_apps");
        assert_eq!(
            turn.usage,
            Usage {
                input: 80,
                output: 30,
                cache_read: 120,
                cache_write: 0
            }
        );
        assert!(turn.raw.is_none());

        let mut acc = XaiAcc::default();
        assert!(
            acc.apply(&json!({"error":"Incorrect API key provided"}))
                .unwrap_err()
                .contains("Incorrect API key")
        );
    }

    #[test]
    fn sse_parser_handles_split_chunks_and_crlf() {
        let mut sse = Sse::default();
        assert!(sse.push(b"event: ping\r\ndata: {\"a\"").is_empty());
        let out = sse.push(b":1}\r\n\r\ndata: [DONE]\n\n: comment\n\n");
        assert_eq!(out, vec!["{\"a\":1}".to_string(), "[DONE]".to_string()]);
        // UTF-8 split across chunks
        let bytes = "data: \"ä\"\n\n".as_bytes();
        assert!(sse.push(&bytes[..7]).is_empty());
        assert_eq!(sse.push(&bytes[7..]), vec!["\"ä\"".to_string()]);
    }

    #[test]
    fn history_roundtrips_through_json() {
        let h = history();
        let s = serde_json::to_string(&h).unwrap();
        let back: Vec<Msg> = serde_json::from_str(&s).unwrap();
        assert_eq!(back, h);
    }
}
