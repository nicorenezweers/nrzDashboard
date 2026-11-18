//! Notification channels (`$DATA_DIR/notifications.json`, mode 0600): ntfy, Discord, Telegram, webhook.

use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use serde_json::{Value, json};

use crate::api::monitor::{Heartbeat, MASK, Monitor, NotifyChannel, NotifyKind, Status};
use crate::server::{config, tracing_warn};

fn file() -> std::path::PathBuf {
    config().data_dir.join("notifications.json")
}

fn store() -> &'static RwLock<Vec<NotifyChannel>> {
    static S: OnceLock<RwLock<Vec<NotifyChannel>>> = OnceLock::new();
    S.get_or_init(|| {
        let list = std::fs::read_to_string(file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        RwLock::new(list)
    })
}

fn persist(list: &[NotifyChannel]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    super::write_private(&file(), json.as_bytes())
}

/// Masks a secret: `••••abcd` (last 4 characters for long values).
pub fn mask(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > 12 {
        let tail: String = chars.iter().skip(chars.len() - 4).collect();
        format!("{MASK}{tail}")
    } else {
        MASK.to_string()
    }
}

fn is_masked(s: &str) -> bool {
    s.starts_with(MASK)
}

fn mask_channel(mut c: NotifyChannel) -> NotifyChannel {
    c.token = mask(&c.token);
    if c.kind == NotifyKind::Discord {
        c.url = mask(&c.url);
    }
    c
}

/// Replaces masked fields with the stored values of the channel with the same id.
fn unmask(mut c: NotifyChannel, stored: Option<&NotifyChannel>) -> NotifyChannel {
    if let Some(old) = stored {
        if is_masked(&c.token) {
            c.token = old.token.clone();
        }
        if is_masked(&c.url) {
            c.url = old.url.clone();
        }
    } else {
        if is_masked(&c.token) {
            c.token.clear();
        }
        if is_masked(&c.url) {
            c.url.clear();
        }
    }
    c
}

pub fn channels_masked() -> Vec<NotifyChannel> {
    store()
        .read()
        .map(|l| l.iter().cloned().map(mask_channel).collect())
        .unwrap_or_default()
}

fn validate(c: &NotifyChannel) -> Result<(), String> {
    let url_ok = |u: &str| u.starts_with("http://") || u.starts_with("https://");
    match c.kind {
        NotifyKind::Ntfy => {
            if !url_ok(&c.url) {
                return Err("ntfy-Server-URL muss mit http:// oder https:// beginnen".into());
            }
            if c.topic.trim().is_empty() {
                return Err("Topic fehlt".into());
            }
        }
        NotifyKind::Discord | NotifyKind::Webhook => {
            if !url_ok(&c.url) {
                return Err("Webhook-URL muss mit http:// oder https:// beginnen".into());
            }
        }
        NotifyKind::Telegram => {
            if c.token.trim().is_empty() || c.chat_id.trim().is_empty() {
                return Err("Bot-Token und Chat-ID nötig".into());
            }
        }
    }
    Ok(())
}

fn trimmed(mut c: NotifyChannel) -> NotifyChannel {
    for f in [
        &mut c.name,
        &mut c.url,
        &mut c.topic,
        &mut c.token,
        &mut c.chat_id,
    ] {
        *f = f.trim().to_string();
    }
    c.url = c.url.trim_end_matches('/').to_string();
    c
}

pub fn save(c: NotifyChannel) -> Result<String, String> {
    let mut list = store().write().map_err(|_| "Interner Fehler".to_string())?;
    let stored = list
        .iter()
        .find(|x| x.id == c.id && !c.id.is_empty())
        .cloned();
    let mut c = trimmed(unmask(c, stored.as_ref()));
    validate(&c)?;
    if c.name.is_empty() {
        c.name = c.kind.label().to_string();
    }
    if stored.is_some() {
        if let Some(slot) = list.iter_mut().find(|x| x.id == c.id) {
            *slot = c.clone();
        }
    } else {
        c.id = crate::server::auth::random_token(6);
        list.push(c.clone());
    }
    persist(&list)?;
    Ok(c.id)
}

pub fn delete(id: &str) -> Result<(), String> {
    let mut list = store().write().map_err(|_| "Interner Fehler".to_string())?;
    list.retain(|c| c.id != id);
    persist(&list)
}

