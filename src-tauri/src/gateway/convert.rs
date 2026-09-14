//! 跨协议转换：当入站协议与上游端点协议不一致时，把请求与响应交给 `llmwire`。
//!
//! 这里是 `llmwire` 在 Lumen 内的**唯一适配点**——协议标识映射、能力策略装配、
//! 请求 / 响应 / 流式转换与报告收集都在此收口。同协议路径不经过本模块，继续字节透传。

use llmwire::{converter, resolve, Converter, ProtocolId, Termination};
use serde_json::Value;

use crate::db::models::{PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI, PROTOCOL_RESPONSES};
use crate::error::AppError;

/// Lumen 协议标识映射到 `llmwire` 协议；Gemini 不在转换核内，返回 `None`。
pub fn protocol_id(protocol: &str) -> Option<ProtocolId> {
    match protocol {
        PROTOCOL_OPENAI => Some(ProtocolId::Chat),
        PROTOCOL_ANTHROPIC => Some(ProtocolId::Messages),
        PROTOCOL_RESPONSES => Some(ProtocolId::Responses),
        _ => None,
    }
}

/// 该协议能否参与跨协议转换（目前为除 Gemini 外的全部协议）。
pub fn is_convertible(protocol: &str) -> bool {
    protocol_id(protocol).is_some()
}

/// 本次请求是否需要转换：两端都可转换且协议不同。同协议与含 Gemini 的组合都走透传。
pub fn needs_conversion(inbound: &str, upstream: &str) -> bool {
    inbound != upstream && is_convertible(inbound) && is_convertible(upstream)
}

/// 请求用到了转换核无法安全表达的语义，且**需要跨协议转换**时给出显式原因。
///
/// 这些特性依赖上游服务端状态或 host 侧预解析，跨协议无法保真；调用方应显式拒绝
/// （或改用同协议端点），绝不能静默降级成错误语义。
pub fn unsupported_request(body: &Value, is_stream: bool, inbound: &str) -> Option<String> {
    if inbound == PROTOCOL_RESPONSES {
        if body.get("store").and_then(Value::as_bool) == Some(true) {
            return Some("responses 的 store=true 依赖上游服务端状态，无法跨协议转换".to_string());
        }
        if body.get("previous_response_id").is_some() {
            return Some(
                "responses 的 previous_response_id 依赖上游服务端状态，无法跨协议转换".to_string(),
            );
        }
    }
    if inbound == PROTOCOL_OPENAI
        && is_stream
        && body.get("n").and_then(Value::as_u64).is_some_and(|n| n > 1)
    {
        return Some("chat 流式 n>1 无法跨协议转换".to_string());
    }
    if contains_file_id(body) {
        return Some("图片 file_id 需要 host 预解析，无法跨协议转换".to_string());
    }
    None
}

/// 递归查找 `file_id` 字段（图片引用，需 host 预解析成 URL / Base64）。
fn contains_file_id(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            (key == "file_id" && value.is_string()) || contains_file_id(value)
        }),
        Value::Array(items) => items.iter().any(contains_file_id),
        _ => false,
    }
}

/// 一次请求的转换器，封装 `llmwire::Converter` 与两端协议，隔离 SDK 公开接口。
pub struct Conversion {
    converter: Box<dyn Converter>,
    inbound: String,
    upstream: String,
}

impl Conversion {
    /// 以入站协议为源、上游端点协议为目标构造转换器。
    ///
    /// 能力策略 MVP 走 `llmwire::resolve`（协议默认：目标 Chat 时剥离 thinking，目标
    /// Messages / Responses 时透传；缓存控制与 beta 仅透传给 Messages）。需要按模型覆盖
    /// （thinking / max_output_tokens / supported 参数）时，在此改用 `StaticHost` /
    /// `ModelProfile`，把 Lumen 的模型表映射成 `Capabilities`——这是唯一的装配点。
    pub fn new(inbound: &str, upstream: &str, model: &str) -> Result<Self, AppError> {
        let source = protocol_id(inbound).ok_or_else(|| unconvertible(inbound))?;
        let target = protocol_id(upstream).ok_or_else(|| unconvertible(upstream))?;
        let caps = resolve(source, target, model);
        let converter = converter(source, target, caps).map_err(conversion_error)?;
        Ok(Self {
            converter,
            inbound: inbound.to_string(),
            upstream: upstream.to_string(),
        })
    }

