use serde_json::Value;

use crate::db::logs::insert_log;
use crate::db::models::{contains_cache_read, RequestLog, PROTOCOL_GEMINI};
use crate::db::with_db;
use crate::error::AppError;
use crate::gateway::estimate::Estimates;
use crate::gateway::resolve::ResolvedRoute;
use crate::state::AppState;

pub const PER_MILLION: f64 = 1_000_000.0;

/// 用量的来源与可信度。缺失的字段一律不折算为 0。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UsageSource {
    /// 上游明确返回，可直接结算。
    Provider,
    /// 网关本地估算：上游未返回用量、或只返回部分用量时，用请求 / 响应文本补齐。
    Estimated,
    /// 上游未返回任何可用用量。
    #[default]
    Missing,
    /// 有部分真实用量，但流未正常收尾，不得视为完整。
    Partial,
}

impl UsageSource {
    pub fn as_str(self) -> &'static str {
        match self {
            UsageSource::Provider => "provider",
            UsageSource::Estimated => "estimated",
            UsageSource::Partial => "partial",
            UsageSource::Missing => "missing",
        }
    }
}

/// 六类规范量 + 来源标记。所有缓存/推理量都保留，避免「缺失即 0」。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UsageTotals {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub reasoning_tokens: i64,
    /// 输入总量是否已包含缓存命中部分（OpenAI/DeepSeek 为真，Anthropic 为假）。
    pub contains_cache_read: bool,
    pub source: UsageSource,
}

impl UsageTotals {
    pub fn missing() -> Self {
        Self {
            source: UsageSource::Missing,
            ..Self::default()
        }
    }
}

/// 从响应 JSON 中解析出的「原始存在性」字段；`None` 表示上游未返回，而非 0。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UsageFields {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub total: Option<i64>,
    pub cache_read: Option<i64>,
    pub cache_creation: Option<i64>,
    pub reasoning: Option<i64>,
}

impl UsageFields {
    pub fn is_empty(&self) -> bool {
        self.input.is_none()
            && self.output.is_none()
            && self.total.is_none()
            && self.cache_read.is_none()
            && self.cache_creation.is_none()
            && self.reasoning.is_none()
    }

    /// 流式合并：上游后续出现的字段「覆盖」先前值（Anthropic 的 output 为累计值，
    /// cache 字段保留最新非空），缺省字段沿用旧值。绝不能做无差别的整块替换或逐字段取大。
    pub fn merge(self, next: UsageFields) -> UsageFields {
        UsageFields {
            input: next.input.or(self.input),
            output: next.output.or(self.output),
            total: next.total.or(self.total),
            cache_read: next.cache_read.or(self.cache_read),
            cache_creation: next.cache_creation.or(self.cache_creation),
            reasoning: next.reasoning.or(self.reasoning),
        }
    }
}

/// 只接受非负整数；字符串数字显式解析；`null`/负数/其它类型一律视为缺失。
fn token(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    let parsed = match value {
        Value::Number(number) => number.as_i64()?,
        Value::String(text) => text.trim().parse::<i64>().ok()?,
        _ => return None,
    };
    (parsed >= 0).then_some(parsed)
}

