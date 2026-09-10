use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use serde_json::Value;

use crate::db::models::{PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI};
use crate::error::AppError;
use crate::gateway::resolve::ResolvedRoute;
use crate::gateway::usage::{
    build_log, extract_fields, finalize, record, LogContext, UsageFields, UsageSource, UsageTotals,
};
use crate::state::AppState;

pub fn upstream_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// OpenAI 协议：输入总量已包含缓存命中，计费时需先扣除。
pub fn contains_cache_read(protocol: &str) -> bool {
    protocol != PROTOCOL_ANTHROPIC
}

/// 向上游发起请求：按 provider 注入自定义头与鉴权，用提供商密钥替换客户端鉴权。
pub async fn send(
    state: &AppState,
    route: &ResolvedRoute,
    body: &Value,
    upstream_path: &str,
    timeout: Option<Duration>,
) -> Result<reqwest::Response, AppError> {
    let url = upstream_url(&route.base_url, upstream_path);
    let mut request = state.http.post(url).json(body);
    if let Some(timeout) = timeout {
        request = request.timeout(timeout);
    }

    for (name, value) in &route.extra_headers {
        request = request.header(name.as_str(), value.as_str());
    }

    if !route.api_key.is_empty() {
        request = match route.auth_scheme.as_str() {
            "x-api-key" => request.header("x-api-key", &route.api_key),
            _ => request.bearer_auth(&route.api_key),
        };
    }

    if route.protocol == PROTOCOL_ANTHROPIC
        && !route.extra_headers.contains_key("anthropic-version")
    {
        request = request.header("anthropic-version", "2023-06-01");
    }
    Ok(request.send().await?)
}

/// 让 OpenAI 兼容上游在流末块返回完整 usage；客户端已显式设置时尊重其选择。
pub fn ensure_include_usage(body: &mut Value, protocol: &str, is_stream: bool) {
    if !is_stream || protocol != PROTOCOL_OPENAI {
        return;
    }
    let already_set = body
        .get("stream_options")
        .and_then(Value::as_object)
        .map(|options| options.contains_key("include_usage"))
        .unwrap_or(false);
    if !already_set {
        body["stream_options"]["include_usage"] = Value::Bool(true);
    }
}

pub fn request_id(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("x-request-id")
        .or_else(|| headers.get("request-id"))
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// 扫描 SSE 流，按字段语义合并用量，并判断流是否正常收尾。
#[derive(Default)]
struct UsageScanner {
    buffer: Vec<u8>,
    fields: UsageFields,
    usage_seen: bool,
    finalized: bool,
}

impl UsageScanner {
    fn push(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
        while let Some(position) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=position).collect();
            self.scan_line(&line);
        }
        if self.buffer.len() > 1_000_000 {
            self.buffer.clear();
        }
    }

    fn scan_line(&mut self, line: &[u8]) {
        let text = String::from_utf8_lossy(line);
        let Some(data) = text.trim().strip_prefix("data:") else {
            return;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return;
        };

        // Anthropic 收尾事件；OpenAI 的最终 usage 块 choices 为空且带 usage。
        if value.get("type").and_then(Value::as_str) == Some("message_stop") {
            self.finalized = true;
        }
        if let Some(choices) = value.get("choices").and_then(Value::as_array) {
            let has_usage = value
                .get("usage")
                .map(|usage| !usage.is_null())
                .unwrap_or(false);
            if choices.is_empty() && has_usage {
                self.finalized = true;
            }
        }

        let usage = value
            .get("usage")
            .filter(|usage| !usage.is_null())
            .or_else(|| {
                value
                    .get("message")
                    .and_then(|message| message.get("usage"))
                    .filter(|usage| !usage.is_null())
            });
        if let Some(usage) = usage {
            let fields = extract_fields(usage);
            if !fields.is_empty() {
                self.fields = self.fields.merge(fields);
                self.usage_seen = true;
            }
        }
    }

    fn totals(&self, contains_cache_read: bool) -> UsageTotals {
        if !self.usage_seen {
            return UsageTotals::missing();
        }
        match finalize(self.fields, contains_cache_read) {
            Some(mut totals) => {
                if !self.finalized {
                    totals.source = UsageSource::Partial;
                }
                totals
            }
            None => UsageTotals::missing(),
        }
    }
}