    /// 入站协议（客户端使用的协议）。
    pub fn inbound(&self) -> &str {
        &self.inbound
    }

    /// 上游协议（实际转发使用的协议）。
    pub fn upstream(&self) -> &str {
        &self.upstream
    }

    /// 客户端请求体 → 上游协议请求体。
    pub fn request(&mut self, body: &[u8]) -> Result<Vec<u8>, AppError> {
        let mut out = Vec::new();
        self.converter
            .request(body, &mut out)
            .map_err(conversion_error)?;
        Ok(out)
    }

    /// 上游非流式响应体 → 客户端协议响应体。
    pub fn response(&mut self, body: &[u8]) -> Result<Vec<u8>, AppError> {
        let mut out = Vec::new();
        self.converter
            .response(body, &mut out)
            .map_err(conversion_error)?;
        Ok(out)
    }

    /// 消费一段上游流式字节，返回可转发给客户端的事件字节（可能为空）。
    pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<u8>, AppError> {
        let mut out = Vec::new();
        self.converter.feed(chunk, &mut out).map_err(conversion_error)?;
        Ok(out)
    }

    /// 冲刷分帧尾部，返回待转发字节与终止原因。
    pub fn finish(&mut self) -> Result<(Vec<u8>, Termination), AppError> {
        let mut out = Vec::new();
        let termination = self.converter.finish(&mut out).map_err(conversion_error)?;
        Ok((out, termination))
    }

    /// 取走本次转换累积的质量报告并落到 tracing，返回是否存在 `Fatal` 条目。
    ///
    /// 报告不是记账真源，但降级 / 无法映射必须留痕，不能静默吞掉。
    pub fn log_report(&mut self, context: &str) -> bool {
        let report = self.converter.take_report();
        if report.is_empty() {
            return false;
        }
        for entry in &report.unmapped {
            tracing::warn!(
                inbound = self.inbound(),
                upstream = self.upstream(),
                field = %entry.field,
                reason = ?entry.reason,
                severity = ?entry.severity,
                "{context}：字段无法映射"
            );
        }
        for warning in &report.warnings {
            tracing::warn!(
                inbound = self.inbound(),
                upstream = self.upstream(),
                field = %warning.field,
                severity = ?warning.severity,
                "{context}：{}",
                warning.message
            );
        }
        report.has_fatal()
    }
}

fn conversion_error(error: llmwire::Error) -> AppError {
    AppError::message(format!("协议转换失败：{error}"))
}

