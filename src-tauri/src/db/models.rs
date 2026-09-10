use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const AUTH_BEARER: &str = "bearer";
pub const PROTOCOL_OPENAI: &str = "openai";
pub const PROTOCOL_ANTHROPIC: &str = "anthropic";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub auth_scheme: String,
    pub protocol: String,
    pub extra_headers: BTreeMap<String, String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_auth_scheme")]
    pub auth_scheme: String,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub extra_headers: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_auth_scheme() -> String {
    AUTH_BEARER.to_string()
}

fn default_protocol() -> String {
    PROTOCOL_OPENAI.to_string()
}

fn default_true() -> bool {
    true
}

fn parse_headers(raw: &str) -> BTreeMap<String, String> {
    serde_json::from_str(raw).unwrap_or_default()
}

impl Provider {
    pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            base_url: row.get("base_url")?,
            api_key: row.get("api_key")?,
            auth_scheme: row.get("auth_scheme")?,
            protocol: row.get("protocol")?,
            extra_headers: parse_headers(&row.get::<_, String>("extra_headers")?),
            enabled: row.get::<_, i64>("enabled")? != 0,
            created_at: row.get("created_at")?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamModel {
    pub id: String,
    pub provider_id: String,
    pub model_id: String,
    pub display_name: String,
    pub input_price: f64,
    pub output_price: f64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamModelInput {
    pub id: Option<String>,
    pub provider_id: String,
    pub model_id: String,
    pub display_name: String,
    #[serde(default)]
    pub input_price: f64,
    #[serde(default)]
    pub output_price: f64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl UpstreamModel {
    pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            provider_id: row.get("provider_id")?,
            model_id: row.get("model_id")?,
            display_name: row.get("display_name")?,
            input_price: row.get("input_price")?,
            output_price: row.get("output_price")?,
            enabled: row.get::<_, i64>("enabled")? != 0,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub id: String,
    pub alias: String,
    pub display_name: String,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteTarget {
    pub id: String,
    pub route_id: String,
    pub upstream_model_id: String,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteWithTargets {
    #[serde(flatten)]
    pub route: Route,
    pub targets: Vec<RouteTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteInput {
    pub id: Option<String>,
    pub alias: String,
    pub display_name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub targets: Vec<RouteTargetInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteTargetInput {
    pub upstream_model_id: String,
    #[serde(default)]
    pub priority: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Route {
    pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            alias: row.get("alias")?,
            display_name: row.get("display_name")?,
            enabled: row.get::<_, i64>("enabled")? != 0,
            created_at: row.get("created_at")?,
        })
    }
}

impl RouteTarget {
    pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            route_id: row.get("route_id")?,
            upstream_model_id: row.get("upstream_model_id")?,
            priority: row.get("priority")?,
            enabled: row.get::<_, i64>("enabled")? != 0,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualKey {
    pub id: String,
    pub key: String,
    pub name: String,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualKeyInput {
    pub id: Option<String>,
    pub key: Option<String>,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl VirtualKey {
    pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            key: row.get("key")?,
            name: row.get("name")?,
            enabled: row.get::<_, i64>("enabled")? != 0,
            created_at: row.get("created_at")?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLog {
    pub id: String,
    pub occurred_at: String,
    pub endpoint: String,
    pub method: String,
    pub route_alias: Option<String>,
    pub route_id: Option<String>,
    pub upstream_model_id: Option<String>,
    pub upstream_model_name: Option<String>,
    pub provider_id: Option<String>,
    pub virtual_key_id: Option<String>,
    pub kind: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cost: f64,
    pub status: String,
    pub http_status: Option<i64>,
    pub latency_ms: Option<i64>,
    pub error_message: Option<String>,
    pub is_stream: bool,
}

impl RequestLog {
    pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            occurred_at: row.get("occurred_at")?,
            endpoint: row.get("endpoint")?,
            method: row.get("method")?,
            route_alias: row.get("route_alias")?,
            route_id: row.get("route_id")?,
            upstream_model_id: row.get("upstream_model_id")?,
            upstream_model_name: row.get("upstream_model_name")?,
            provider_id: row.get("provider_id")?,
            virtual_key_id: row.get("virtual_key_id")?,
            kind: row.get("kind")?,
            input_tokens: row.get("input_tokens")?,
            output_tokens: row.get("output_tokens")?,
            total_tokens: row.get("total_tokens")?,
            cost: row.get("cost")?,
            status: row.get("status")?,
            http_status: row.get("http_status")?,
            latency_ms: row.get("latency_ms")?,
            error_message: row.get("error_message")?,
            is_stream: row.get::<_, i64>("is_stream")? != 0,
        })
    }
}
