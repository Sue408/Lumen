use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use llmwire::Termination;
use serde_json::Value;
use tokio::time::timeout;

use crate::db::models::{
    contains_cache_read, PROTOCOL_ANTHROPIC, PROTOCOL_GEMINI, PROTOCOL_OPENAI, PROTOCOL_RESPONSES,
};
use crate::error::AppError;
use crate::gateway::convert::Conversion;
use crate::gateway::estimate::{collect_stream_delta, count_tokens, Estimates};
use crate::gateway::headers::build_upstream_headers;
use crate::gateway::resolve::ResolvedRoute;
use crate::gateway::usage::{
    apply_estimates, build_log, extract_fields, finalize_with_estimate, record, LogContext,
    UsageFields, UsageSource, UsageTotals,
};
use crate::state::AppState;

/// 流式上游两次数据之间的最大静默时长；超过即视为挂起，主动收尾并记为失败。
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

pub fn upstream_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// 把上游的非 2xx 响应整理成人话：优先取 `error.message`，否则截断原始文本。
/// 探测与拉取模型列表共用。
pub fn describe_upstream_error(status: StatusCode, text: &str) -> String {
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

/// 由上游协议与模型名决定转发路径；端点与协议绑定后，这是路径的唯一来源。
/// Gemini 的模型名在 path 而非 body，流式端点强制 `alt=sse` 以取得标准 SSE。
pub fn upstream_path(protocol: &str, model_id: &str, is_stream: bool) -> String {
    match protocol {
        PROTOCOL_ANTHROPIC => "messages".to_string(),
        PROTOCOL_RESPONSES => "responses".to_string(),
        PROTOCOL_GEMINI => {
            if is_stream {
                format!("models/{model_id}:streamGenerateContent?alt=sse")
            } else {
                format!("models/{model_id}:generateContent")
            }
        }
        _ => "chat/completions".to_string(),
    }
}

/// 把 provider 的四意图（内置底座 / 透传 / 替换 / 添加 / 移除）与鉴权注入请求：
/// 鉴权最后注入、无条件覆盖，硬黑名单在求值阶段剥离。转发与只读探测共用此装饰。
fn decorate(
    mut request: reqwest::RequestBuilder,
    route: &ResolvedRoute,
    client_headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    let mapped = build_upstream_headers(client_headers, &route.header_rules, &route.extra_headers);
    for (name, value) in mapped.iter() {
        request = request.header(name, value);
    }

    // 鉴权最后注入、无条件覆盖。
    if !route.api_key.is_empty() {
        request = match route.auth_scheme.as_str() {
            "x-api-key" => request.header("x-api-key", &route.api_key),
            "x-goog-api-key" => request.header("x-goog-api-key", &route.api_key),
            _ => request.bearer_auth(&route.api_key),
        };
    }

    // Anthropic 默认版本头：仅当上游头（规则结果或 provider 基线）里缺失时补。
    if route.upstream_protocol == PROTOCOL_ANTHROPIC && !mapped.contains_key("anthropic-version") {
        request = request.header("anthropic-version", "2023-06-01");
    }
    request
}

/// 向上游发起请求：按 provider 的四意图构建上游头（见 `decorate`）。
pub async fn send(
    state: &AppState,
    route: &ResolvedRoute,
    body: &Value,
    upstream_path: &str,
    timeout: Option<Duration>,
    client_headers: &HeaderMap,
) -> Result<reqwest::Response, AppError> {
    let url = upstream_url(&route.base_url, upstream_path);
    let mut request = state.http().post(url).json(body);
    if let Some(timeout) = timeout {
        request = request.timeout(timeout);
    }
    Ok(decorate(request, route, client_headers).send().await?)
}

/// 向上游发起 GET（无 body）：供拉取模型列表等只读探测复用同一套头与鉴权。
pub async fn fetch(
    state: &AppState,
    route: &ResolvedRoute,
    upstream_path: &str,
    timeout: Option<Duration>,
) -> Result<reqwest::Response, AppError> {
    let url = upstream_url(&route.base_url, upstream_path);
    let mut request = state.http().get(url);
    if let Some(timeout) = timeout {
        request = request.timeout(timeout);
    }
    Ok(decorate(request, route, &HeaderMap::new()).send().await?)
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
    /// 上游协议，决定容器、缓存边界与收尾标记。
    protocol: String,
    /// 真实上游模型名，用于在上游漏报时选取 tokenizer。
    model_id: String,
    /// 累积的输出文本，供上游未给 usage 时估算。
    output_text: String,
    /// 请求侧估算的输入 token；上游未回输入时用它补齐。
    input_estimate: Option<i64>,
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
        if data.is_empty() {
            return;
        }
        // `[DONE]` 是 SSE 的通用终止符：视为正常收尾，避免把「用量挂在非空
        // choices 块上、随后以 [DONE] 结束」的上游误判成 partial。
        if data == "[DONE]" {
            self.finalized = true;
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return;
        };

        // 无论上游是否上报 usage，都累积输出文本，供漏报时估算。
        collect_stream_delta(&self.protocol, &value, &mut self.output_text);

        // Anthropic 以 message_stop 收尾；OpenAI 的最终 usage 块 choices 为空且带 usage；
        // Responses 以 response.completed / incomplete / failed 收尾。
        let event_type = value.get("type").and_then(Value::as_str);
        if event_type == Some("message_stop") {
            self.finalized = true;
        }
        if matches!(
            event_type,
            Some("response.completed" | "response.incomplete" | "response.failed")
        ) {
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
        // Gemini SSE 没有显式终止符：以「带 usageMetadata 且候选给出 finishReason」为收尾。
        if self.protocol == PROTOCOL_GEMINI
            && value.get("usageMetadata").is_some_and(|usage| !usage.is_null())
            && value
                .pointer("/candidates/0/finishReason")
                .and_then(Value::as_str)
                .is_some()
        {
            self.finalized = true;
        }

        let usage = value
            .get("usage")
            .filter(|usage| !usage.is_null())
            .or_else(|| {
                value
                    .get("message")
                    .and_then(|message| message.get("usage"))
                    .filter(|usage| !usage.is_null())
            })
            .or_else(|| {
                value
                    .get("response")
                    .and_then(|response| response.get("usage"))
                    .filter(|usage| !usage.is_null())
            })
            .or_else(|| value.get("usageMetadata").filter(|usage| !usage.is_null()));
        if let Some(usage) = usage {
            let fields = extract_fields(usage);
            if !fields.is_empty() {
                self.fields = self.fields.merge(fields);
                self.usage_seen = true;
            }
        }
    }

    fn totals(&self) -> UsageTotals {
        let estimates = Estimates {
            input: self.input_estimate,
            output: if self.output_text.is_empty() {
                None
            } else {
                Some(count_tokens(&self.model_id, &self.output_text))
            },
        };
        let mut fields = self.fields;
        let used = apply_estimates(&mut fields, estimates);
        // 上游一字未报、且估算也拿不到文本（如完全失败）时，只能记 missing。
        if !self.usage_seen && !used {
            return UsageTotals::missing();
        }
        match finalize_with_estimate(fields, contains_cache_read(&self.protocol), used) {
            Some(mut totals) => {
                // 上游给了完整字段但流未正常收尾 → Partial；用了估算则已是 Estimated。
                if totals.source == UsageSource::Provider && !self.finalized {
                    totals.source = UsageSource::Partial;
                }
                totals
            }
            None => UsageTotals::missing(),
        }
    }
}

/// 流式收尾写日志所需的请求身份。打包传入，避免 `stream_response` 参数持续膨胀。
pub struct StreamMeta {
    pub alias: String,
    pub endpoint: String,
    pub virtual_key_id: Option<String>,
    pub attempt_index: i64,
    pub session_id: Option<String>,
    /// 请求侧估算的输入 token，供上游漏报时补齐。
    pub input_estimate: Option<i64>,
}

/// 透传上游 SSE 流，同时在后台扫描末块 usage 并落库。
///
/// 跨协议转换时 `conversion` 为 `Some`：上游字节仍按**上游协议**喂给用量扫描器，
/// 且逐块转换后转发给客户端；流末调用 `finish` 冲刷尾部并据终止原因判定成败。
pub fn stream_response(
    state: Arc<AppState>,
    route: ResolvedRoute,
    meta: StreamMeta,
    response: reqwest::Response,
    mut conversion: Option<Conversion>,
) -> Response {
    let status = response.status();
    let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
    let request_id = request_id(response.headers());
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(16);

    tokio::spawn(async move {
        let started = Instant::now();
        let mut stream = response.bytes_stream();
        let mut scanner = UsageScanner {
            protocol: route.upstream_protocol.clone(),
            model_id: route.model_id.clone(),
            input_estimate: meta.input_estimate,
            ..UsageScanner::default()
        };
        // 首个数据块到达的时刻：`latency_ms − ttfb_ms` 即纯生成时长。
        let mut ttfb: Option<Duration> = None;
        // 上游长时间不吐字节即视为挂起：主动中止并把该次调用记为失败，
        // 避免连接与扫描任务被永久占住。
        let mut failure: Option<String> = None;
        loop {
            let item = match timeout(STREAM_IDLE_TIMEOUT, stream.next()).await {
                Ok(item) => item,
                Err(_) => {
                    failure = Some("上游流式响应超时（长时间无数据）".to_string());
                    let _ = tx
                        .send(Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "上游流式响应超时",
                        )))
                        .await;
                    break;
                }
            };
            let Some(chunk) = item else { break };
            match chunk {
                Ok(bytes) => {
                    if ttfb.is_none() && !bytes.is_empty() {
                        ttfb = Some(started.elapsed());
                    }
                    // 用量始终按上游协议扫描，与是否跨协议转换无关。
                    scanner.push(&bytes);
                    let payload = match conversion.as_mut() {
                        Some(conversion) => match conversion.feed(&bytes) {
                            Ok(output) if output.is_empty() => continue,
                            Ok(output) => Bytes::from(output),
                            Err(error) => {
                                failure = Some(error.to_string());
                                break;
                            }
                        },
                        None => bytes,
                    };
                    if tx.send(Ok(payload)).await.is_err() {
                        // 下游（客户端）提前断开：既不能记为成功，也没必要继续读上游。
                        failure = Some("客户端中断连接，响应未完整送达".to_string());
                        break;
                    }
                }
                Err(error) => {
                    failure = Some(error.to_string());
                    let _ = tx.send(Err(std::io::Error::other(error.to_string()))).await;
                    break;
                }
            }
        }

        // 正常收流时冲刷分帧尾部，并把终止原因映射为成败。
        if failure.is_none() {
            if let Some(conversion) = conversion.as_mut() {
                match conversion.finish() {
                    Ok((tail, ended)) => {
                        if !tail.is_empty() {
                            let _ = tx.send(Ok(Bytes::from(tail))).await;
                        }
                        if !matches!(ended, Termination::Explicit | Termination::CleanClose) {
                            failure = Some(format!("上游流式响应未正常结束：{ended:?}"));
                        }
                    }
                    Err(error) => failure = Some(error.to_string()),
                }
            }
        }
        drop(tx);

        let mut log = build_log(LogContext {
            endpoint: meta.endpoint,
            alias: meta.alias,
            is_stream: true,
            route: Some(route),
            latency_ms: started.elapsed().as_millis() as i64,
            status: if failure.is_none() && status.is_success() {
                "success".to_string()
            } else {
                "error".to_string()
            },
            http_status: Some(status.as_u16() as i64),
            error_message: failure,
            request_id,
            virtual_key_id: meta.virtual_key_id,
            usage: scanner.totals(),
            attempt_index: meta.attempt_index,
            session_id: meta.session_id,
        });
        log.ttfb_ms = ttfb.map(|elapsed| elapsed.as_millis() as i64);
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

    fn make_scanner(protocol: &str) -> UsageScanner {
        UsageScanner {
            protocol: protocol.to_string(),
            ..UsageScanner::default()
        }
    }

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
    fn upstream_path_covers_new_protocols() {
        assert_eq!(upstream_path(PROTOCOL_OPENAI, "gpt", false), "chat/completions");
        assert_eq!(upstream_path(PROTOCOL_ANTHROPIC, "claude", false), "messages");
        assert_eq!(upstream_path(PROTOCOL_RESPONSES, "gpt", false), "responses");
        assert_eq!(
            upstream_path(PROTOCOL_GEMINI, "gemini-2.5-pro", false),
            "models/gemini-2.5-pro:generateContent"
        );
        assert_eq!(
            upstream_path(PROTOCOL_GEMINI, "gemini-2.5-pro", true),
            "models/gemini-2.5-pro:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn scanner_finalizes_openai_usage_chunk() {
        let mut scanner = make_scanner(PROTOCOL_OPENAI);
        scanner.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n");
        scanner.push(
            b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":20,\"total_tokens\":120}}\n\n",
        );
        scanner.push(b"data: [DONE]\n\n");
        let totals = scanner.totals();
        assert_eq!(totals.source, UsageSource::Provider);
        assert_eq!(totals.input_tokens, 100);
        assert_eq!(totals.output_tokens, 20);
    }

    #[test]
    fn scanner_finalizes_openai_stream_on_done() {
        let mut scanner = make_scanner(PROTOCOL_OPENAI);
        // usage 挂在最后一个非空 choices 块上，随后以 [DONE] 结束。
        scanner.push(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}],\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":20}}\n\n",
        );
        scanner.push(b"data: [DONE]\n\n");
        let totals = scanner.totals();
        assert_eq!(totals.source, UsageSource::Provider);
        assert_eq!(totals.input_tokens, 100);
        assert_eq!(totals.output_tokens, 20);
    }

    #[test]
    fn scanner_merges_anthropic_events_and_flags_aborted_stream() {
        let mut scanner = make_scanner(PROTOCOL_ANTHROPIC);
        scanner.push(
            b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":25,\"output_tokens\":1,\"cache_read_input_tokens\":10}}}\n\n",
        );
        scanner.push(
            b"event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":15}}\n\n",
        );
        scanner.push(b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
        let totals = scanner.totals();
        assert_eq!(totals.source, UsageSource::Provider);
        assert_eq!(totals.input_tokens, 25);
        assert_eq!(totals.output_tokens, 15);
        assert_eq!(totals.cache_read_tokens, 10);

        // 缺少 message_stop：视为未收尾。
        let mut aborted = make_scanner(PROTOCOL_ANTHROPIC);
        aborted.push(
            b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":15}}\n\n",
        );
        assert_eq!(aborted.totals().source, UsageSource::Partial);
    }

    #[test]
    fn scanner_reads_responses_completed_usage() {
        let mut scanner = make_scanner(PROTOCOL_RESPONSES);
        scanner.push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n");
        scanner.push(
            b"data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":75,\"input_tokens_details\":{\"cached_tokens\":20},\"output_tokens\":30,\"total_tokens\":105}}}\n\n",
        );
        let totals = scanner.totals();
        assert_eq!(totals.source, UsageSource::Provider);
        assert_eq!(totals.input_tokens, 75);
        assert_eq!(totals.output_tokens, 30);
        assert_eq!(totals.cache_read_tokens, 20);
    }

    #[test]
    fn scanner_estimates_output_when_usage_never_arrives() {
        let mut scanner = make_scanner(PROTOCOL_RESPONSES);
        scanner.push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n");
        let totals = scanner.totals();
        // 上游未给 usage，但响应文本在手：估算而不是放弃。
        assert_eq!(totals.source, UsageSource::Estimated);
        assert!(totals.output_tokens > 0);
    }

    #[test]
    fn scanner_stays_missing_without_usage_or_text() {
        let mut scanner = make_scanner(PROTOCOL_OPENAI);
        scanner.push(b"data: {\"choices\":[]}\n\n");
        assert_eq!(scanner.totals().source, UsageSource::Missing);
    }

    #[test]
    fn scanner_reads_gemini_sse_usage() {
        let mut scanner = make_scanner(PROTOCOL_GEMINI);
        scanner.push(
            b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hi\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":100,\"cachedContentTokenCount\":60,\"candidatesTokenCount\":20,\"thoughtsTokenCount\":8,\"totalTokenCount\":133}}\n\n",
        );
        let totals = scanner.totals();
        assert_eq!(totals.source, UsageSource::Provider);
        assert_eq!(totals.input_tokens, 100);
        assert_eq!(totals.output_tokens, 20);
        assert_eq!(totals.cache_read_tokens, 60);
        assert_eq!(totals.reasoning_tokens, 8);
    }

    #[test]
    fn scanner_flags_gemini_stream_without_finish_reason() {
        let mut scanner = make_scanner(PROTOCOL_GEMINI);
        // 有用量但未给出 finishReason：不视为收尾 → Partial。
        scanner.push(
            b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hi\"}]}}],\"usageMetadata\":{\"promptTokenCount\":100,\"candidatesTokenCount\":20}}\n\n",
        );
        assert_eq!(scanner.totals().source, UsageSource::Partial);
    }
}