fn unconvertible(protocol: &str) -> AppError {
    AppError::message(format!("协议「{protocol}」不支持跨协议转换"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    /// 一个最小但结构完整的客户端请求（`model` 用 `m`，便于断言转换是否原样保留）。
    fn request_body(protocol: &str) -> Value {
        match protocol {
            PROTOCOL_OPENAI => json!({
                "model": "m",
                "max_completion_tokens": 16,
                "messages": [{ "role": "user", "content": "hi" }]
            }),
            PROTOCOL_ANTHROPIC => json!({
                "model": "m",
                "max_tokens": 16,
                "messages": [{ "role": "user", "content": "hi" }]
            }),
            PROTOCOL_RESPONSES => json!({ "model": "m", "input": "hi" }),
            other => panic!("unhandled protocol {other}"),
        }
    }

    /// 上游在 `protocol` 协议下返回的文本响应（取自 llmwire 的协议矩阵夹具）。
    fn response_body(protocol: &str) -> &'static str {
        match protocol {
            PROTOCOL_OPENAI => r#"{"id":"chatcmpl-matrix","choices":[{"index":0,"message":{"role":"assistant","content":"world"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}"#,
            PROTOCOL_ANTHROPIC => r#"{"id":"msg_matrix","type":"message","role":"assistant","content":[{"type":"text","text":"world"}],"model":"up","stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":2}}"#,
            PROTOCOL_RESPONSES => r#"{"id":"resp_matrix","object":"response","status":"completed","model":"up","output":[{"type":"message","id":"msg_matrix","role":"assistant","content":[{"type":"output_text","text":"world"}]}],"usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5}}"#,
            other => panic!("unhandled protocol {other}"),
        }
    }

    /// 上游在 `protocol` 协议下返回的文本 SSE 流。
    fn stream_body(protocol: &str) -> &'static str {
        match protocol {
            PROTOCOL_OPENAI => concat!(
                "data: {\"id\":\"chatcmpl-stream\",\"model\":\"up\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"world\"},\"finish_reason\":null}]}\n\n",
                "data: [DONE]\n\n",
            ),
            PROTOCOL_ANTHROPIC => concat!(
                "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_stream\",\"model\":\"up\"}}\n\n",
                "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"world\"}}\n\n",
                "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
                "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
            ),
            PROTOCOL_RESPONSES => concat!(
                "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_stream\",\"model\":\"up\"}}\n\n",
                "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_stream\"}}\n\n",
                "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"delta\":\"world\"}\n\n",
                "event: response.output_text.done\ndata: {\"type\":\"response.output_text.done\",\"output_index\":0,\"content_index\":0,\"text\":\"world\"}\n\n",
                "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_stream\",\"status\":\"completed\",\"model\":\"up\",\"output\":[]}}\n\n",
            ),
            other => panic!("unhandled protocol {other}"),
        }
    }

    /// 从 `protocol` 协议的文本响应里取出助手文本。
    fn assistant_text(protocol: &str, value: &Value) -> String {
        match protocol {
            PROTOCOL_OPENAI => value["choices"][0]["message"]["content"]
                .as_str()
                .unwrap()
                .to_string(),
            PROTOCOL_ANTHROPIC => value["content"][0]["text"].as_str().unwrap().to_string(),
            PROTOCOL_RESPONSES => value["output"][0]["content"][0]["text"]
                .as_str()
                .unwrap()
                .to_string(),
            other => panic!("unhandled protocol {other}"),
        }
    }

    #[test]
    fn flags_unsupported_requests_only_when_relevant() {
        let responses = |body: Value| unsupported_request(&body, false, PROTOCOL_RESPONSES);
        assert!(responses(json!({ "store": true })).is_some());
        assert!(responses(json!({ "previous_response_id": "resp_1" })).is_some());
        assert!(unsupported_request(&json!({ "n": 2 }), true, PROTOCOL_OPENAI).is_some());
        assert!(unsupported_request(&json!({ "n": 1 }), true, PROTOCOL_OPENAI).is_none());
        // 非流式下 `n>1` 可转换，不拒绝。
        assert!(unsupported_request(&json!({ "n": 2 }), false, PROTOCOL_OPENAI).is_none());
        assert!(responses(json!({
            "input": [{ "type": "input_image", "file_id": "file-1" }]
        }))
        .is_some());
        // 与入站协议无关的字段不误报。
        assert!(unsupported_request(&json!({ "store": true }), false, PROTOCOL_OPENAI).is_none());
    }

    const PROTOCOLS: [&str; 3] = [PROTOCOL_OPENAI, PROTOCOL_ANTHROPIC, PROTOCOL_RESPONSES];

    #[test]
    fn converts_all_six_directed_pairs() {
        for src in PROTOCOLS {
            for dst in PROTOCOLS {
                if src == dst {
                    continue;
                }
                let mut conversion = Conversion::new(src, dst, "up").unwrap();

                let request = serde_json::to_vec(&request_body(src)).unwrap();
                let encoded = conversion.request(&request).unwrap();
                let upstream: Value = serde_json::from_slice(&encoded)
                    .unwrap_or_else(|error| panic!("{src}->{dst} request: {error}"));
                assert_eq!(upstream["model"], "m", "{src}->{dst} keeps client model");

                let decoded = conversion.response(response_body(dst).as_bytes()).unwrap();
                let client: Value = serde_json::from_slice(&decoded)
                    .unwrap_or_else(|error| panic!("{src}->{dst} response: {error}"));
                assert_eq!(assistant_text(src, &client), "world", "{src}->{dst} text");
            }
        }
    }

    #[test]
    fn streams_all_six_directed_pairs() {
        for src in PROTOCOLS {
            for dst in PROTOCOLS {
                if src == dst {
                    continue;
                }
                let mut conversion = Conversion::new(src, dst, "up").unwrap();
                let mut request = request_body(src);
                request["stream"] = json!(true);
                conversion
                    .request(&serde_json::to_vec(&request).unwrap())
                    .unwrap();

                let mut body =
                    String::from_utf8_lossy(&conversion.feed(stream_body(dst).as_bytes()).unwrap())
                        .into_owned();
                let (tail, ended) = conversion.finish().unwrap();
                body.push_str(&String::from_utf8_lossy(&tail));

                assert!(body.contains("world"), "{src}->{dst} stream: {body}");
                assert!(
                    matches!(ended, Termination::Explicit | Termination::CleanClose),
                    "{src}->{dst} termination: {ended:?}"
                );
            }
        }
    }

    #[test]
    fn maps_lumen_protocols_to_llmwire() {
        assert_eq!(protocol_id(PROTOCOL_OPENAI), Some(ProtocolId::Chat));
        assert_eq!(protocol_id(PROTOCOL_ANTHROPIC), Some(ProtocolId::Messages));
        assert_eq!(protocol_id(PROTOCOL_RESPONSES), Some(ProtocolId::Responses));
        assert_eq!(protocol_id("gemini"), None);
        assert_eq!(protocol_id("unknown"), None);
    }

    #[test]
    fn detects_when_conversion_is_needed() {
        assert!(needs_conversion(PROTOCOL_OPENAI, PROTOCOL_ANTHROPIC));
        assert!(needs_conversion(PROTOCOL_RESPONSES, PROTOCOL_ANTHROPIC));
        assert!(!needs_conversion(PROTOCOL_OPENAI, PROTOCOL_OPENAI));
        assert!(!needs_conversion("gemini", PROTOCOL_OPENAI));
        assert!(!needs_conversion(PROTOCOL_OPENAI, "gemini"));
        assert!(!needs_conversion("gemini", "gemini"));
    }

    #[test]
    fn conversion_rejects_unconvertible_protocols() {
        assert!(Conversion::new("gemini", PROTOCOL_OPENAI, "m").is_err());
        assert!(Conversion::new(PROTOCOL_OPENAI, "gemini", "m").is_err());
    }

    #[test]
    fn chat_request_and_messages_response_round_trip() {
        let mut conversion =
            Conversion::new(PROTOCOL_OPENAI, PROTOCOL_ANTHROPIC, "claude-x").unwrap();

        let request = br#"{
            "model": "claude-x",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 64
        }"#;
        let encoded = conversion.request(request).unwrap();
        let value: Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(value["model"], "claude-x");
        assert_eq!(value["max_tokens"], 64);
        assert_eq!(value["messages"][0]["role"], "user");
        assert_eq!(value["messages"][0]["content"][0]["text"], "hi");

        let response = br#"{
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "claude-upstream",
            "content": [{"type": "text", "text": "hello"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 3, "output_tokens": 2}
        }"#;
        let decoded = conversion.response(response).unwrap();
        let value: Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(value["choices"][0]["message"]["content"], "hello");
        assert_eq!(value["choices"][0]["finish_reason"], "stop");
        assert_eq!(value["model"], "claude-upstream");
        assert_eq!(value["usage"]["prompt_tokens"], 3);
        assert_eq!(value["usage"]["completion_tokens"], 2);
    }

    #[test]
    fn messages_request_and_chat_response_round_trip() {
        let mut conversion =
            Conversion::new(PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI, "gpt-x").unwrap();

        let request = br#"{
            "model": "gpt-x",
            "max_tokens": 64,
            "messages": [{"role": "user", "content": "hi"}]
        }"#;
        let encoded = conversion.request(request).unwrap();
        let value: Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(value["model"], "gpt-x");
        assert_eq!(value["max_completion_tokens"], 64);
        assert_eq!(value["messages"][0]["content"], "hi");

        let response = br#"{
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "model": "gpt-upstream",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "hello"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5}
        }"#;
        let decoded = conversion.response(response).unwrap();
        let value: Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(value["content"][0]["text"], "hello");
        assert_eq!(value["stop_reason"], "end_turn");
        assert_eq!(value["model"], "gpt-upstream");
        assert_eq!(value["usage"]["input_tokens"], 3);
        assert_eq!(value["usage"]["output_tokens"], 2);
    }
}
