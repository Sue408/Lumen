use serde_json::Value;

use crate::db::logs::insert_log;
use crate::db::models::RequestLog;
use crate::db::with_db;
use crate::error::AppError;
use crate::gateway::resolve::ResolvedRoute;
use crate::state::AppState;

pub const PER_MILLION: f64 = 1_000_000.0;

/// 费用 = 输入 token 计价 + 输出 token 计价，单价均为「每百万 token」。
/// 结果保留 6 位小数，避免浮点尾差进入数据库。
pub fn calculate_cost(
    input_tokens: i64,
    output_tokens: i64,
    input_price: f64,
    output_price: f64,
) -> f64 {
    let cost = input_tokens as f64 / PER_MILLION * input_price
        + output_tokens as f64 / PER_MILLION * output_price;
    (cost * 1_000_000.0).round() / 1_000_000.0
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UsageTotals {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
}

impl UsageTotals {
    pub fn merged(&self, other: &UsageTotals) -> UsageTotals {
        UsageTotals {
            input_tokens: self.input_tokens.max(other.input_tokens),
            output_tokens: self.output_tokens.max(other.output_tokens),
            total_tokens: self
                .total_tokens
                .max(other.total_tokens),
        }
    }
}

/// 从 OpenAI 兼容响应中提取 usage，同时兼容 `prompt_tokens` 与 `input_tokens` 命名。
pub fn extract_usage(value: &Value) -> Option<UsageTotals> {
    let usage = value.get("usage")?;
    if usage.is_null() {
        return None;
    }
    let input = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let total = usage
        .get("total_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(input + output);
    Some(UsageTotals {
        input_tokens: input,
        output_tokens: output,
        total_tokens: total,
    })
}

pub struct LogContext {
    pub endpoint: String,
    pub method: String,
    pub alias: String,
    pub kind: String,
    pub is_stream: bool,
    pub route: Option<ResolvedRoute>,
    pub latency_ms: i64,
    pub status: String,
    pub http_status: Option<i64>,
    pub error_message: Option<String>,
    pub usage: Option<UsageTotals>,
}

pub fn build_log(context: LogContext) -> RequestLog {
    let LogContext {
        endpoint,
        method,
        alias,
        kind,
        is_stream,
        route,
        latency_ms,
        status,
        http_status,
        error_message,
        usage,
    } = context;

    let totals = usage.unwrap_or_default();
    let cost = route
        .as_ref()
        .map(|route| {
            calculate_cost(
                totals.input_tokens,
                totals.output_tokens,
                route.input_price,
                route.output_price,
            )
        })
        .unwrap_or(0.0);

    RequestLog {
        id: uuid::Uuid::new_v4().to_string(),
        occurred_at: chrono::Utc::now().to_rfc3339(),
        endpoint,
        method,
        route_alias: Some(alias),
        route_id: route.as_ref().map(|route| route.route_id.clone()),
        upstream_model_id: route.as_ref().map(|route| route.upstream_model_id.clone()),
        upstream_model_name: route.as_ref().map(|route| route.display_name.clone()),
        provider_id: route.as_ref().map(|route| route.provider_id.clone()),
        virtual_key_id: None,
        kind,
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        total_tokens: totals.total_tokens,
        cost,
        status,
        http_status,
        latency_ms: Some(latency_ms),
        error_message,
        is_stream,
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
    use serde_json::json;

    #[test]
    fn cost_uses_per_million_pricing() {
        assert_eq!(calculate_cost(1_000_000, 1_000_000, 3.0, 15.0), 18.0);
        assert_eq!(calculate_cost(0, 0, 3.0, 15.0), 0.0);
    }

    #[test]
    fn cost_rounds_to_six_decimals() {
        assert_eq!(calculate_cost(1, 0, 3.0, 0.0), 0.000003);
    }

    #[test]
    fn extract_usage_accepts_openai_naming() {
        let value = json!({ "usage": { "prompt_tokens": 12, "completion_tokens": 8, "total_tokens": 20 } });
        assert_eq!(
            extract_usage(&value),
            Some(UsageTotals { input_tokens: 12, output_tokens: 8, total_tokens: 20 })
        );
    }

    #[test]
    fn extract_usage_accepts_alternate_naming_and_derives_total() {
        let value = json!({ "usage": { "input_tokens": 5, "output_tokens": 7 } });
        assert_eq!(
            extract_usage(&value),
            Some(UsageTotals { input_tokens: 5, output_tokens: 7, total_tokens: 12 })
        );
    }

    #[test]
    fn extract_usage_returns_none_without_usage() {
        assert_eq!(extract_usage(&json!({ "choices": [] })), None);
    }
}
