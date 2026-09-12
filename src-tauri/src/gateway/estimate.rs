use serde_json::Value;

use crate::db::models::{
    PROTOCOL_ANTHROPIC, PROTOCOL_GEMINI, PROTOCOL_OPENAI, PROTOCOL_RESPONSES,
};

/// 请求侧与响应侧的估算 token。`None` 表示该侧无文本、无法估算。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Estimates {
    pub input: Option<i64>,
    pub output: Option<i64>,
}

/// 累积输出文本的上限，避免异常长的流占满内存。
const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

/// 单条字符串超过该长度且不含空白时视为二进制块（base64 图片 / 音频），跳过计数。
const BLOB_LEN: usize = 2048;

/// 估算一段文本的 token 数：优先按模型匹配 BPE 词表，未知模型回退字符启发式。
pub fn count_tokens(model: &str, text: &str) -> i64 {
    if text.is_empty() {
        return 0;
    }
    if !model.is_empty() {
        if let Some(encoding) = tiktoken::encoding_for_model(model) {
            return encoding.count(text) as i64;
        }
        if let Some(name) = fallback_encoding(model) {
            if let Some(encoding) = tiktoken::get_encoding(name) {
                return encoding.count(text) as i64;
            }
        }
    }
    heuristic_tokens(text)
}

/// 词表库未直接收录上游别名时的关键词归一：把常见模型名映射到对应编码。
fn fallback_encoding(model: &str) -> Option<&'static str> {
    let model = model.to_ascii_lowercase();
    let name = if model.contains("claude") {
        "cl100k_base"
    } else if model.contains("gemini") {
        "o200k_base"
    } else if model.contains("deepseek") {
        "deepseek_v3"
    } else if model.contains("qwen") {
        "qwen2"
    } else if model.contains("glm") {
        "glm4"
    } else if model.contains("kimi") || model.contains("moonshot") {
        "kimi_k2"
    } else if model.contains("minimax") {
        "minimax_m2"
    } else if model.contains("llama") {
        "llama3"
    } else if model.contains("mistral") || model.contains("mixtral") {
        "mistral_v3"
    } else if model.contains("gpt-oss") {
        "o200k_harmony"
    } else if model.contains("gpt-4o")
        || model.contains("gpt-4.1")
        || model.contains("gpt-5")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
    {
        "o200k_base"
    } else if model.contains("gpt-4") || model.contains("gpt-3.5") {
        "cl100k_base"
    } else {
        return None;
    };
    Some(name)
}

/// 字符启发式：CJK 字符每字约 1 token，其余每 4 字符约 1 token（向上取整）。
pub fn heuristic_tokens(text: &str) -> i64 {
    let mut wide = 0i64;
    let mut narrow = 0i64;
    for ch in text.chars() {
        if is_cjk(ch) {
            wide += 1;
        } else {
            narrow += 1;
        }
    }
    wide + (narrow + 3) / 4
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3040..=0x30FF      // 平假名 / 片假名
            | 0x3400..=0x4DBF // CJK 扩展 A
            | 0x4E00..=0x9FFF // CJK 统一表意
            | 0xF900..=0xFAFF // CJK 兼容表意
            | 0xAC00..=0xD7AF // 韩文音节
            | 0x20000..=0x2FA1F // CJK 扩展 B~F、兼容补充
    )
}

/// 估算请求输入的 prompt token：递归收集请求体中所有文本（含消息、系统提示、
/// 工具 schema 与结构化 JSON），跳过模型名与疑似 base64 / 二进制块。
pub fn estimate_input(model: &str, body: &Value) -> i64 {
    let mut text = String::new();
    collect_prompt(body, &mut text);
    count_tokens(model, &text)
}

fn collect_prompt(value: &Value, out: &mut String) {
    match value {
        Value::String(text) if !is_blob(text) => {
            out.push_str(text);
            out.push('\n');
        }
        Value::Array(items) => {
            for item in items {
                collect_prompt(item, out);
            }
        }
        Value::Object(map) => {
            for (key, child) in map {
                if key == "model" {
                    continue;
                }
                collect_prompt(child, out);
            }
        }
        _ => {}
    }
}

fn is_blob(text: &str) -> bool {
    text.len() > BLOB_LEN && !text.chars().any(|ch| ch.is_whitespace())
}

fn push_str(out: &mut String, value: Option<&Value>) {
    if let Some(text) = value.and_then(Value::as_str) {
        out.push_str(text);
    }
}

fn push_json(value: &Value, out: &mut String) {
    if value.is_null() {
        return;
    }
    if let Ok(text) = serde_json::to_string(value) {
        out.push_str(&text);
    }
}

/// 累积流式 SSE 事件中的增量输出文本（含 reasoning / thinking / 工具参数）。
pub fn collect_stream_delta(protocol: &str, event: &Value, out: &mut String) {
    if out.len() >= MAX_OUTPUT_BYTES {
        return;
    }
    match protocol {
        PROTOCOL_OPENAI => {
            if let Some(choices) = event.get("choices").and_then(Value::as_array) {
                for choice in choices {
                    let delta = &choice["delta"];
                    push_str(out, delta.get("content"));
                    push_str(out, delta.get("reasoning_content"));
                    if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                        for call in calls {
                            push_str(out, call.pointer("/function/arguments"));
                        }
                    }
                }
            }
        }
        PROTOCOL_ANTHROPIC => {
            let delta = &event["delta"];
            push_str(out, delta.get("text"));
            push_str(out, delta.get("thinking"));
            push_str(out, delta.get("partial_json"));
        }
        // Responses 的各 `.delta` 事件（output_text / reasoning_summary / function_args）
        // 增量都挂在顶层 `delta` 上。
        PROTOCOL_RESPONSES => push_str(out, event.get("delta")),
        PROTOCOL_GEMINI => collect_gemini(event, out),
        _ => {}
    }
}

