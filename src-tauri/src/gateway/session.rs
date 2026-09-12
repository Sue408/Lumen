//! 会话解析：把下游请求头里的会话标识「捕获」成一个可聚合的维度。
//!
//! 会话不做实体化、不做生命周期、不做合成——只按候选头名表取第一个非空值；
//! 客户端没带就返回 `None`（落库为 `NULL`）。统计与追踪全部由 `request_logs`
//! 按 `(virtual_key_id, session_id)` 聚合得出。

use axum::http::HeaderMap;

/// 覆盖 Codex / Claude Code / OpenCode 的默认候选头名（按优先级）。
/// OpenCode 自家的 `x-opencode-session` 排在最前；其余为第三方 provider 走
/// `x-session-affinity` / `X-Session-Id` 的形态。
pub const DEFAULT_SESSION_HEADERS: &[&str] = &[
    "x-opencode-session",
    "x-session-affinity",
    "x-session-id",
    "x-claude-code-session-id",
    "session_id",
];

/// 按候选顺序取第一个非空请求头值；找不到返回 `None`。
///
/// 头名比较大小写不敏感（`HeaderMap` 原生支持）；空串 / 全空白视为未命中；
/// 非 UTF-8 值跳过继续找下一个候选。
pub fn resolve_session(headers: &HeaderMap, candidates: &[String]) -> Option<String> {
    candidates.iter().find_map(|name| {
        headers
            .get(name.as_str())
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn candidates(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn picks_the_first_non_empty_candidate() {
        let mut headers = HeaderMap::new();
        headers.insert("x-session-id", HeaderValue::from_static("ses_b"));
        headers.insert("x-opencode-session", HeaderValue::from_static("ses_a"));
        let resolved = resolve_session(
            &headers,
            &candidates(&["x-opencode-session", "x-session-id"]),
        );
        assert_eq!(resolved.as_deref(), Some("ses_a"));
    }

    #[test]
    fn skips_blank_values() {
        let mut headers = HeaderMap::new();
        headers.insert("x-opencode-session", HeaderValue::from_static("   "));
        headers.insert("session_id", HeaderValue::from_static("ses_real"));
        let resolved =
            resolve_session(&headers, &candidates(&["x-opencode-session", "session_id"]));
        assert_eq!(resolved.as_deref(), Some("ses_real"));
    }

    #[test]
    fn returns_none_when_all_missing() {
        let headers = HeaderMap::new();
        assert_eq!(
            resolve_session(&headers, &candidates(&["x-opencode-session", "session_id"])),
            None
        );
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Claude-Code-Session-Id", HeaderValue::from_static("ses_cc"));
        let resolved =
            resolve_session(&headers, &candidates(&["X-CLAUDE-CODE-SESSION-ID"]));
        assert_eq!(resolved.as_deref(), Some("ses_cc"));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let mut headers = HeaderMap::new();
        headers.insert("x-session-id", HeaderValue::from_static("  ses_x  "));
        assert_eq!(
            resolve_session(&headers, &candidates(&["x-session-id"])).as_deref(),
            Some("ses_x")
        );
    }

    #[test]
    fn empty_candidate_table_yields_none() {
        let mut headers = HeaderMap::new();
        headers.insert("x-session-id", HeaderValue::from_static("ses_x"));
        assert_eq!(resolve_session(&headers, &[]), None);
    }
}
