//! 响应回程的模型名还原：把上游响应里暴露的真实模型名改回客户端请求的别名。
//!
//! 别名是网关对外的唯一契约，真实模型名只在发往上游时注入（见
//! `handlers::build_attempt`）。回程若原样透传，客户端就会看到
//! `claude-sonnet-4-5-20250929`，而不是自己选的 `anthropic/claude-sonnet`——
//! 同协议字节透传与跨协议转换两条路都存在这个问题。

use std::borrow::Cow;

/// 需要还原的 JSON 键：OpenAI / Anthropic / Responses 用 `model`，Gemini 用 `modelVersion`。
const KEYS: [&[u8]; 2] = [b"\"model\"", b"\"modelVersion\""];

/// 单条流式行允许积压的上限；超过就提前吐出，避免上游不发换行时缓存无限增长。
const MAX_PENDING: usize = 1_000_000;

/// 把 `text` 中**作为 JSON 键**出现的 `model` / `modelVersion` 的值换成 `alias`。
///
/// 按字节扫描而非解析后重序列化：上游排版原样保留，也不会误伤正文里恰好出现的
/// `"model"` 字样——JSON 字符串内部的引号会被转义成 `\"`，凑不出 `"model"` 这个
/// 连续字节序列。值不是字符串、或本身已是别名时原样返回。
pub fn restore_model<'a>(text: &'a [u8], alias: &str) -> Cow<'a, [u8]> {
    if alias.is_empty() {
        return Cow::Borrowed(text);
    }
    let mut out: Option<Vec<u8>> = None;
    // 已写进 out 的文本位置；每次替换后从这里接续。
    let mut cursor = 0usize;
    let mut search = 0usize;
    while let Some((key_at, key_len)) = find_key(text, search) {
        let key_end = key_at + key_len;
        let Some((value_start, value_end)) = string_value_range(text, key_end) else {
            search = key_end;
            continue;
        };
        if &text[value_start..value_end] != alias.as_bytes() {
            let out = out.get_or_insert_with(|| Vec::with_capacity(text.len() + alias.len()));
            out.extend_from_slice(&text[cursor..value_start]);
            out.extend_from_slice(alias.as_bytes());
            cursor = value_end;
        }
        search = value_end;
    }
    match out {
        Some(mut out) => {
            out.extend_from_slice(&text[cursor..]);
            Cow::Owned(out)
        }
        None => Cow::Borrowed(text),
    }
}

/// 非流式响应体（`String`）的入口：未发生替换时零拷贝返回原串。
pub fn restore_model_string(text: String, alias: &str) -> String {
    let owned = match restore_model(text.as_bytes(), alias) {
        Cow::Borrowed(_) => return text,
        Cow::Owned(bytes) => bytes,
    };
    // 只是在合法 UTF-8 里换了另一段合法 UTF-8，失败分支只是防御性的兜底。
    String::from_utf8(owned).unwrap_or(text)
}

/// 找到 `from` 之后最早出现的目标键，返回到键起点的偏移与键长。
fn find_key(text: &[u8], from: usize) -> Option<(usize, usize)> {
    if from >= text.len() {
        return None;
    }
    let rest = &text[from..];
    KEYS.iter()
        .filter_map(|key| find_bytes(rest, key).map(|offset| (from + offset, key.len())))
        .min_by_key(|(at, _)| *at)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}

/// 从键末尾出发，跳过分隔符后取出字符串值的裸区间；不是 `"key": "value"` 形状则 None。
fn string_value_range(text: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut index = from;
    while matches!(text.get(index), Some(b' ' | b'\t' | b'\r' | b'\n')) {
        index += 1;
    }
    if text.get(index) != Some(&b':') {
        return None;
    }
    index += 1;
    while matches!(text.get(index), Some(b' ' | b'\t' | b'\r' | b'\n')) {
        index += 1;
    }
    if text.get(index) != Some(&b'"') {
        return None;
    }
    let value_start = index + 1;
    let mut index = value_start;
    while index < text.len() {
        match text[index] {
            // 跳过转义对（`\"` 不会提前结束字符串）。
            b'\\' => index += 2,
            b'"' => return Some((value_start, index)),
            _ => index += 1,
        }
    }
    None
}

/// 流式回程的按行还原器。
///
/// 上游的分块边界是任意的，`"model"` 键可能被切成两半，所以必须攒到换行为止再改写；
/// 换行之前不吐字节也只是等到一个完整 SSE 事件，不会额外增加客户端的可感知延迟。
pub struct StreamRestorer {
    alias: String,
    pending: Vec<u8>,
}

impl StreamRestorer {
    pub fn new(alias: impl Into<String>) -> Self {
        Self {
            alias: alias.into(),
            pending: Vec::new(),
        }
    }

