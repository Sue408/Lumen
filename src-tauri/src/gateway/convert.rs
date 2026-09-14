//! 跨协议转换：当入站协议与上游端点协议不一致时，把请求与响应交给 `llmwire`。
//!
//! 这里是 `llmwire` 在 Lumen 内的**唯一适配点**——协议标识映射、能力策略装配、
//! 请求 / 响应 / 流式转换与报告收集都在此收口。同协议路径不经过本模块，继续字节透传。
// Task 2/3 把本模块接入 handlers / forward 之前，入口尚无调用点。
#![allow(dead_code)]

use llmwire::{converter, resolve, Converter, ProtocolId, Report, Termination};

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

/// 一次请求的转换器，封装 `llmwire::Converter` 与两端协议，隔离 SDK 公开接口。
pub struct Conversion {
    converter: Box<dyn Converter>,
    inbound: String,
    upstream: String,
}

impl Conversion {
    /// 以入站协议为源、上游端点协议为目标构造转换器。
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

    /// 取走本次转换累积的质量报告。
    pub fn take_report(&mut self) -> Report {
        self.converter.take_report()
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
    use serde_json::Value;

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