/// 跨厂商提取六类量，覆盖 OpenAI Chat / Responses / Anthropic / DeepSeek / Gemini
/// 的常见字段。字段名只作「存在即取值」，缺失一律留 `None`，绝不补 0。
pub fn extract_fields(usage: &Value) -> UsageFields {
    UsageFields {
        input: token(
            usage
                .get("prompt_tokens")
                .or_else(|| usage.get("input_tokens"))
                .or_else(|| usage.get("promptTokenCount")),
        ),
        output: token(
            usage
                .get("completion_tokens")
                .or_else(|| usage.get("output_tokens"))
                .or_else(|| usage.get("candidatesTokenCount")),
        ),
        total: token(usage.get("total_tokens").or_else(|| usage.get("totalTokenCount"))),
        cache_read: token(
            usage
                .get("cache_read_input_tokens")
                .or_else(|| usage.pointer("/prompt_tokens_details/cached_tokens"))
                .or_else(|| usage.pointer("/input_tokens_details/cached_tokens"))
                .or_else(|| usage.get("prompt_cache_hit_tokens"))
                .or_else(|| usage.get("cachedContentTokenCount")),
        ),
        cache_creation: token(
            usage
                .get("cache_creation_input_tokens")
                .or_else(|| usage.pointer("/prompt_tokens_details/cache_write_tokens")),
        ),
        reasoning: token(
            usage
                .pointer("/completion_tokens_details/reasoning_tokens")
                .or_else(|| usage.pointer("/output_tokens_details/reasoning_tokens"))
                .or_else(|| usage.get("thoughtsTokenCount")),
        ),
    }
}

/// 把存在性字段收敛为规范量。输入与输出都缺失时返回 `None`（无法结算）。
#[cfg(test)]
pub fn finalize(fields: UsageFields, contains_cache_read: bool) -> Option<UsageTotals> {
    finalize_with_estimate(fields, contains_cache_read, false)
}

/// 与 `finalize` 相同，但当缺失的边由网关估算补齐时（`estimated = true`），来源标为
/// `Estimated` 而非 `Provider` / `Partial`，让账本能把估算值与上游原值分开。
pub fn finalize_with_estimate(
    fields: UsageFields,
    contains_cache_read: bool,
    estimated: bool,
) -> Option<UsageTotals> {
    if fields.input.is_none() && fields.output.is_none() && fields.total.is_none() {
        return None;
    }
    let input = fields.input.unwrap_or(0);
    let output = fields.output.unwrap_or(0);
    let cache_read = fields.cache_read.unwrap_or(0);
    let cache_creation = fields.cache_creation.unwrap_or(0);
    let computed_total = if contains_cache_read {
        input + output
    } else {
        input + cache_read + cache_creation + output
    };
    let source = if estimated {
        UsageSource::Estimated
    } else if fields.input.is_some() && fields.output.is_some() {
        UsageSource::Provider
    } else {
        UsageSource::Partial
    };
    Some(UsageTotals {
        input_tokens: input,
        output_tokens: output,
        total_tokens: fields.total.unwrap_or(computed_total),
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        reasoning_tokens: fields.reasoning.unwrap_or(0),
        contains_cache_read,
        source,
    })
}

/// 用估算值补上上游缺失的边；返回是否真的用到了估算（用于决定来源标记）。
pub fn apply_estimates(fields: &mut UsageFields, estimates: Estimates) -> bool {
    let mut used = false;
    if fields.input.is_none() {
        if let Some(value) = estimates.input {
            fields.input = Some(value);
            used = true;
        }
    }
    if fields.output.is_none() {
        if let Some(value) = estimates.output {
            fields.output = Some(value);
            used = true;
        }
    }
    used
}

/// 按协议选定用量容器：Gemini 原生用 `usageMetadata`，其余用 `usage`。
fn usage_container<'a>(value: &'a Value, protocol: &str) -> Option<&'a Value> {
    let key = if protocol == PROTOCOL_GEMINI {
        "usageMetadata"
    } else {
        "usage"
    };
    value.get(key).filter(|usage| !usage.is_null())
}

/// 从完整响应中提取规范用量（非流式）。容器与缓存边界都由协议决定。
#[cfg(test)]
pub fn extract_usage(value: &Value, protocol: &str) -> Option<UsageTotals> {
    let usage = usage_container(value, protocol)?;
    finalize(extract_fields(usage), contains_cache_read(protocol))
}