    /// 喂入一段（已按客户端协议就绪的）字节，返回可以立即转发的部分。
    pub fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.pending.extend_from_slice(chunk);
        let Some(last_newline) = self.pending.iter().rposition(|byte| *byte == b'\n') else {
            if self.pending.len() > MAX_PENDING {
                return self.drain_all();
            }
            return Vec::new();
        };
        let ready: Vec<u8> = self.pending.drain(..=last_newline).collect();
        restore_model(&ready, &self.alias).into_owned()
    }

    /// 流结束（或中断）时冲刷尾部未换行的残留。
    pub fn finish(&mut self) -> Vec<u8> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        self.drain_all()
    }

    fn drain_all(&mut self) -> Vec<u8> {
        let pending = std::mem::take(&mut self.pending);
        restore_model(&pending, &self.alias).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn restored(text: &str, alias: &str) -> String {
        String::from_utf8(restore_model(text.as_bytes(), alias).into_owned()).unwrap()
    }

    #[test]
    fn rewrites_top_level_model() {
        let body = r#"{"id":"cmpl-1","object":"chat.completion","model":"gpt-4o-mini","choices":[]}"#;
        assert_eq!(
            restored(body, "lumen/mini"),
            r#"{"id":"cmpl-1","object":"chat.completion","model":"lumen/mini","choices":[]}"#
        );
    }

    #[test]
    fn rewrites_nested_models_used_by_anthropic_and_responses() {
        let anthropic = r#"{"type":"message_start","message":{"id":"msg_1","model":"claude-up"}}"#;
        assert_eq!(
            restored(anthropic, "lumen/claude"),
            r#"{"type":"message_start","message":{"id":"msg_1","model":"lumen/claude"}}"#
        );
        let responses = r#"{"type":"response.created","response":{"model":"gpt-up"}}"#;
        assert_eq!(
            restored(responses, "lumen/gpt"),
            r#"{"type":"response.created","response":{"model":"lumen/gpt"}}"#
        );
    }

    #[test]
    fn rewrites_gemini_model_version() {
        let body = r#"{"candidates":[],"modelVersion":"gemini-2.5-pro"}"#;
        assert_eq!(
            restored(body, "lumen/gemini"),
            r#"{"candidates":[],"modelVersion":"lumen/gemini"}"#
        );
    }

    #[test]
    fn leaves_model_words_inside_content_alone() {
        // 正文里的引号必然被转义，不会形成 `"model"` 这个字节序列。
        let body = r#"{"model":"up","choices":[{"message":{"content":"use \"model\": \"up\" verbatim"}}]}"#;
        assert_eq!(
            restored(body, "lumen/mock"),
            r#"{"model":"lumen/mock","choices":[{"message":{"content":"use \"model\": \"up\" verbatim"}}]}"#
        );
    }

    #[test]
    fn tolerates_whitespace_around_the_colon() {
        let body = "{\n  \"model\" : \t\"up\"\n}";
        assert_eq!(restored(body, "alias/x"), "{\n  \"model\" : \t\"alias/x\"\n}");
    }

    #[test]
    fn returns_input_untouched_when_nothing_to_rewrite() {
        let body = r#"{"id":"1"}"#;
        assert!(matches!(
            restore_model(body.as_bytes(), "alias/x"),
            Cow::Borrowed(_)
        ));
        // 已经是别名：不产生任何改动。
        let already = r#"{"model":"alias/x"}"#;
        assert!(matches!(
            restore_model(already.as_bytes(), "alias/x"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn non_string_values_and_malformed_input_are_left_alone() {
        assert_eq!(restored(r#"{"model":null}"#, "alias/x"), r#"{"model":null}"#);
        assert_eq!(restored(r#"broken {"model"#, "alias/x"), r#"broken {"model"#);
    }

    #[test]
    fn stream_restorer_waits_for_a_line_boundary() {
        let mut restorer = StreamRestorer::new("lumen/mock");
        // 模型名被上游切块切开：前半段落不吐字节。
        assert!(restorer.push(b"data: {\"model\":\"mo").is_empty());
        let out = restorer.push(b"ck-model\"}\n\n");
        assert_eq!(String::from_utf8(out).unwrap(), "data: {\"model\":\"lumen/mock\"}\n\n");
    }

    #[test]
    fn stream_restorer_flushes_unterminated_tail() {
        let mut restorer = StreamRestorer::new("lumen/mock");
        let pass = restorer.push(b"data: {\"model\":\"mock-model\"}");
        assert!(pass.is_empty());
        assert_eq!(
            String::from_utf8(restorer.finish()).unwrap(),
            "data: {\"model\":\"lumen/mock\"}"
        );
        assert!(restorer.finish().is_empty(), "冲刷后不应再有残留");
    }
}
