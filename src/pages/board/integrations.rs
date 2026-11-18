//! Widgets backed by other nrz apps: DNS (nrzDNS) and finance (nrzBudget).

use dioxus::prelude::*;
use icons::{PiggyBank, Shield, ShieldCheck, ShieldOff, TrendingDown, TrendingUp};

use crate::api::board::Widget;
use crate::api::integrations::{BudgetOrg, DnsPoint, budget_overview, dns_widget};
use crate::app::Route;
use crate::utils::{err_msg, fmt_uptime, sleep_ms};

/// German number formatting: 1.234.567
fn fmt_int(v: f64) -> String {
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// 12.3k / 1.2M for big counters.
fn fmt_compact(v: f64) -> String {
    if v >= 1_000_000.0 {
        format!("{} Mio.", format!("{:.1}", v / 1_000_000.0).replace('.', ","))
    } else if v >= 10_000.0 {
        format!("{:.1}k", v / 1000.0).replace('.', ",")
    } else {
        fmt_int(v)
    }
}

/// 1.234,56 €
pub fn fmt_money(v: f64, currency: &str) -> String {
    let cents = (v.abs() * 100.0).round() as i64;
    let int = fmt_int((cents / 100) as f64);
    let s = format!("{}{int},{:02}", if v < 0.0 { "-" } else { "" }, cents % 100);
    let sym = match currency {
        "EUR" | "" => "€",
        "CHF" => "CHF",
        "USD" => "$",
        "GBP" => "£",
        other => other,
    };
    format!("{s} {sym}")
}

fn pct(v: f64) -> String {
    format!("{:.1} %", v).replace('.', ",")
}

#[component]
fn NotConfigured(message: String) -> Element {
    rsx! {
        div { class: "flex h-full flex-col items-center justify-center gap-2 text-center text-sm",
            p { class: "text-muted-foreground", "{message}" }
            Link { to: Route::SystemPage {}, class: "text-xs underline underline-offset-2 text-muted-foreground hover:text-foreground", "System → Integrationen" }
        }
    }
}

// ---------- DNS ----------

#[component]
fn StatBox(label: String, value: String, #[props(default)] sub: String, #[props(default)] tone: &'static str) -> Element {
    rsx! {
        div { class: "min-w-0 rounded-lg bg-muted/40 px-3 py-2",
            div { class: "truncate text-[11px] text-muted-foreground", "{label}" }
            div { class: "truncate text-lg font-semibold tabular-nums {tone}", "{value}" }
            if !sub.is_empty() {
                div { class: "truncate text-[11px] text-muted-foreground", "{sub}" }
            }
        }
    }
}

/// Stacked bars: total (muted) with the blocked share on top (destructive).
#[component]
fn DnsBars(series: Vec<DnsPoint>) -> Element {
    if series.is_empty() {
        return rsx! {};
    }
    const W: f64 = 300.0;
    const H: f64 = 60.0;
    let max = series.iter().map(|p| p.total).fold(1.0_f64, f64::max);
    let n = series.len() as f64;
    let bw = W / n;
    let gap = (bw * 0.2).min(2.0);
    rsx! {
        svg { class: "h-14 w-full", view_box: "0 0 {W} {H}", preserve_aspect_ratio: "none",
            for (i, p) in series.iter().enumerate() {
                {
                    let x = i as f64 * bw + gap / 2.0;
                    let th = p.total / max * (H - 2.0);
                    let bh = p.blocked / max * (H - 2.0);
                    rsx! {
                        rect { x: "{x}", y: "{H - th}", width: "{(bw - gap).max(0.5)}", height: "{th}", rx: "1", fill: "var(--muted-foreground)", opacity: "0.35" }
                        rect { x: "{x}", y: "{H - bh}", width: "{(bw - gap).max(0.5)}", height: "{bh}", rx: "1", fill: "var(--destructive)", opacity: "0.85" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn DnsWidget(widget: Widget) -> Element {
    let range = {
        let r = widget.opt_str("range");
        if r.is_empty() { "day".to_string() } else { r }
    };
    let mut data = use_resource(use_reactive!(|range| async move { dns_widget(range).await }));
    use_future(move || async move {
        loop {
            sleep_ms(15_000).await;
            data.restart();
        }
    });
    let wide = widget.w >= 6;
    let tall = widget.h >= 5;
    match data.read().clone() {
        None => rsx! { div { class: "h-full animate-pulse rounded-lg bg-muted/40" } },
        Some(Err(e)) => {
            let m = err_msg(&e);
            if m.contains("Token") {
                rsx! { NotConfigured { message: "nrzDNS ist noch nicht verbunden." } }
            } else {
                rsx! { p { class: "text-sm text-destructive break-words", "{m}" } }
            }
        }
        Some(Ok(d)) => {
            let o = d.overview;
            let (status, tone) = if o.blocking.paused {
                ("Filter pausiert", "text-warning")
            } else if o.blocking.active {
                ("Filter aktiv", "text-success")
            } else {
                ("Filter aus", "text-destructive")
            };
            let range_label = match widget.opt_str("range").as_str() {
                "hour" => "1 Std",
                "week" => "7 Tage",
                "month" => "30 Tage",
                _ => "24 Std",
            };
            let top = if tall { 6 } else { 3 };
            rsx! {
                div { class: "flex h-full flex-col gap-3 overflow-y-auto",
                    div { class: "flex items-center justify-between gap-2 text-xs",
                        span { class: "flex items-center gap-1.5 font-medium {tone} [&_svg]:size-3.5",
                            if o.blocking.paused { Shield {} } else if o.blocking.active { ShieldCheck {} } else { ShieldOff {} }
                            "{status}"
                        }
                        span { class: "text-muted-foreground tabular-nums", "{o.server.queries_per_second:.1} Anfr./s · {range_label}" }
                    }
                    div { class: if wide { "grid grid-cols-4 gap-2" } else { "grid grid-cols-3 gap-2" },
                        StatBox { label: "Anfragen", value: fmt_compact(o.totals.queries) }
                        StatBox { label: "Gesperrt", value: pct(o.totals.blocked_percent), sub: fmt_compact(o.totals.blocked), tone: "text-destructive" }
                        StatBox { label: "Cache", value: pct(o.totals.cache_hit_percent) }
                        if wide {
                            StatBox { label: "Ø Antwort", value: format!("{:.0} ms", o.totals.avg_duration_ms), sub: format!("{} Clients", fmt_int(o.totals.clients)) }
                        }
                    }
                    DnsBars { series: d.series.clone() }
                    if widget.h >= 4 {
                        div { class: if wide { "grid grid-cols-2 gap-4" } else { "grid grid-cols-1 gap-3" },
                            div { class: "min-w-0",
                                div { class: "mb-1 text-[11px] text-muted-foreground", "Meist gesperrt" }
                                for h in o.top_blocked.iter().take(top) {
                                    div { class: "flex items-center justify-between gap-2 py-0.5 text-xs",
                                        span { class: "truncate font-mono", "{h.name}" }
                                        span { class: "shrink-0 tabular-nums text-muted-foreground", {fmt_int(h.hits)} }
                                    }
                                }
                            }
                            if wide || widget.h >= 4 {
                                div { class: "min-w-0",
                                    div { class: "mb-1 text-[11px] text-muted-foreground", "Upstreams ({o.upstreams.healthy:.0} ok)" }
                                    for u in o.upstreams.items.iter().take(top) {
                                        div { class: "flex items-center justify-between gap-2 py-0.5 text-xs",
                                            span { class: "flex min-w-0 items-center gap-1.5",
                                                span { class: if u.ok > 0.0 && u.failed <= u.ok { "size-1.5 shrink-0 rounded-full bg-success" } else { "size-1.5 shrink-0 rounded-full bg-destructive" } }
                                                span { class: "truncate", "{u.name}" }
                                            }
                                            span { class: "shrink-0 tabular-nums text-muted-foreground", if u.ok > 0.0 { "{u.avg_ms:.0} ms" } else { "–" } }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if tall {
                        div { class: "mt-auto flex justify-between text-[11px] text-muted-foreground",
                            span { "{fmt_int(o.blocking.blocked_domains)} Domains auf {o.blocking.lists:.0} Listen" }
                            span { "nrzDNS {o.server.version} · {fmt_uptime(o.server.uptime_secs)}" }
                        }
                    }
                }
            }
        }
    }
}

// ---------- Finance ----------

#[component]
pub fn FinanceWidget(widget: Widget) -> Element {
    let mut data = use_resource(budget_overview);
    use_future(move || async move {
        loop {
            sleep_ms(60_000).await;
            data.restart();
        }
    });
    let want = widget.opt_str("organization").to_lowercase();
    match data.read().clone() {
        None => rsx! { div { class: "h-full animate-pulse rounded-lg bg-muted/40" } },
        Some(Err(e)) => {
            let m = err_msg(&e);
            if m.contains("Token") {
                rsx! { NotConfigured { message: "nrzBudget ist noch nicht verbunden." } }
            } else {
                rsx! { p { class: "text-sm text-destructive break-words", "{m}" } }
            }
        }
        Some(Ok(o)) => {
            let org = o
                .organizations
                .iter()
                .find(|g| !want.is_empty() && (g.name.to_lowercase() == want || format!("{}", g.id as i64) == want))
                .or_else(|| o.organizations.first())
                .cloned();
            match org {
                None => rsx! { p { class: "text-sm text-muted-foreground", "Keine Organisation in nrzBudget." } },
                Some(g) => rsx! { FinanceView { org: g, h: widget.h, w: widget.w } },
            }
        }
    }
}

#[component]
fn FinanceView(org: BudgetOrg, h: i32, w: i32) -> Element {
    let cur = org.currency.clone();
    let net_tone = if org.net >= 0.0 { "text-success" } else { "text-destructive" };
    let month_tone = if org.current_month.net >= 0.0 { "text-success" } else { "text-destructive" };
    let max = org.income.max(org.expenses).max(1.0);
    let wide = w >= 6;
    let top = if h >= 5 { 6 } else { 4 };
    rsx! {
        div { class: "flex h-full flex-col gap-3 overflow-y-auto",
            div { class: "flex items-baseline justify-between gap-2",
                div { class: "min-w-0",
                    div { class: "truncate text-[11px] text-muted-foreground", "{org.name} · {org.year:.0}" }
                    div { class: "text-2xl font-semibold tabular-nums {net_tone}", {fmt_money(org.net, &cur)} }
                }
                div { class: "flex shrink-0 items-center gap-1 text-xs text-muted-foreground [&_svg]:size-3.5",
                    PiggyBank {}
                    "Sparquote "
                    span { class: "tabular-nums {net_tone}", {pct(org.savings_rate * 100.0)} }
                }
            }
            div { class: "flex flex-col gap-2",
                div { class: "flex flex-col gap-1",
                    div { class: "flex justify-between text-xs",
                        span { class: "flex items-center gap-1 text-muted-foreground [&_svg]:size-3.5", TrendingUp {} "Einnahmen" }
                        span { class: "tabular-nums", {fmt_money(org.income, &cur)} }
                    }
                    div { class: "h-1.5 overflow-hidden rounded-full bg-secondary",
                        div { class: "h-full rounded-full bg-success", style: format!("width: {:.1}%", org.income / max * 100.0) }
                    }
                }
                div { class: "flex flex-col gap-1",
                    div { class: "flex justify-between text-xs",
                        span { class: "flex items-center gap-1 text-muted-foreground [&_svg]:size-3.5", TrendingDown {} "Ausgaben" }
                        span { class: "tabular-nums", {fmt_money(org.expenses, &cur)} }
                    }
                    div { class: "h-1.5 overflow-hidden rounded-full bg-secondary",
                        div { class: "h-full rounded-full bg-destructive", style: format!("width: {:.1}%", org.expenses / max * 100.0) }
                    }
                }
            }
            div { class: "grid grid-cols-3 gap-2 rounded-lg bg-muted/40 px-3 py-2 text-xs",
                div {
                    div { class: "text-[11px] text-muted-foreground", "Monat {org.current_month.month}" }
                    div { class: "tabular-nums font-medium {month_tone}", {fmt_money(org.current_month.net, &cur)} }
                }
                div {
                    div { class: "text-[11px] text-muted-foreground", "Ø pro Monat" }
                    div { class: "tabular-nums font-medium", {fmt_money(org.monthly_average.net, &cur)} }
                }
                div {
                    div { class: "text-[11px] text-muted-foreground", "Saldo" }
                    div { class: "tabular-nums font-medium", {fmt_money(org.ending_balance, &cur)} }
                }
            }
            if h >= 4 {
                div { class: if wide { "grid grid-cols-2 gap-4" } else { "grid grid-cols-1" },
                    div { class: "flex flex-col gap-1.5",
                        div { class: "text-[11px] text-muted-foreground", "Grösste Ausgaben" }
                        for s in org.top_expenses.iter().take(top) {
                            div { class: "flex flex-col gap-0.5",
                                div { class: "flex justify-between gap-2 text-xs",
                                    span { class: "truncate", "{s.group}" }
                                    span { class: "shrink-0 tabular-nums text-muted-foreground", {fmt_money(s.total, &cur)} }
                                }
                                div { class: "h-1 overflow-hidden rounded-full bg-secondary",
                                    div { class: "h-full rounded-full bg-primary/70", style: format!("width: {:.1}%", s.share * 100.0) }
                                }
                            }
                        }
                    }
                    if wide {
                        div { class: "flex flex-col gap-1.5",
                            div { class: "text-[11px] text-muted-foreground", "Einnahmen nach Gruppe" }
                            for s in org.top_income.iter().take(top) {
                                div { class: "flex justify-between gap-2 text-xs",
                                    span { class: "truncate", "{s.group}" }
                                    span { class: "shrink-0 tabular-nums text-muted-foreground", {fmt_money(s.total, &cur)} }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_and_ints() {
        assert_eq!(fmt_int(1234567.0), "1.234.567");
        assert_eq!(fmt_int(-950.0), "-950");
        assert_eq!(fmt_money(-95.98, "EUR"), "-95,98 €");
        assert_eq!(fmt_money(1234.5, "CHF"), "1.234,50 CHF");
        assert_eq!(pct(12.04), "12,0 %");
        assert_eq!(fmt_compact(2_376_137.0), "2,4 Mio.");
        assert_eq!(fmt_compact(17_657.0), "17,7k");
    }
}