/// 透传上游 SSE 流，同时在后台扫描末块 usage 并落库。
pub fn stream_response(
    state: Arc<AppState>,
    route: ResolvedRoute,
    alias: String,
    endpoint: String,
    response: reqwest::Response,
) -> Response {
    let status = response.status();
    let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
    let request_id = request_id(response.headers());
    let cache_in_input = contains_cache_read(&route.protocol);
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(16);

    tokio::spawn(async move {
        let started = Instant::now();
        let mut stream = response.bytes_stream();
        let mut scanner = UsageScanner::default();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    scanner.push(&bytes);
                    if tx.send(Ok(bytes)).await.is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = tx.send(Err(std::io::Error::other(error.to_string()))).await;
                    break;
                }
            }
        }
        drop(tx);

        let log = build_log(LogContext {
            endpoint,
            method: "POST".to_string(),
            alias,
            kind: "chat".to_string(),
            is_stream: true,
            route: Some(route),
            latency_ms: started.elapsed().as_millis() as i64,
            status: if status.is_success() {
                "success".to_string()
            } else {
                "error".to_string()
            },
            http_status: Some(status.as_u16() as i64),
            error_message: None,
            request_id,
            usage: scanner.totals(cache_in_input),
        });
        let _ = record(&state, log).await;
    });

    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    });

    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = status;
    if let Some(content_type) = content_type {
        response.headers_mut().insert(header::CONTENT_TYPE, content_type);
    }
    response
}

pub fn error_response(status: StatusCode, message: &str) -> Response {
    let body = serde_json::json!({
        "error": { "message": message, "type": "lumen_error" }
    });
    (status, axum::Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn injects_include_usage_for_openai_stream() {
        let mut body = json!({ "model": "x", "stream": true });
        ensure_include_usage(&mut body, PROTOCOL_OPENAI, true);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn respects_explicit_include_usage_flag() {
        let mut body = json!({ "stream": true, "stream_options": { "include_usage": false } });
        ensure_include_usage(&mut body, PROTOCOL_OPENAI, true);
        assert_eq!(body["stream_options"]["include_usage"], false);
    }

    #[test]
    fn skips_injection_for_non_stream_or_anthropic() {
        let mut non_stream = json!({ "stream": false });
        ensure_include_usage(&mut non_stream, PROTOCOL_OPENAI, false);
        assert!(non_stream.get("stream_options").is_none());

        let mut anthropic = json!({ "stream": true });
        ensure_include_usage(&mut anthropic, PROTOCOL_ANTHROPIC, true);
        assert!(anthropic.get("stream_options").is_none());
    }

    #[test]
    fn scanner_finalizes_openai_usage_chunk() {
        let mut scanner = UsageScanner::default();
        scanner.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n");
        scanner.push(
            b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":20,\"total_tokens\":120}}\n\n",
        );
        scanner.push(b"data: [DONE]\n\n");
        let totals = scanner.totals(true);
        assert_eq!(totals.source, UsageSource::Provider);
        assert_eq!(totals.input_tokens, 100);
        assert_eq!(totals.output_tokens, 20);
    }

    #[test]
    fn scanner_merges_anthropic_events_and_flags_aborted_stream() {
        let mut scanner = UsageScanner::default();
        scanner.push(
            b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":25,\"output_tokens\":1,\"cache_read_input_tokens\":10}}}\n\n",
        );
        scanner.push(
            b"event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":15}}\n\n",
        );
        scanner.push(b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
        let totals = scanner.totals(false);
        assert_eq!(totals.source, UsageSource::Provider);
        assert_eq!(totals.input_tokens, 25);
        assert_eq!(totals.output_tokens, 15);
        assert_eq!(totals.cache_read_tokens, 10);

        // 缺少 message_stop：视为未收尾。
        let mut aborted = UsageScanner::default();
        aborted.push(
            b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":15}}\n\n",
        );
        assert_eq!(aborted.totals(false).source, UsageSource::Partial);
    }
}

