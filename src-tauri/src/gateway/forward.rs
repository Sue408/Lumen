use std::sync::Arc;
use std::time::Instant;

use axum::body::{Body, Bytes};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use serde_json::Value;

use crate::db::models::PROTOCOL_ANTHROPIC;
use crate::error::AppError;
use crate::gateway::resolve::ResolvedRoute;
use crate::gateway::usage::{build_log, extract_usage, record, LogContext, UsageTotals};
use crate::state::AppState;

pub fn upstream_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// 向上游发起请求：按 provider 注入自定义头与鉴权，用提供商密钥替换客户端鉴权。
pub async fn send(
    state: &AppState,
    route: &ResolvedRoute,
    body: &Value,
    upstream_path: &str,
) -> Result<reqwest::Response, AppError> {
    let url = upstream_url(&route.base_url, upstream_path);
    let mut request = state.http.post(url).json(body);

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

#[derive(Default)]
struct UsageScanner {
    buffer: Vec<u8>,
    usage: Option<UsageTotals>,
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
        if let Ok(value) = serde_json::from_str::<Value>(data) {
            // OpenAI 用顶层 usage；Anthropic 的 message_start 把 usage 嵌在 message 下。
            let usage = extract_usage(&value)
                .or_else(|| value.get("message").and_then(extract_usage));
            if let Some(usage) = usage {
                self.usage = Some(match self.usage {
                    Some(previous) => previous.merged(&usage),
                    None => usage,
                });
            }
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
            usage: scanner.usage,
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
