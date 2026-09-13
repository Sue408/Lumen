use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::{HeaderMap, StatusCode};
use serde::Serialize;
use serde_json::{json, Value};

use crate::db::models::{
    Provider, ProviderEndpoint, PROTOCOL_ANTHROPIC, PROTOCOL_GEMINI, PROTOCOL_RESPONSES,
};
use crate::db::providers::{first_enabled_model, get_provider};
use crate::db::with_db;
use crate::error::AppError;
use crate::gateway::forward::{send, upstream_path};
use crate::gateway::resolve::ResolvedRoute;
use crate::state::AppState;

/// 探测请求的超时：远小于正常转发，避免卡住界面。
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// 手动连通性测试结果。`ok` 表示上游以 2xx 响应了最小请求。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    /// 本次探测所用的协议端点。
    pub protocol: String,
    pub ok: bool,
    pub http_status: Option<u16>,
    pub latency_ms: i64,
    /// 探测所用的上游真实模型名。
    pub model: String,
    pub error: Option<String>,
}

/// 各协议的最小消息请求体。探测只求「发得出去、回得来」，不关心回答内容。
pub fn minimal_body(protocol: &str, model: &str) -> Value {
    match protocol {
        PROTOCOL_ANTHROPIC => json!({
            "model": model,
            "max_tokens": 1,
            "messages": [{ "role": "user", "content": "ping" }]
        }),
        PROTOCOL_RESPONSES => json!({
            "model": model,
            "input": "ping",
            "max_tokens": 1
        }),
        PROTOCOL_GEMINI => json!({
            "contents": [{ "parts": [{ "text": "ping" }] }],
            "generationConfig": { "maxOutputTokens": 1 }
        }),
        _ => json!({
            "model": model,
            "messages": [{ "role": "user", "content": "ping" }],
            "max_tokens": 1
        }),
    }
}

/// 用某个协议端点拼出一次探测所需的 `ResolvedRoute`（只用于 `send`，不落库）。
fn route_for(
    provider: &Provider,
    endpoint: &ProviderEndpoint,
    model_id: &str,
    upstream_model_id: &str,
) -> ResolvedRoute {
    ResolvedRoute {
        route_id: String::new(),
        upstream_model_id: upstream_model_id.to_string(),
        model_id: model_id.to_string(),
        display_name: model_id.to_string(),
        input_price: 0.0,
        output_price: 0.0,
        cache_read_price: 0.0,
        cache_creation_price: 0.0,
        provider_id: provider.id.clone(),
        base_url: endpoint.base_url.clone(),
        api_key: provider.api_key.clone(),
        auth_scheme: endpoint.auth_scheme.clone(),
        route_protocol: endpoint.protocol.clone(),
        upstream_protocol: endpoint.protocol.clone(),
        extra_headers: provider.extra_headers.clone(),
        header_rules: provider.header_rules.clone(),
    }
}

fn describe(status: StatusCode, text: &str) -> String {
    let snippet = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| text.chars().take(200).collect());
    if snippet.trim().is_empty() {
        format!("上游返回 {status}")
    } else {
        format!("上游返回 {status}：{}", snippet.trim())
    }
}

/// 对提供商的**每个启用协议端点**发起一次最小消息请求，逐端点返回结果。
/// **不写日志、不计费、不触发冷却**——它是用户主动的一次性诊断，绝不能污染账本
/// 或干扰真实降级链。
pub async fn probe(
    state: Arc<AppState>,
    provider_id: String,
) -> Result<Vec<ProbeResult>, AppError> {
    let (provider, model) = with_db(&state.db, move |conn| {
        let provider = get_provider(conn, &provider_id)?
            .ok_or_else(|| AppError::message("上游提供商不存在"))?;
        let model = first_enabled_model(conn, &provider_id)?.ok_or_else(|| {
            AppError::message(format!(
                "「{}」名下没有启用的模型，无法探测",
                provider.provider.name
            ))
        })?;
        Ok((provider, model))
    })
    .await?;

    let endpoints: Vec<ProviderEndpoint> = provider
        .endpoints
        .iter()
        .filter(|endpoint| endpoint.enabled)
        .cloned()
        .collect();
    if endpoints.is_empty() {
        return Err(AppError::message(format!(
            "「{}」没有启用的协议端点，无法探测",
            provider.provider.name
        )));
    }

    let mut results = Vec::with_capacity(endpoints.len());
    for endpoint in &endpoints {
        let path = upstream_path(&endpoint.protocol, &model.model_id, false);
        let body = minimal_body(&endpoint.protocol, &model.model_id);
        let route = route_for(&provider.provider, endpoint, &model.model_id, &model.id);
        let headers = HeaderMap::new();

        let started = Instant::now();
        let response = send(&state, &route, &body, &path, Some(PROBE_TIMEOUT), &headers).await;
        let latency_ms = started.elapsed().as_millis() as i64;

        match response {
            Ok(response) => {
                let status = response.status();
                let ok = status.is_success();
                let error = if ok {
                    None
                } else {
                    let text = response.text().await.unwrap_or_default();
                    Some(describe(status, &text))
                };
                results.push(ProbeResult {
                    protocol: endpoint.protocol.clone(),
                    ok,
                    http_status: Some(status.as_u16()),
                    latency_ms,
                    model: model.model_id.clone(),
                    error,
                });
            }
            Err(error) => results.push(ProbeResult {
                protocol: endpoint.protocol.clone(),
                ok: false,
                http_status: None,
                latency_ms,
                model: model.model_id.clone(),
                error: Some(error.to_string()),
            }),
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::PROTOCOL_OPENAI;

    #[test]
    fn minimal_body_covers_each_protocol() {
        let openai = minimal_body(PROTOCOL_OPENAI, "gpt");
        assert_eq!(openai["model"], "gpt");
        assert_eq!(openai["max_tokens"], 1);
        assert!(openai["messages"].is_array());

        let anthropic = minimal_body(PROTOCOL_ANTHROPIC, "claude");
        assert_eq!(anthropic["max_tokens"], 1);
        assert_eq!(anthropic["messages"][0]["role"], "user");

        let responses = minimal_body(PROTOCOL_RESPONSES, "gpt");
        assert_eq!(responses["input"], "ping");

        let gemini = minimal_body(PROTOCOL_GEMINI, "gemini");
        assert_eq!(gemini["generationConfig"]["maxOutputTokens"], 1);
        // Gemini 的模型名在 URL path，body 不带 model。
        assert!(gemini.get("model").is_none());
    }

    #[test]
    fn describe_prefers_upstream_error_message() {
        let text = r#"{"error":{"message":"insufficient quota"}}"#;
        let message = describe(StatusCode::TOO_MANY_REQUESTS, text);
        assert!(message.contains("insufficient quota"));
        assert!(message.contains("429"));
    }
}
