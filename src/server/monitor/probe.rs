//! One check per monitor type.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::json;

use super::calc;
use crate::api::monitor::{Monitor, MonitorKind};

/// Result of a successful check.
pub struct ProbeOk {
    pub ms: f64,
    pub msg: String,
    /// TLS certificate expiry (unix seconds), HTTPS only.
    pub cert_expiry: Option<u64>,
}

fn ms_since(start: Instant) -> f64 {
    (start.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0
}

fn client(insecure: bool) -> &'static reqwest::Client {
    static SECURE: OnceLock<reqwest::Client> = OnceLock::new();
    static INSECURE: OnceLock<reqwest::Client> = OnceLock::new();
    let cell = if insecure { &INSECURE } else { &SECURE };
    cell.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("nrzDashboard-Monitor/1.0")
            .tls_info(true)
            .danger_accept_invalid_certs(insecure)
            .build()
            .unwrap_or_default()
    })
}

pub async fn probe(m: &Monitor) -> Result<ProbeOk, String> {
    let timeout = Duration::from_secs(u64::from(m.timeout.clamp(1, 120)));
    match m.kind {
        MonitorKind::Http => http(m, timeout).await,
        MonitorKind::Tcp => tcp(m.target.trim(), timeout).await,
        MonitorKind::Ping => {
            let ms = crate::server::ping::icmp(m.target.trim(), timeout).await?;
            Ok(ProbeOk {
                ms: (ms * 10.0).round() / 10.0,
                msg: String::new(),
                cert_expiry: None,
            })
        }
        MonitorKind::Dns => dns(m, timeout).await,
        MonitorKind::App => app(m.target.trim(), timeout).await,
        MonitorKind::Push => Err("Push-Monitore werden nicht aktiv geprüft".into()),
    }
}

async fn http(m: &Monitor, timeout: Duration) -> Result<ProbeOk, String> {
    let codes = calc::parse_codes(&m.accepted_codes)?;
    let method = reqwest::Method::from_bytes(m.method.trim().to_ascii_uppercase().as_bytes())
        .map_err(|_| "Ungültige HTTP-Methode".to_string())?;
    let start = Instant::now();
    let res = client(m.ignore_tls)
        .request(method, m.target.trim())
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| http_error(&e))?;
    let cert_expiry = res
        .extensions()
        .get::<reqwest::tls::TlsInfo>()
        .and_then(|t| t.peer_certificate())
        .and_then(calc::cert_not_after);
    let status = res.status();
    let code_text = format!(
        "{} {}",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
    );
    let code_text = code_text.trim().to_string();
    if !calc::code_ok(&codes, status.as_u16()) {
        return Err(format!("Statuscode {code_text} nicht erwartet"));
    }
    let keyword = m.keyword.trim();
    let mut msg = code_text;
    if !keyword.is_empty() {
        let body = tokio::time::timeout(timeout, res.text())
            .await
            .map_err(|_| "Timeout beim Lesen der Antwort".to_string())?
            .map_err(|e| e.to_string())?;
        let found = body.contains(keyword);
        match (found, m.keyword_invert) {
            (true, false) => msg.push_str(&format!(" · Keyword „{keyword}“ gefunden")),
            (false, true) => msg.push_str(&format!(" · Keyword „{keyword}“ nicht enthalten")),
            (false, false) => return Err(format!("Keyword „{keyword}“ nicht gefunden")),
            (true, true) => return Err(format!("Keyword „{keyword}“ ist enthalten")),
        }
    }
    Ok(ProbeOk {
        ms: ms_since(start),
        msg,
        cert_expiry,
    })
}

fn http_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        return "Timeout".into();
    }
    // Walk the source chain for the useful part (DNS, TLS, refused …).
    let mut msg = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        msg = s.to_string();
        src = s.source();
    }
    msg
}

async fn tcp(addr: &str, timeout: Duration) -> Result<ProbeOk, String> {
    let start = Instant::now();
    tokio::time::timeout(timeout, tokio::net::TcpStream::connect(addr))
        .await
        .map_err(|_| "Timeout".to_string())?
        .map_err(|e| e.to_string())?;
    Ok(ProbeOk {
        ms: ms_since(start),
        msg: "Port offen".into(),
        cert_expiry: None,
    })
}

async fn dns(m: &Monitor, timeout: Duration) -> Result<ProbeOk, String> {
    let name = m.target.trim().to_string();
    let start = Instant::now();
    let addrs = tokio::time::timeout(timeout, tokio::net::lookup_host((name.as_str(), 0)))
        .await
        .map_err(|_| "Timeout".to_string())?
        .map_err(|e| format!("Nicht auflösbar: {e}"))?;
    let ms = ms_since(start);
    let mut ips: Vec<std::net::IpAddr> = addrs
        .map(|a| a.ip())
        .filter(|ip| match m.dns_record.as_str() {
            "A" => ip.is_ipv4(),
            "AAAA" => ip.is_ipv6(),
            _ => true,
        })
        .collect();
    ips.sort();
    ips.dedup();
    if ips.is_empty() {
        let rec = if m.dns_record.is_empty() {
            "A/AAAA"
        } else {
            m.dns_record.as_str()
        };
        return Err(format!("Kein {rec}-Eintrag"));
    }
    let list = ips
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let expected = m.dns_expected.trim();
    if !expected.is_empty() {
        let exp: std::net::IpAddr = expected
            .parse()
            .map_err(|_| "Erwartete IP ist ungültig".to_string())?;
        if !ips.contains(&exp) {
            return Err(format!("{expected} nicht in Antwort ({list})"));
        }
    }
    Ok(ProbeOk {
        ms,
        msg: list,
        cert_expiry: None,
    })
}

async fn app(app_id: &str, timeout: Duration) -> Result<ProbeOk, String> {
    let start = Instant::now();
    let v = tokio::time::timeout(
        timeout,
        crate::server::mcp::call("get_app_status", json!({ "appId": app_id })),
    )
    .await
    .map_err(|_| "Timeout".to_string())??;
    let ms = ms_since(start);
    let state = v
        .get("state")
        .and_then(|s| s.as_str())
        .unwrap_or("unbekannt")
        .to_string();
    if app_state_up(&state) {
        Ok(ProbeOk {
            ms,
            msg: format!("App {state}"),
            cert_expiry: None,
        })
    } else {
        Err(format!("App-Status: {state}"))
    }
}

pub fn app_state_up(state: &str) -> bool {
    matches!(state, "ready" | "running")
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_states() {
        assert!(super::app_state_up("ready"));
        assert!(super::app_state_up("running"));
        assert!(!super::app_state_up("stopped"));
        assert!(!super::app_state_up("starting"));
    }
}