/// 非流式：提取上游用量，缺失的边用估算补齐。上游完全未返回时，只要估算可用，
/// 仍产出一条 `estimated` 记录——网关不做「上游不给就放弃」。
pub fn extract_usage_estimated(
    value: &Value,
    protocol: &str,
    estimates: Estimates,
) -> UsageTotals {
    let mut fields = usage_container(value, protocol)
        .map(extract_fields)
        .unwrap_or_default();
    let used = apply_estimates(&mut fields, estimates);
    finalize_with_estimate(fields, contains_cache_read(protocol), used)
        .unwrap_or_else(UsageTotals::missing)
}

#[derive(Debug, Clone, Copy)]
pub struct Pricing {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_creation: f64,
}

/// 计费：先按边界判断命中是否已含在输入中，再分别计价。
/// 缓存单价为 0 时回退到输入单价，保证「未配置缓存价」不会把命中算成免费。
/// 结果保留 6 位小数，避免浮点尾差进入数据库。
pub fn calculate_cost(usage: &UsageTotals, price: &Pricing) -> f64 {
    let cache_read_price = if price.cache_read > 0.0 {
        price.cache_read
    } else {
        price.input
    };
    let cache_creation_price = if price.cache_creation > 0.0 {
        price.cache_creation
    } else {
        price.input
    };
    let billable_input = if usage.contains_cache_read {
        (usage.input_tokens - usage.cache_read_tokens).max(0)
    } else {
        usage.input_tokens
    };
    let cost = billable_input as f64 / PER_MILLION * price.input
        + usage.cache_read_tokens as f64 / PER_MILLION * cache_read_price
        + usage.cache_creation_tokens as f64 / PER_MILLION * cache_creation_price
        + usage.output_tokens as f64 / PER_MILLION * price.output;
    (cost * 1_000_000.0).round() / 1_000_000.0
}

#[derive(Clone)]
pub struct LogContext {
    pub endpoint: String,
    pub alias: String,
    pub is_stream: bool,
    pub route: Option<ResolvedRoute>,
    pub latency_ms: i64,
    pub status: String,
    pub http_status: Option<i64>,
    pub error_message: Option<String>,
    pub request_id: Option<String>,
    pub virtual_key_id: Option<String>,
    pub usage: UsageTotals,
    /// 本次客户端请求内的上游尝试序号，从 0 起；降级时递增。
    pub attempt_index: i64,
    /// 会话标识：由候选会话头名解析而来，每次尝试都带（成功与失败均记，便于按会话追踪）。
    pub session_id: Option<String>,
}

pub fn build_log(context: LogContext) -> RequestLog {
    let LogContext {
        endpoint,
        alias,
        is_stream,
        route,
        latency_ms,
        status,
        http_status,
        error_message,
        request_id,
        virtual_key_id,
        usage,
        attempt_index,
        session_id,
    } = context;
    // 只有拿到 input/output 或缓存计数时才能按 token 计价。仅有 total_tokens 的
    // 响应无法拆分计价，保守记 0，并保留 usage_source = partial 供账本筛出。
    let billable = usage.input_tokens > 0
        || usage.output_tokens > 0
        || usage.cache_read_tokens > 0
        || usage.cache_creation_tokens > 0;
    let cost = match (&route, usage.source) {
        (Some(_), UsageSource::Missing) => 0.0,
        (Some(_), _) if !billable => 0.0,
        (Some(route), _) => calculate_cost(
            &usage,
            &Pricing {
                input: route.input_price,
                output: route.output_price,
                cache_read: route.cache_read_price,
                cache_creation: route.cache_creation_price,
            },
        ),
        (None, _) => 0.0,
    };

    RequestLog {
        id: uuid::Uuid::new_v4().to_string(),
        occurred_at: chrono::Utc::now().to_rfc3339(),
        endpoint,
        method: "POST".to_string(),
        route_alias: Some(alias),
        route_id: route.as_ref().map(|route| route.route_id.clone()),
        upstream_model_id: route.as_ref().map(|route| route.upstream_model_id.clone()),
        upstream_model_name: route.as_ref().map(|route| route.display_name.clone()),
        model_real: route.as_ref().map(|route| route.model_id.clone()),
        provider_id: route.as_ref().map(|route| route.provider_id.clone()),
        virtual_key_id,
        kind: "chat".to_string(),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        cache_read_tokens: usage.cache_read_tokens,
        cache_creation_tokens: usage.cache_creation_tokens,
        cache_read_in_input: usage.contains_cache_read,
        reasoning_tokens: usage.reasoning_tokens,
        cost,
        usage_source: usage.source.as_str().to_string(),
        status,
        http_status,
        latency_ms: Some(latency_ms),
        ttfb_ms: None,
        error_message,
        request_id,
        is_stream,
        attempt_index,
        session_id,
    }
}

