use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use serde::Serialize;
use serde_json::{json, Value};

use crate::db::models::{ProviderEndpoint, PROTOCOL_ANTHROPIC, PROTOCOL_GEMINI, PROTOCOL_RESPONSES};
use crate::db::providers::{enabled_model_by_model_id, first_enabled_model, get_provider};
use crate::db::with_db;
use crate::error::AppError;
use crate::gateway::forward::{describe_upstream_error, send, upstream_path};
use crate::gateway::resolve::endpoint_route;
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

/// 对提供商的协议端点发起一次最小消息请求，逐「模型 × 端点」返回结果。
/// `model_id` 缺省取首个启用模型；`protocol` 缺省测全部端点，给出值时只测该协议。
/// **不写日志、不计费、不触发冷却**——它是用户主动的一次性诊断，绝不能污染账本
/// 或干扰真实降级链。
pub async fn probe(
    state: Arc<AppState>,
    provider_id: String,
    model_id: Option<String>,
    protocol: Option<String>,
) -> Result<Vec<ProbeResult>, AppError> {
    let (provider, model) = with_db(&state.db, move |conn| {
        let provider = get_provider(conn, &provider_id)?
            .ok_or_else(|| AppError::message("上游提供商不存在"))?;
        let model = match model_id.filter(|value| !value.is_empty()) {
            Some(model_id) => enabled_model_by_model_id(conn, &provider_id, &model_id)?
                .ok_or_else(|| {
                    AppError::message(format!(
                        "「{}」下没有启用的模型「{model_id}」",
                        provider.provider.name
                    ))
                })?,
            None => first_enabled_model(conn, &provider_id)?.ok_or_else(|| {
                AppError::message(format!(
                    "「{}」名下没有启用的模型，无法探测",
                    provider.provider.name
                ))
            })?,
        };
        Ok((provider, model))
    })
    .await?;

    let all_endpoints: Vec<ProviderEndpoint> = provider.endpoints.clone();
    if all_endpoints.is_empty() {
        return Err(AppError::message(format!(
            "「{}」没有协议端点，无法探测",
            provider.provider.name
        )));
    }
    let protocol = protocol.filter(|value| !value.is_empty());
    let endpoints: Vec<&ProviderEndpoint> = match protocol.as_deref() {
        Some(protocol) => all_endpoints
            .iter()
            .filter(|endpoint| endpoint.protocol == protocol)
            .collect(),
        None => all_endpoints.iter().collect(),
    };
    if endpoints.is_empty() {
        return Err(AppError::message(format!(
            "「{}」没有 {} 协议的端点",
            provider.provider.name,
            protocol.as_deref().unwrap_or_default()
        )));
    }

    let mut results = Vec::with_capacity(endpoints.len());
    for endpoint in endpoints {
        let path = upstream_path(&endpoint.protocol, &model.model_id, false);
        let body = minimal_body(&endpoint.protocol, &model.model_id);
        let route = endpoint_route(&provider.provider, endpoint);
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
                    Some(describe_upstream_error(status, &text))
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
    use axum::http::StatusCode;

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
        let message = describe_upstream_error(StatusCode::TOO_MANY_REQUESTS, text);
        assert!(message.contains("insufficient quota"));
        assert!(message.contains("429"));
    }
}
