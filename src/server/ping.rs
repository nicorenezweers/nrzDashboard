//! Native ICMP echo over unprivileged datagram sockets (fallback: system `ping`). Used by the monitors.

use std::time::{Duration, Instant};

/// ICMP echo; returns the round-trip time in ms.
pub async fn icmp(host: &str, timeout: Duration) -> Result<f64, String> {
    if host.is_empty() || host.starts_with('-') || host.contains(char::is_whitespace) {
        return Err("Ungültiger Host".into());
    }
    let addr = tokio::net::lookup_host((host, 0))
        .await
        .map_err(|e| format!("Host unbekannt: {e}"))?
        .next()
        .ok_or("Host unbekannt")?;
    let ip = addr.ip();
    let native = tokio::task::spawn_blocking(move || icmp_native(ip, timeout))
        .await
        .map_err(|e| e.to_string())?;
    match native {
        Err(IcmpError::NoSocket) => icmp_command(host, timeout).await,
        Err(IcmpError::Failed(e)) => Err(e),
        Ok(ms) => Ok(ms),
    }
}

enum IcmpError {
    /// Unprivileged ICMP sockets are not allowed (`net.ipv4.ping_group_range`).
    NoSocket,
    Failed(String),
}

/// Echo request over an unprivileged ICMP datagram socket (no root, no capabilities).
fn icmp_native(ip: std::net::IpAddr, timeout: Duration) -> Result<f64, IcmpError> {
    use socket2::{Domain, Protocol, SockAddr, Socket, Type};
    use std::io::Read;

    let v6 = ip.is_ipv6();
    let (domain, proto, req_type, reply_type) = if v6 {
        (Domain::IPV6, Protocol::ICMPV6, 128u8, 129u8)
    } else {
        (Domain::IPV4, Protocol::ICMPV4, 8u8, 0u8)
    };
    let sock = Socket::new(domain, Type::DGRAM, Some(proto)).map_err(|_| IcmpError::NoSocket)?;
    sock.set_read_timeout(Some(timeout))
        .map_err(|e| IcmpError::Failed(e.to_string()))?;

    let ident = (std::process::id() & 0xffff) as u16;
    let seq: u16 = rand::random();
    let mut pkt = vec![req_type, 0, 0, 0];
    pkt.extend_from_slice(&ident.to_be_bytes());
    pkt.extend_from_slice(&seq.to_be_bytes());
    pkt.extend_from_slice(b"nrzDashboard-ping");
    if !v6 {
        let sum = checksum(&pkt);
        pkt[2..4].copy_from_slice(&sum.to_be_bytes());
    }

    let start = Instant::now();
    sock.send_to(&pkt, &SockAddr::from(std::net::SocketAddr::new(ip, 0)))
        .map_err(|e| IcmpError::Failed(e.to_string()))?;
    let mut buf = [0u8; 1500];
    loop {
        if start.elapsed() >= timeout {
            return Err(IcmpError::Failed("Timeout".into()));
        }
        let n = match (&sock).read(&mut buf) {
            Ok(n) => n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(IcmpError::Failed("Timeout".into()));
            }
            Err(e) => return Err(IcmpError::Failed(e.to_string())),
        };
        let data = buf.get(..n).unwrap_or_default();
        // macOS includes the IPv4 header on datagram ICMP sockets, Linux does not.
        let icmp = match data.first() {
            Some(b) if !v6 && b >> 4 == 4 => {
                data.get(usize::from(b & 0x0f) * 4..).unwrap_or_default()
            }
            _ => data,
        };
        if icmp.first() == Some(&reply_type) && icmp.get(6..8) == Some(&seq.to_be_bytes()[..]) {
            return Ok(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = data
        .chunks(2)
        .map(|c| u32::from(u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])))
        .sum();
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Fallback: system `ping` binary.
async fn icmp_command(host: &str, timeout: Duration) -> Result<f64, String> {
    let start = Instant::now();
    let out = tokio::time::timeout(
        timeout,
        tokio::process::Command::new("ping")
            .arg("-c")
            .arg("1")
            .arg(host)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "Timeout".to_string())?
    .map_err(|_| "ICMP nicht erlaubt und kein ping-Programm vorhanden".to_string())?;
    if !out.status.success() {
        return Err("Keine Antwort".into());
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let ms = stdout
        .split("time=")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.trim_end_matches("ms").parse::<f64>().ok())
        .unwrap_or_else(|| start.elapsed().as_secs_f64() * 1000.0);
    Ok(ms)
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_icmp_localhost() {
        let r = super::icmp_native(
            "127.0.0.1".parse().unwrap(),
            std::time::Duration::from_secs(2),
        );
        match r {
            Ok(ms) => assert!(ms >= 0.0),
            Err(super::IcmpError::NoSocket) => {}
            Err(super::IcmpError::Failed(e)) => panic!("{e}"),
        }
    }
}