/// 从完整（非流式）响应中提取全部输出文本（含 reasoning / thinking / 工具参数）。
pub fn response_text(protocol: &str, value: &Value) -> String {
    let mut out = String::new();
    match protocol {
        PROTOCOL_OPENAI => {
            if let Some(choices) = value.get("choices").and_then(Value::as_array) {
                for choice in choices {
                    let message = &choice["message"];
                    push_str(&mut out, message.get("content"));
                    push_str(&mut out, message.get("reasoning_content"));
                    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
                        for call in calls {
                            push_str(&mut out, call.pointer("/function/arguments"));
                        }
                    }
                }
            }
        }
        PROTOCOL_ANTHROPIC => {
            if let Some(blocks) = value.get("content").and_then(Value::as_array) {
                for block in blocks {
                    push_str(&mut out, block.get("text"));
                    push_str(&mut out, block.get("thinking"));
                    push_json(&block["input"], &mut out);
                }
            }
        }
        PROTOCOL_RESPONSES => {
            if let Some(items) = value.get("output").and_then(Value::as_array) {
                for item in items {
                    if let Some(content) = item.get("content").and_then(Value::as_array) {
                        for block in content {
                            push_str(&mut out, block.get("text"));
                        }
                    }
                    if let Some(summary) = item.get("summary").and_then(Value::as_array) {
                        for block in summary {
                            push_str(&mut out, block.get("text"));
                        }
                    }
                    push_str(&mut out, item.get("arguments"));
                }
            }
        }
        PROTOCOL_GEMINI => collect_gemini(value, &mut out),
        _ => {}
    }
    out
}

fn collect_gemini(value: &Value, out: &mut String) {
    if let Some(candidates) = value.get("candidates").and_then(Value::as_array) {
        for candidate in candidates {
            if let Some(parts) = candidate
                .pointer("/content/parts")
                .and_then(Value::as_array)
            {
                for part in parts {
                    push_str(out, part.get("text"));
                    push_json(&part["functionCall"], out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn known_model_uses_real_tokenizer() {
        // o200k_base 对这句英文是 2 个 token（与 OpenAI 官方一致）。
        assert_eq!(count_tokens("gpt-4o", "hello world"), 2);
    }

    #[test]
    fn alias_falls_back_to_vendor_encoding() {
        // 上游别名不在词表库中，按家族回退到对应编码，不应退化成启发式。
        assert_eq!(count_tokens("deepseek-chat", "hello world"), 2);
        assert_eq!(count_tokens("claude-sonnet-4", "hello world"), 2);
    }

    #[test]
    fn unknown_model_uses_heuristic() {
        let text = "hello world";
        assert_eq!(count_tokens("totally-unknown-xyz", text), heuristic_tokens(text));
    }

    #[test]
    fn heuristic_counts_cjk_per_character() {
        assert_eq!(heuristic_tokens("你好世界"), 4);
        assert_eq!(heuristic_tokens("hello world"), 3);
        assert_eq!(heuristic_tokens(""), 0);
    }

    #[test]
    fn estimate_input_collects_messages_and_tools() {
        let body = json!({
            "model": "gpt-4o",
            "stream": true,
            "messages": [
                { "role": "system", "content": "你是助手" },
                { "role": "user", "content": "你好" }
            ],
            "tools": [{ "type": "function", "function": { "name": "lookup" } }]
        });
        // 模型名本身不计入；消息与工具定义都应有贡献。
        assert!(estimate_input("gpt-4o", &body) > 0);
        let without_text = json!({ "model": "gpt-4o", "stream": true });
        assert_eq!(estimate_input("gpt-4o", &without_text), 0);
    }

    #[test]
    fn estimate_input_skips_base64_blobs() {
        let blob = "A".repeat(10_000);
        assert!(is_blob(&blob));
        let body = json!({ "messages": [{ "role": "user", "content": blob }] });
        // base64 图片若被完整计入，估值会达到数千；跳过二进制块后只剩少量结构文本。
        assert!(estimate_input("gpt-4o", &body) < 100);
    }

    #[test]
    fn response_text_reads_openai_content_and_tool_arguments() {
        let value = json!({
            "choices": [{
                "message": {
                    "content": "hello",
                    "tool_calls": [{ "function": { "arguments": "{\"a\":1}" } }]
                }
            }]
        });
        let text = response_text(PROTOCOL_OPENAI, &value);
        assert!(text.contains("hello"));
        assert!(text.contains("\"a\":1"));
    }

    #[test]
    fn response_text_reads_anthropic_blocks() {
        let value = json!({
            "content": [
                { "type": "thinking", "thinking": "hmm" },
                { "type": "text", "text": "answer" }
            ]
        });
        let text = response_text(PROTOCOL_ANTHROPIC, &value);
        assert!(text.contains("hmm"));
        assert!(text.contains("answer"));
    }

    #[test]
    fn stream_delta_accumulates_openai_and_gemini() {
        let mut openai = String::new();
        collect_stream_delta(
            PROTOCOL_OPENAI,
            &json!({ "choices": [{ "delta": { "content": "he" } }] }),
            &mut openai,
        );
        collect_stream_delta(
            PROTOCOL_OPENAI,
            &json!({ "choices": [{ "delta": { "content": "llo" } }] }),
            &mut openai,
        );
        assert_eq!(openai, "hello");

        let mut gemini = String::new();
        collect_stream_delta(
            PROTOCOL_GEMINI,
            &json!({ "candidates": [{ "content": { "parts": [{ "text": "hi" }] } }] }),
            &mut gemini,
        );
        assert_eq!(gemini, "hi");
    }
}