pub async fn record(state: &AppState, log: RequestLog) -> Result<(), AppError> {
    let to_store = log.clone();
    with_db(&state.db, move |conn| insert_log(conn, &to_store)).await?;
    state.events.log(&log);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI, PROTOCOL_RESPONSES};
    use serde_json::json;

    fn pricing(input: f64, output: f64) -> Pricing {
        Pricing {
            input,
            output,
            cache_read: 0.0,
            cache_creation: 0.0,
        }
    }

    fn totals(input: i64, output: i64, contains_cache_read: bool) -> UsageTotals {
        UsageTotals {
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
            contains_cache_read,
            source: UsageSource::Provider,
            ..UsageTotals::default()
        }
    }

    #[test]
    fn cost_uses_per_million_pricing() {
        assert_eq!(
            calculate_cost(&totals(1_000_000, 1_000_000, true), &pricing(3.0, 15.0)),
            18.0
        );
        assert_eq!(
            calculate_cost(&totals(0, 0, true), &pricing(3.0, 15.0)),
            0.0
        );
    }

    #[test]
    fn cost_rounds_to_six_decimals() {
        assert_eq!(
            calculate_cost(&totals(1, 0, true), &pricing(3.0, 0.0)),
            0.000003
        );
    }

    #[test]
    fn cost_subtracts_cache_when_input_contains_it() {
        let mut usage = totals(100, 0, true);
        usage.cache_read_tokens = 60;
        // 未配置缓存价：命中回退输入价，总价与全按输入计相同，不重复计费。
        assert_eq!(calculate_cost(&usage, &pricing(3.0, 0.0)), 0.0003);

        let price = Pricing {
            input: 3.0,
            output: 0.0,
            cache_read: 1.0,
            cache_creation: 0.0,
        };
        // 未命中 40 × 3 + 命中 60 × 1，每百万。
        assert_eq!(calculate_cost(&usage, &price), 0.00018);
    }

    #[test]
    fn cost_keeps_cache_separate_when_input_excludes_it() {
        let mut usage = totals(100, 0, false);
        usage.cache_read_tokens = 60;
        let price = Pricing {
            input: 3.0,
            output: 0.0,
            cache_read: 1.0,
            cache_creation: 0.0,
        };
        // Anthropic 语义：输入不含缓存读取，两部分相加。
        assert_eq!(calculate_cost(&usage, &price), 0.00036);
    }

    #[test]
    fn extract_usage_accepts_openai_naming() {
        let value = json!({ "usage": { "prompt_tokens": 12, "completion_tokens": 8, "total_tokens": 20 } });
        let usage = extract_usage(&value, PROTOCOL_OPENAI).unwrap();
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 8);
        assert_eq!(usage.total_tokens, 20);
        assert_eq!(usage.source, UsageSource::Provider);
    }

    #[test]
    fn extract_usage_reads_cache_and_reasoning_fields() {
        let value = json!({
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 20,
                "total_tokens": 120,
                "prompt_tokens_details": { "cached_tokens": 64 },
                "completion_tokens_details": { "reasoning_tokens": 5 }
            }
        });
        let usage = extract_usage(&value, PROTOCOL_OPENAI).unwrap();
        assert_eq!(usage.cache_read_tokens, 64);
        assert_eq!(usage.reasoning_tokens, 5);
    }

    #[test]
    fn extract_usage_reads_anthropic_cache_fields() {
        let value = json!({
            "usage": {
                "input_tokens": 500,
                "output_tokens": 120,
                "cache_read_input_tokens": 100,
                "cache_creation_input_tokens": 400
            }
        });
        let usage = extract_usage(&value, PROTOCOL_ANTHROPIC).unwrap();
        assert_eq!(usage.input_tokens, 500);
        assert_eq!(usage.cache_read_tokens, 100);
        assert_eq!(usage.cache_creation_tokens, 400);
        // 输入不含缓存：总量应把读取与写入补回。
        assert_eq!(usage.total_tokens, 1120);
    }

    #[test]
    fn extract_usage_treats_negative_and_null_as_missing() {
        assert_eq!(extract_usage(&json!({ "usage": null }), PROTOCOL_OPENAI), None);
        let value = json!({ "usage": { "prompt_tokens": -1, "completion_tokens": -1 } });
        assert_eq!(extract_usage(&value, PROTOCOL_OPENAI), None);
        let partial = json!({ "usage": { "prompt_tokens": -1, "completion_tokens": 7 } });
        let usage = extract_usage(&partial, PROTOCOL_OPENAI).unwrap();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 7);
        assert_eq!(usage.source, UsageSource::Partial);
    }

    #[test]
    fn extract_usage_parses_string_numbers() {
        let value = json!({ "usage": { "prompt_tokens": "12", "completion_tokens": "8" } });
        let usage = extract_usage(&value, PROTOCOL_OPENAI).unwrap();
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 8);
    }

    #[test]
    fn extract_usage_returns_none_without_usage() {
        assert_eq!(extract_usage(&json!({ "choices": [] }), PROTOCOL_OPENAI), None);
    }

    #[test]
    fn extract_usage_reads_responses_fields() {
        let value = json!({
            "output": [],
            "usage": {
                "input_tokens": 75,
                "input_tokens_details": { "cached_tokens": 20 },
                "output_tokens": 1186,
                "output_tokens_details": { "reasoning_tokens": 1024 },
                "total_tokens": 1261
            }
        });
        let usage = extract_usage(&value, PROTOCOL_RESPONSES).unwrap();
        assert_eq!(usage.input_tokens, 75);
        assert_eq!(usage.output_tokens, 1186);
        assert_eq!(usage.cache_read_tokens, 20);
        assert_eq!(usage.reasoning_tokens, 1024);
        assert!(usage.contains_cache_read);
    }

    #[test]
    fn extract_usage_reads_gemini_usage_metadata() {
        let value = json!({
            "candidates": [],
            "usageMetadata": {
                "promptTokenCount": 100,
                "cachedContentTokenCount": 60,
                "candidatesTokenCount": 20,
                "thoughtsTokenCount": 8,
                "totalTokenCount": 133
            }
        });
        let usage = extract_usage(&value, PROTOCOL_GEMINI).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 20);
        assert_eq!(usage.cache_read_tokens, 60);
        assert_eq!(usage.reasoning_tokens, 8);
        assert_eq!(usage.total_tokens, 133);
        assert!(usage.contains_cache_read);
        // Gemini 无独立写入量，不得臆造。
        assert_eq!(usage.cache_creation_tokens, 0);
    }

    #[test]
    fn merge_overwrites_present_fields_only() {
        let start = UsageFields {
            input: Some(25),
            output: Some(1),
            cache_read: Some(10),
            ..UsageFields::default()
        };
        let delta = UsageFields {
            output: Some(15),
            ..UsageFields::default()
        };
        let merged = start.merge(delta);
        assert_eq!(merged.input, Some(25));
        assert_eq!(merged.output, Some(15));
        assert_eq!(merged.cache_read, Some(10));
    }

    fn sample_route() -> ResolvedRoute {
        ResolvedRoute {
            route_id: "r1".into(),
            upstream_model_id: "m1".into(),
            model_id: "gpt-x".into(),
            display_name: "GPT X".into(),
            input_price: 3.0,
            output_price: 15.0,
            cache_read_price: 0.0,
            cache_creation_price: 0.0,
            provider_id: "p1".into(),
            base_url: "https://example.com/v1".into(),
            api_key: "secret".into(),
            auth_scheme: "bearer".into(),
            upstream_protocol: PROTOCOL_OPENAI.into(),
            extra_headers: std::collections::BTreeMap::new(),
            header_rules: Default::default(),
        }
    }

    #[test]
    fn total_only_usage_is_partial_and_unbilled() {
        let usage =
            extract_usage(&json!({ "usage": { "total_tokens": 150 } }), PROTOCOL_OPENAI).unwrap();
        assert_eq!(usage.total_tokens, 150);
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.source, UsageSource::Partial);

        let log = build_log(LogContext {
            endpoint: "/v1/chat/completions".into(),
            alias: "lumen/x".into(),
            is_stream: false,
            route: Some(sample_route()),
            latency_ms: 1,
            status: "success".into(),
            http_status: Some(200),
            error_message: None,
            request_id: None,
            virtual_key_id: None,
            usage,
            attempt_index: 0,
            session_id: None,
        });
        // 拆分未知 → 保守不结算，但保留 total 与 partial 标记。
        assert_eq!(log.cost, 0.0);
        assert_eq!(log.total_tokens, 150);
        assert_eq!(log.usage_source, "partial");
    }

    #[test]
    fn apply_estimates_fills_only_missing_sides() {
        let mut fields = UsageFields::default();
        let used = apply_estimates(
            &mut fields,
            Estimates {
                input: Some(30),
                output: Some(7),
            },
        );
        assert!(used);
        assert_eq!(fields.input, Some(30));
        assert_eq!(fields.output, Some(7));

        let mut partial = UsageFields {
            input: Some(10),
            ..UsageFields::default()
        };
        let used = apply_estimates(
            &mut partial,
            Estimates {
                input: Some(99),
                output: Some(7),
            },
        );
        assert!(used);
        // 上游已给的边不被估算覆盖。
        assert_eq!(partial.input, Some(10));
        assert_eq!(partial.output, Some(7));
    }

    #[test]
    fn estimated_usage_is_billable_and_tagged() {
        let usage = extract_usage_estimated(
            &json!({ "choices": [] }),
            PROTOCOL_OPENAI,
            Estimates {
                input: Some(1000),
                output: Some(500),
            },
        );
        assert_eq!(usage.source, UsageSource::Estimated);
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 500);

        let log = build_log(LogContext {
            endpoint: "/v1/chat/completions".into(),
            alias: "lumen/x".into(),
            is_stream: false,
            route: Some(sample_route()),
            latency_ms: 1,
            status: "success".into(),
            http_status: Some(200),
            error_message: None,
            request_id: None,
            virtual_key_id: None,
            usage,
            attempt_index: 0,
            session_id: None,
        });
        // 估算值计入花费：账本不因上游漏报而归零。
        assert!(log.cost > 0.0);
        assert_eq!(log.usage_source, "estimated");
    }

    #[test]
    fn provider_usage_survives_estimates() {
        let value = json!({ "usage": { "prompt_tokens": 12, "completion_tokens": 8 } });
        let usage = extract_usage_estimated(
            &value,
            PROTOCOL_OPENAI,
            Estimates {
                input: Some(999),
                output: Some(999),
            },
        );
        assert_eq!(usage.source, UsageSource::Provider);
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 8);
    }

    #[test]
    fn partial_usage_is_completed_by_estimates() {
        // 上游只给了 total：两侧都用估算补齐，并标为估算。
        let value = json!({ "usage": { "total_tokens": 150 } });
        let usage = extract_usage_estimated(
            &value,
            PROTOCOL_OPENAI,
            Estimates {
                input: Some(100),
                output: Some(50),
            },
        );
        assert_eq!(usage.source, UsageSource::Estimated);
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }
}
