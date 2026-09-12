use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const AUTH_BEARER: &str = "bearer";
pub const PROTOCOL_OPENAI: &str = "openai";
pub const PROTOCOL_ANTHROPIC: &str = "anthropic";
pub const PROTOCOL_RESPONSES: &str = "responses";
pub const PROTOCOL_GEMINI: &str = "gemini";
pub const ICON_TINT_INK: &str = "ink";
pub const QUOTA_PERIOD_DAILY: &str = "daily";
pub const QUOTA_PERIOD_WEEKLY: &str = "weekly";
pub const QUOTA_PERIOD_MONTHLY: &str = "monthly";
pub const QUOTA_PERIOD_TOTAL: &str = "total";

/// 上游模型能力标签的权威词表。前端 `providerModel.ts` 的 `capabilityOrder` 是它的
/// 镜像，变更需两侧同步（`save_upstream_model` 会拒绝词表外的取值）。
pub const CAPABILITY_VISION: &str = "vision";
pub const CAPABILITY_TOOLS: &str = "tools";
pub const CAPABILITY_REASONING: &str = "reasoning";
pub const MODEL_CAPABILITIES: [&str; 3] =
    [CAPABILITY_VISION, CAPABILITY_TOOLS, CAPABILITY_REASONING];

pub fn is_known_capability(value: &str) -> bool {
    MODEL_CAPABILITIES.contains(&value)
}

/// 入站协议是否受网关支持。路由保存与种子导入时据此校验。
pub fn is_known_protocol(protocol: &str) -> bool {
    matches!(
        protocol,
        PROTOCOL_OPENAI | PROTOCOL_ANTHROPIC | PROTOCOL_RESPONSES | PROTOCOL_GEMINI
    )
}

/// 输入总量是否已包含缓存命中。Anthropic 的 `input_tokens` 不含缓存读取，其余协议
/// （OpenAI / DeepSeek / Responses / Gemini）的输入总量已含命中，计费与命中率据此归一。
pub fn contains_cache_read(protocol: &str) -> bool {
    protocol != PROTOCOL_ANTHROPIC
}

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
    pub icon: Option<String>,
    pub icon_tint: String,
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
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "default_icon_tint")]
    pub icon_tint: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

pub(crate) fn default_auth_scheme() -> String {
    AUTH_BEARER.to_string()
}

pub(crate) fn default_protocol() -> String {
    PROTOCOL_OPENAI.to_string()
}

pub(crate) fn default_icon_tint() -> String {
    ICON_TINT_INK.to_string()
}

pub(crate) fn default_true() -> bool {
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
            icon: row.get("icon")?,
            icon_tint: row.get("icon_tint")?,
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
    pub cache_read_price: f64,
    pub cache_creation_price: f64,
    pub context_window: i64,
    pub capabilities: Vec<String>,
    pub icon: Option<String>,
    pub icon_tint: String,
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
    #[serde(default)]
    pub cache_read_price: f64,
    #[serde(default)]
    pub cache_creation_price: f64,
    #[serde(default)]
    pub context_window: i64,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "default_icon_tint")]
    pub icon_tint: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn parse_capabilities(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
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
            cache_read_price: row.get("cache_read_price")?,
            cache_creation_price: row.get("cache_creation_price")?,
            context_window: row.get("context_window")?,
            capabilities: parse_capabilities(&row.get::<_, String>("capabilities")?),
            icon: row.get("icon")?,
            icon_tint: row.get("icon_tint")?,
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
    pub protocol: String,
    /// 显式选择的品牌图标；`None` 时回落到首选目标上游模型的图标。
    pub icon: Option<String>,
    /// `None` = 继承首选目标模型的着色；`"ink"` / `"brand"` = 显式。
    pub icon_tint: Option<String>,
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
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub icon_tint: Option<String>,
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
            protocol: row.get("protocol")?,
            icon: row.get("icon")?,
            icon_tint: row.get("icon_tint")?,
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
    pub quota_limit: Option<f64>,
    pub quota_period: String,
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
    #[serde(default)]
    pub quota_limit: Option<f64>,
    #[serde(default = "default_quota_period")]
    pub quota_period: String,
}

pub(crate) fn default_quota_period() -> String {
    QUOTA_PERIOD_MONTHLY.to_string()
}

impl VirtualKey {
    pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            key: row.get("key")?,
            name: row.get("name")?,
            enabled: row.get::<_, i64>("enabled")? != 0,
            quota_limit: row.get("quota_limit")?,
            quota_period: row.get("quota_period")?,
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
    pub model_real: Option<String>,
    pub provider_id: Option<String>,
    pub virtual_key_id: Option<String>,
    pub kind: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    /// 输入总量是否已包含缓存命中（OpenAI / DeepSeek / Responses / Gemini 为真，
    /// Anthropic 为假）。命中率的分母按此边界计算。
    pub cache_read_in_input: bool,
    pub reasoning_tokens: i64,
    pub cost: f64,
    pub usage_source: String,
    pub status: String,
    pub http_status: Option<i64>,
    pub latency_ms: Option<i64>,
    pub error_message: Option<String>,
    pub request_id: Option<String>,
    pub is_stream: bool,
    /// 本次客户端请求内的上游尝试序号，从 0 起。降级链中失败与成功的尝试各占一条。
    pub attempt_index: i64,
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
            model_real: row.get("model_real")?,
            provider_id: row.get("provider_id")?,
            virtual_key_id: row.get("virtual_key_id")?,
            kind: row.get("kind")?,
            input_tokens: row.get("input_tokens")?,
            output_tokens: row.get("output_tokens")?,
            total_tokens: row.get("total_tokens")?,
            cache_read_tokens: row.get("cache_read_tokens")?,
            cache_creation_tokens: row.get("cache_creation_tokens")?,
            cache_read_in_input: row.get::<_, i64>("cache_read_in_input")? != 0,
            reasoning_tokens: row.get("reasoning_tokens")?,
            cost: row.get("cost")?,
            usage_source: row.get("usage_source")?,
            status: row.get("status")?,
            http_status: row.get("http_status")?,
            latency_ms: row.get("latency_ms")?,
            error_message: row.get("error_message")?,
            request_id: row.get("request_id")?,
            is_stream: row.get::<_, i64>("is_stream")? != 0,
            attempt_index: row.get("attempt_index")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_protocols_cover_all_four() {
        for protocol in [
            PROTOCOL_OPENAI,
            PROTOCOL_ANTHROPIC,
            PROTOCOL_RESPONSES,
            PROTOCOL_GEMINI,
        ] {
            assert!(is_known_protocol(protocol), "{protocol} 应被识别");
        }
        assert!(!is_known_protocol("cohere"));
        assert!(!is_known_protocol(""));
    }

    #[test]
    fn cache_boundary_only_excludes_anthropic() {
        assert!(!contains_cache_read(PROTOCOL_ANTHROPIC));
        assert!(contains_cache_read(PROTOCOL_OPENAI));
        assert!(contains_cache_read(PROTOCOL_RESPONSES));
        assert!(contains_cache_read(PROTOCOL_GEMINI));
    }

    #[test]
    fn known_capabilities_match_pinned_list() {
        // 前端 providerModel.ts 的 capabilityOrder 必须与此一致。
        assert_eq!(MODEL_CAPABILITIES, ["vision", "tools", "reasoning"]);
        for value in MODEL_CAPABILITIES {
            assert!(is_known_capability(value));
        }
        assert!(!is_known_capability("audio"));
        assert!(!is_known_capability(""));
    }
}
