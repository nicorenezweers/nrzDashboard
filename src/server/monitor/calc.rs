//! Pure helpers: status transitions, uptime, events, chart downsampling, status-code ranges, certificate dates.

use crate::api::monitor::{ChartPoint, Heartbeat, MonitorEvent, Status};

/// Next status after a check (Uptime-Kuma semantics).
///
/// `fails` is the number of consecutive failures before this check. A failure only turns into
/// `Down` once more than `retries` attempts failed; while retries remain the monitor is `Pending`.
/// An already down monitor stays down without a pending phase.
pub fn next_status(prev: Status, ok: bool, fails: u32, retries: u32) -> (Status, u32) {
    if ok {
        return (Status::Up, 0);
    }
    let fails = fails.saturating_add(1);
    if prev == Status::Down || fails > retries {
        (Status::Down, fails)
    } else {
        (Status::Pending, fails)
    }
}

/// Uptime in percent over beats with `ts >= since` (pending beats are ignored).
pub fn uptime(beats: &[Heartbeat], since: u64) -> Option<f64> {
    let (mut up, mut total) = (0u64, 0u64);
    for b in beats.iter().rev().take_while(|b| b.ts >= since) {
        match b.status {
            Status::Up => {
                up += 1;
                total += 1;
            }
            Status::Down => total += 1,
            _ => {}
        }
    }
    (total > 0).then(|| up as f64 * 100.0 / total as f64)
}

/// Average response time of successful beats with `ts >= since`.
pub fn avg_ms(beats: &[Heartbeat], since: u64) -> Option<f64> {
    let (mut sum, mut n) = (0.0, 0u32);
    for b in beats.iter().rev().take_while(|b| b.ts >= since) {
        if b.status == Status::Up
            && let Some(ms) = b.ms
        {
            sum += ms;
            n += 1;
        }
    }
    (n > 0).then(|| sum / f64::from(n))
}

/// Status changes up↔down (pending is skipped), newest first, at most `limit`.
pub fn events(beats: &[Heartbeat], now: u64, limit: usize) -> Vec<MonitorEvent> {
    let mut out: Vec<MonitorEvent> = Vec::new();
    let mut last: Option<Status> = None;
    for b in beats {
        if !matches!(b.status, Status::Up | Status::Down) || last == Some(b.status) {
            continue;
        }
        last = Some(b.status);
        out.push(MonitorEvent {
            ts: b.ts,
            status: b.status,
            duration_secs: 0,
            msg: b.msg.clone(),
            ongoing: false,
        });
    }
    let n = out.len();
    for i in 0..n {
        let end = out.get(i + 1).map_or(now, |e| e.ts);
        if let Some(e) = out.get_mut(i) {
            e.duration_secs = end.saturating_sub(e.ts);
            e.ongoing = i + 1 == n;
        }
    }
    out.reverse();
    out.truncate(limit);
    out
}

/// Response-time chart for `[since, now]`, averaged into at most `buckets` points.
pub fn chart(beats: &[Heartbeat], since: u64, now: u64, buckets: usize) -> Vec<ChartPoint> {
    let start = beats.partition_point(|b| b.ts < since);
    let pts = beats.get(start..).unwrap_or_default();
    let point = |b: &Heartbeat| ChartPoint {
        ts: b.ts,
        ms: if b.status == Status::Up { b.ms } else { None },
        down: b.status == Status::Down,
    };
    if pts.len() <= buckets.max(1) {
        return pts.iter().map(point).collect();
    }
    let span = now.saturating_sub(since).max(1);
    let width = span.div_ceil(buckets as u64).max(1);
    let mut out: Vec<ChartPoint> = Vec::with_capacity(buckets);
    let mut cur: Option<(u64, f64, u32, bool)> = None; // (bucket, sum, n, down)
    let flush = |out: &mut Vec<ChartPoint>, c: (u64, f64, u32, bool)| {
        let (bucket, sum, n, down) = c;
        out.push(ChartPoint {
            ts: since + bucket * width + width / 2,
            ms: (n > 0).then(|| sum / f64::from(n)),
            down,
        });
    };
    for b in pts {
        let bucket = b.ts.saturating_sub(since) / width;
        if let Some(c) = cur
            && c.0 != bucket
        {
            flush(&mut out, c);
            cur = None;
        }
        let c = cur.get_or_insert((bucket, 0.0, 0, false));
        if b.status == Status::Up
            && let Some(ms) = b.ms
        {
            c.1 += ms;
            c.2 += 1;
        }
        c.3 |= b.status == Status::Down;
    }
    if let Some(c) = cur {
        flush(&mut out, c);
    }
    out
}