pub async fn test(c: NotifyChannel) -> Result<String, String> {
    let stored = store()
        .read()
        .ok()
        .and_then(|l| l.iter().find(|x| x.id == c.id && !c.id.is_empty()).cloned());
    let c = trimmed(unmask(c, stored.as_ref()));
    validate(&c)?;
    let msg = Message {
        title: "nrzDashboard: Test".into(),
        body: "Testnachricht aus nrzDashboard – der Kanal funktioniert.".into(),
        status: "test",
        payload: json!({ "event": "test" }),
    };
    send(&c, &msg).await?;
    Ok(format!(
        "Testnachricht an „{}“ gesendet",
        if c.name.is_empty() {
            c.kind.label()
        } else {
            &c.name
        }
    ))
}

struct Message {
    title: String,
    body: String,
    status: &'static str,
    payload: Value,
}

/// Sends a status change of `m` to all enabled channels (in the background).
pub fn status_changed(m: &Monitor, beat: &Heartbeat, prev_since: Option<u64>) {
    let channels: Vec<NotifyChannel> = store()
        .read()
        .map(|l| l.iter().filter(|c| c.enabled).cloned().collect())
        .unwrap_or_default();
    if channels.is_empty() {
        return;
    }
    let up = beat.status == Status::Up;
    let target = m.target_display();
    let mut body = format!("{} ({}: {target})", m.name, m.kind.label());
    if !beat.msg.is_empty() {
        body.push_str(&format!("\n{}", beat.msg));
    }
    if let Some(since) = prev_since {
        let d = crate::api::monitor::fmt_duration(beat.ts.saturating_sub(since));
        body.push_str(&format!(
            "\n{} seit {d}",
            if up { "Ausfall dauerte" } else { "Vorher up" }
        ));
    }
    let msg = Message {
        title: if up {
            format!("✅ {} ist wieder erreichbar", m.name)
        } else {
            format!("🔴 {} ist ausgefallen", m.name)
        },
        body,
        status: if up { "up" } else { "down" },
        payload: json!({
            "event": "status",
            "monitor": { "id": m.id, "name": m.name, "kind": m.kind.key(), "target": target, "group": m.group },
            "heartbeat": beat,
        }),
    };
    tokio::spawn(async move {
        for c in channels {
            if let Err(e) = send(&c, &msg).await {
                tracing_warn(&format!(
                    "Benachrichtigung über „{}“ fehlgeschlagen: {e}",
                    c.name
                ));
            }
        }
    });
}

async fn send(c: &NotifyChannel, msg: &Message) -> Result<(), String> {
    let http = crate::server::http();
    let req = match c.kind {
        NotifyKind::Ntfy => {
            let mut r = http.post(&c.url).json(&json!({
                "topic": c.topic,
                "title": msg.title,
                "message": msg.body,
                "tags": [match msg.status { "up" => "white_check_mark", "down" => "rotating_light", _ => "bell" }],
                "priority": if msg.status == "down" { 4 } else { 3 },
            }));
            if !c.token.is_empty() {
                r = r.bearer_auth(&c.token);
            }
            r
        }
        NotifyKind::Discord => http
            .post(&c.url)
            .json(&json!({ "content": format!("**{}**\n{}", msg.title, msg.body) })),
        NotifyKind::Telegram => http
            .post(format!(
                "https://api.telegram.org/bot{}/sendMessage",
                c.token
            ))
            .json(&json!({ "chat_id": c.chat_id, "text": format!("{}\n{}", msg.title, msg.body) })),
        NotifyKind::Webhook => {
            let mut body = json!({ "title": msg.title, "message": msg.body, "status": msg.status });
            if let (Some(obj), Value::Object(extra)) = (body.as_object_mut(), &msg.payload) {
                obj.extend(extra.clone());
            }
            http.post(&c.url).json(&body)
        }
    };
    let res = req
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| {
            // Never leak the Telegram token / webhook URL in messages.
            if e.is_timeout() {
                "Timeout".to_string()
            } else {
                "Verbindung fehlgeschlagen".to_string()
            }
        })?;
    let status = res.status();
    if status.is_success() {
        Ok(())
    } else {
        let text: String = res
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        Err(format!("HTTP {status}: {text}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masking_roundtrip() {
        assert_eq!(mask(""), "");
        assert_eq!(mask("short"), MASK);
        assert_eq!(mask("1234567890:ABCDEFGH"), format!("{MASK}EFGH"));
        let stored = NotifyChannel {
            id: "a".into(),
            kind: NotifyKind::Discord,
            url: "https://discord.com/api/webhooks/1/secret".into(),
            token: "tok".into(),
            ..Default::default()
        };
        let masked = mask_channel(stored.clone());
        assert!(is_masked(&masked.url) && is_masked(&masked.token));
        assert_eq!(unmask(masked.clone(), Some(&stored)), stored);
        let fresh = unmask(masked, None);
        assert!(fresh.url.is_empty() && fresh.token.is_empty());
    }
}
