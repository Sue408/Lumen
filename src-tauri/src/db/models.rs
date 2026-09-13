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
    pub api_key: String,
    pub extra_headers: BTreeMap<String, String>,
    pub header_rules: ProviderHeaderRules,
    pub icon: Option<String>,
    pub icon_tint: String,
    pub enabled: bool,
    pub created_at: String,
}

/// 一个提供商对外提供的某协议端点：协议 + 该协议的上游地址与鉴权方式。
/// 同一提供商的多种协议共享连接身份（名称 / 密钥 / 请求头），仅端点不同。
/// 自然键 `(provider_id, protocol)`：一个提供商每种协议至多一个端点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEndpoint {
    pub id: String,
    pub provider_id: String,
    pub protocol: String,
    pub base_url: String,
    pub auth_scheme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEndpointInput {
    pub id: Option<String>,
    pub protocol: String,
    pub base_url: String,
    #[serde(default = "default_auth_scheme")]
    pub auth_scheme: String,
}

/// 提供商连同其协议端点。列表 / 保存统一返回此形状，前端一次取全。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderWithEndpoints {
    #[serde(flatten)]
    pub provider: Provider,
    pub endpoints: Vec<ProviderEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub api_key: String,
    /// 该提供商支持的协议端点；至少一条。
    #[serde(default)]
    pub endpoints: Vec<ProviderEndpointInput>,
    #[serde(default)]
    pub extra_headers: BTreeMap<String, String>,
    #[serde(default)]
    pub header_rules: ProviderHeaderRules,
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
            api_key: row.get("api_key")?,
            extra_headers: parse_headers(&row.get::<_, String>("extra_headers")?),
            header_rules: parse_provider_header_rules(&row.get::<_, String>("header_rules")?),
            icon: row.get("icon")?,
            icon_tint: row.get("icon_tint")?,
            enabled: row.get::<_, i64>("enabled")? != 0,
            created_at: row.get("created_at")?,
        })
    }
}

impl ProviderEndpoint {
    pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            provider_id: row.get("provider_id")?,
            protocol: row.get("protocol")?,
            base_url: row.get("base_url")?,
            auth_scheme: row.get("auth_scheme")?,
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

/// per-provider 请求头映射（四意图中的**透传 / 替换 / 移除**；"添加" 复用
/// `Provider.extra_headers`）。空规则序列化为 `{}`。
///
/// 求值顺序（见 `gateway::headers::build_upstream_headers`）：
/// 内置底座 → `forward` → `replace` → `extra_headers` → `remove` → 硬黑名单。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderHeaderRules {
    /// 透传：放行客户端头（通配，如 `session_*` / `session_id`）。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub forward: Vec<String>,
    /// 替换：把客户端头 `from` 以 `to` 的名字发出（有序，后写覆盖先写）。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub replace: Vec<HeaderReplace>,
    /// 移除：从最终结果删除（通配，优先级最高）。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
}

/// 一条替换规则：客户端头 `from` → 上游头 `to`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeaderReplace {
    pub from: String,
    pub to: String,
}

/// 反序列化持久化的 provider 头规则；失败回落空规则并告警，不 panic。
pub fn parse_provider_header_rules(raw: &str) -> ProviderHeaderRules {
    serde_json::from_str(raw).unwrap_or_else(|error| {
        tracing::warn!("provider header_rules 解析失败，回落空规则：{error}");
        ProviderHeaderRules::default()
    })
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
    /// 首字节耗时：流式场景下从上游响应头到达至首个数据块；非流式为 `None`。
    pub ttfb_ms: Option<i64>,
    pub error_message: Option<String>,
    pub request_id: Option<String>,
    pub is_stream: bool,
    /// 本次客户端请求内的上游尝试序号，从 0 起。降级链中失败与成功的尝试各占一条。
    pub attempt_index: i64,
    /// 会话标识：由候选会话头名解析而来，客户端未带时为 `None`。仅作日志维度，
    /// 不实体化、不做生命周期；按 `(virtual_key_id, session_id)` 聚合。
    pub session_id: Option<String>,
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
            ttfb_ms: row.get("ttfb_ms")?,
            error_message: row.get("error_message")?,
            request_id: row.get("request_id")?,
            is_stream: row.get::<_, i64>("is_stream")? != 0,
            attempt_index: row.get("attempt_index")?,
            session_id: row.get("session_id")?,
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
