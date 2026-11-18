//! Integrations with other nrz apps over MCP: nrzDNS (DNS widgets) and nrzBudget (finance widgets).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use super::err;
#[cfg(feature = "server")]
use serde_json::json;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Integration {
    Dns,
    Budget,
}

impl Integration {
    #[cfg_attr(not(feature = "server"), allow(dead_code))]
    pub const ALL: [Self; 2] = [Self::Dns, Self::Budget];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Dns => "nrzDNS",
            Self::Budget => "nrzBudget",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Dns => "DNS-Widgets: Anfragen, Sperrquote, Top-Domains, Upstreams",
            Self::Budget => "Finanz-Widgets: Einnahmen, Ausgaben, Sparquote, Top-Ausgaben",
        }
    }

    /// Address inside the Umbrel network (container name + internal port).
    pub const fn default_url(self) -> &'static str {
        match self {
            Self::Dns => "http://nrz-dns_app_1:8380/mcp",
            Self::Budget => "http://nrz-budget_app_1:8080/mcp",
        }
    }

    pub const fn token_prefix(self) -> &'static str {
        match self {
            Self::Dns => "nrzdns_mcp_…",
            Self::Budget => "nrzb_…",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct IntegrationInfo {
    pub kind: Integration,
    pub url: String,
    pub token_hint: Option<String>,
}

// ---------- nrzDNS ----------

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DnsBlocking {
    pub active: bool,
    pub enabled: bool,
    pub paused: bool,
    pub paused_until: Option<f64>,
    pub blocked_domains: f64,
    pub lists: f64,
    pub lists_with_errors: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DnsServer {
    pub version: String,
    pub queries_per_second: f64,
    pub uptime_secs: f64,
    pub dns_listening: bool,
    pub cache_entries: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DnsTotals {
    pub queries: f64,
    pub blocked: f64,
    pub blocked_percent: f64,
    pub cache_hit_percent: f64,
    pub cached: f64,
    pub forwarded: f64,
    pub failed: f64,
    pub avg_duration_ms: f64,
    pub clients: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct DnsHit {
    pub name: String,
    pub hits: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DnsUpstream {
    pub name: String,
    pub avg_ms: f64,
    pub ok: f64,
    pub failed: f64,
    pub last_error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct DnsUpstreams {
    pub healthy: f64,
    pub items: Vec<DnsUpstream>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DnsOverview {
    pub blocking: DnsBlocking,
    pub server: DnsServer,
    pub totals: DnsTotals,
    pub top_blocked: Vec<DnsHit>,
    pub top_domains: Vec<DnsHit>,
    pub top_clients: Vec<DnsHit>,
    pub upstreams: DnsUpstreams,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct DnsPoint {
    pub at: f64,
    pub total: f64,
    pub blocked: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct DnsWidgetData {
    pub overview: DnsOverview,
    pub series: Vec<DnsPoint>,
}

// ---------- nrzBudget ----------

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct BudgetMonth {
    pub month: String,
    pub income: f64,
    pub expenses: f64,
    pub net: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct BudgetShare {
    pub group: String,
    pub share: f64,
    pub total: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct BudgetAverage {
    pub income: f64,
    pub expenses: f64,
    pub net: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BudgetOrg {
    pub id: f64,
    pub name: String,
    pub currency: String,
    pub year: f64,
    pub income: f64,
    pub expenses: f64,
    pub net: f64,
    pub savings_rate: f64,
    pub ending_balance: f64,
    pub carry_over: f64,
    pub current_month: BudgetMonth,
    pub monthly_average: BudgetAverage,
    pub top_expenses: Vec<BudgetShare>,
    pub top_income: Vec<BudgetShare>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct BudgetOverview {
    pub organizations: Vec<BudgetOrg>,
}

// ---------- Server functions ----------

#[server]
pub async fn integrations_get() -> Result<Vec<IntegrationInfo>, ServerFnError> {
    Ok(crate::server::integrations::infos())
}

/// Saves URL/token (empty token keeps the current one) and tests the connection.
#[server]
pub async fn integration_save(kind: Integration, url: String, token: String) -> Result<String, ServerFnError> {
    crate::server::integrations::save(kind, &url, &token).map_err(err)?;
    crate::server::integrations::test(kind).await.map_err(err)
}

#[server]
pub async fn dns_widget(range: String) -> Result<DnsWidgetData, ServerFnError> {
    use crate::server::integrations::call_as;
    let range = match range.as_str() {
        "hour" | "week" | "month" => range,
        _ => "day".to_string(),
    };
    let (overview, stats) = tokio::join!(
        call_as::<DnsOverview>(Integration::Dns, "get_overview", json!({ "range": range })),
        call_as::<serde_json::Value>(Integration::Dns, "get_statistics", json!({ "range": range, "includeSeries": true })),
    );
    let series = stats
        .ok()
        .and_then(|s| s.get("series").cloned())
        .and_then(|s| serde_json::from_value(s).ok())
        .unwrap_or_default();
    Ok(DnsWidgetData { overview: overview.map_err(err)?, series })
}

#[server]
pub async fn budget_overview() -> Result<BudgetOverview, ServerFnError> {
    crate::server::integrations::call_as(Integration::Budget, "get_overview", json!({})).await.map_err(err)
}