/// Parses accepted HTTP status codes like `200-299, 301, 404`.
pub fn parse_codes(s: &str) -> Result<Vec<(u16, u16)>, String> {
    let mut out = Vec::new();
    for part in s
        .split([',', ' ', ';'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let parse = |x: &str| {
            x.trim()
                .parse::<u16>()
                .ok()
                .filter(|c| (100..=599).contains(c))
                .ok_or_else(|| format!("Ungültiger Statuscode: {x}"))
        };
        let range = match part.split_once('-') {
            Some((a, b)) => (parse(a)?, parse(b)?),
            None => {
                let c = parse(part)?;
                (c, c)
            }
        };
        if range.0 > range.1 {
            return Err(format!("Ungültiger Bereich: {part}"));
        }
        out.push(range);
    }
    if out.is_empty() {
        return Err("Mindestens ein erwarteter Statuscode nötig".into());
    }
    Ok(out)
}

pub fn code_ok(ranges: &[(u16, u16)], code: u16) -> bool {
    ranges.iter().any(|&(a, b)| (a..=b).contains(&code))
}

// ---------- X.509 notAfter (minimal DER walker) ----------

/// Reads one DER TLV, returns (tag, content, rest).
fn tlv(data: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let tag = *data.first()?;
    let first = *data.get(1)?;
    let (len, hdr) = if first < 0x80 {
        (usize::from(first), 2)
    } else {
        let n = usize::from(first & 0x7f);
        if n == 0 || n > 4 {
            return None;
        }
        let mut len = 0usize;
        for i in 0..n {
            len = (len << 8) | usize::from(*data.get(2 + i)?);
        }
        (len, 2 + n)
    };
    let content = data.get(hdr..hdr.checked_add(len)?)?;
    let rest = data.get(hdr + len..)?;
    Some((tag, content, rest))
}

/// Expiry (`notAfter`) of a DER encoded certificate as unix seconds.
pub fn cert_not_after(der: &[u8]) -> Option<u64> {
    let (_, cert, _) = tlv(der)?; // Certificate SEQUENCE
    let (_, tbs, _) = tlv(cert)?; // TBSCertificate SEQUENCE
    let mut rest = tbs;
    let (tag, _, r) = tlv(rest)?;
    if tag == 0xa0 {
        rest = r; // [0] version
    }
    let (_, _, r) = tlv(rest)?; // serialNumber
    let (_, _, r) = tlv(r)?; // signature AlgorithmIdentifier
    let (_, _, r) = tlv(r)?; // issuer
    let (_, validity, _) = tlv(r)?; // validity SEQUENCE
    let (_, _, r) = tlv(validity)?; // notBefore
    let (tag, time, _) = tlv(r)?; // notAfter
    asn1_time(tag, std::str::from_utf8(time).ok()?)
}

/// UTCTime (`YYMMDDHHMMSSZ`, tag 0x17) or GeneralizedTime (`YYYYMMDDHHMMSSZ`, tag 0x18).
pub fn asn1_time(tag: u8, s: &str) -> Option<u64> {
    let s = s.strip_suffix('Z')?;
    let (year, rest) = match tag {
        0x17 => {
            let yy: i64 = s.get(..2)?.parse().ok()?;
            (if yy >= 50 { 1900 + yy } else { 2000 + yy }, s.get(2..)?)
        }
        0x18 => (s.get(..4)?.parse().ok()?, s.get(4..)?),
        _ => return None,
    };
    let num = |i: usize| -> Option<i64> { rest.get(i..i + 2)?.parse().ok() };
    let (mo, d, h, mi, se) = (num(0)?, num(2)?, num(4)?, num(6)?, num(8)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let secs = days_from_civil(year, mo, d) * 86_400 + h * 3600 + mi * 60 + se;
    u64::try_from(secs).ok()
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hb(ts: u64, status: Status, ms: Option<f64>) -> Heartbeat {
        Heartbeat {
            ts,
            status,
            ms,
            msg: String::new(),
        }
    }

    #[test]
    fn status_transitions_with_retries() {
        // Up, then two failures with retries = 1: pending, then down.
        let (s, f) = next_status(Status::Up, false, 0, 1);
        assert_eq!((s, f), (Status::Pending, 1));
        let (s, f) = next_status(s, false, f, 1);
        assert_eq!((s, f), (Status::Down, 2));
        // Still down: no pending phase.
        assert_eq!(next_status(Status::Down, false, 2, 5).0, Status::Down);
        // Recovery resets the counter.
        assert_eq!(next_status(Status::Down, true, 7, 1), (Status::Up, 0));
        // No retries: immediately down.
        assert_eq!(next_status(Status::Up, false, 0, 0).0, Status::Down);
        // First check ever fails with retries left.
        assert_eq!(next_status(Status::Unknown, false, 0, 2).0, Status::Pending);
    }

    #[test]
    fn uptime_and_average() {
        let beats = vec![
            hb(10, Status::Down, None),
            hb(100, Status::Up, Some(10.0)),
            hb(110, Status::Pending, None),
            hb(120, Status::Down, None),
            hb(130, Status::Up, Some(30.0)),
            hb(140, Status::Up, Some(20.0)),
        ];
        assert_eq!(uptime(&beats, 100), Some(75.0));
        assert_eq!(uptime(&beats, 0), Some(60.0));
        assert_eq!(uptime(&beats, 1000), None);
        assert_eq!(avg_ms(&beats, 100), Some(20.0));
        assert_eq!(avg_ms(&beats, 135), Some(20.0));
        assert_eq!(uptime(&[], 0), None);
    }

    #[test]
    fn events_skip_pending_and_measure_duration() {
        let beats = vec![
            hb(0, Status::Up, Some(1.0)),
            hb(60, Status::Up, Some(1.0)),
            hb(120, Status::Pending, None),
            hb(180, Status::Down, None),
            hb(240, Status::Down, None),
            hb(300, Status::Up, Some(1.0)),
        ];
        let ev = events(&beats, 1000, 10);
        assert_eq!(ev.len(), 3);
        assert_eq!(
            (ev[0].status, ev[0].ts, ev[0].duration_secs, ev[0].ongoing),
            (Status::Up, 300, 700, true)
        );
        assert_eq!(
            (ev[1].status, ev[1].ts, ev[1].duration_secs),
            (Status::Down, 180, 120)
        );
        assert_eq!(
            (ev[2].status, ev[2].duration_secs, ev[2].ongoing),
            (Status::Up, 180, false)
        );
        assert_eq!(events(&beats, 1000, 1).len(), 1);
    }

    #[test]
    fn chart_downsamples() {
        let beats: Vec<Heartbeat> = (0..100)
            .map(|i| hb(i * 10, Status::Up, Some(i as f64)))
            .collect();
        let raw = chart(&beats, 500, 1000, 200);
        assert_eq!(raw.len(), 50);
        assert_eq!(raw[0].ts, 500);
        let c = chart(&beats, 0, 1000, 10);
        assert_eq!(c.len(), 10);
        assert_eq!(c[0].ms, Some(4.5));
        let mut with_down = beats.clone();
        with_down[3].status = Status::Down;
        assert!(chart(&with_down, 0, 1000, 10)[0].down);
    }

    #[test]
    fn status_code_ranges() {
        let r = parse_codes("200-299, 301 ,404").unwrap();
        assert_eq!(r, vec![(200, 299), (301, 301), (404, 404)]);
        assert!(code_ok(&r, 204));
        assert!(code_ok(&r, 404));
        assert!(!code_ok(&r, 302));
        assert!(parse_codes("").is_err());
        assert!(parse_codes("abc").is_err());
        assert!(parse_codes("300-200").is_err());
        assert!(parse_codes("99").is_err());
    }

    #[test]
    fn asn1_times() {
        // 2026-11-12 00:00:00 UTC = 1794441600
        assert_eq!(asn1_time(0x17, "261112000000Z"), Some(1_794_441_600));
        assert_eq!(asn1_time(0x18, "20261112000000Z"), Some(1_794_441_600));
        assert_eq!(asn1_time(0x17, "700101000000Z"), Some(0));
        assert_eq!(asn1_time(0x17, "261312000000Z"), None);
    }

    #[test]
    fn certificate_not_after() {
        fn wrap(tag: u8, content: &[u8]) -> Vec<u8> {
            let mut v = vec![tag];
            if content.len() < 0x80 {
                v.push(content.len() as u8);
            } else {
                v.push(0x82);
                v.extend_from_slice(&(content.len() as u16).to_be_bytes());
            }
            v.extend_from_slice(content);
            v
        }
        let validity = [wrap(0x17, b"250101000000Z"), wrap(0x17, b"261112000000Z")].concat();
        let tbs = [
            wrap(0xa0, &wrap(0x02, &[2])),
            wrap(0x02, &[0x01, 0x02]),
            wrap(0x30, &wrap(0x06, &[0x2a, 0x86, 0x48])),
            wrap(0x30, &[0u8; 200]),
            wrap(0x30, &validity),
        ]
        .concat();
        let cert = wrap(0x30, &[wrap(0x30, &tbs), wrap(0x30, &[])].concat());
        assert_eq!(cert_not_after(&cert), Some(1_794_441_600));
        assert_eq!(cert_not_after(&cert[..20]), None);
    }
}
