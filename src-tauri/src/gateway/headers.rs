//! per-provider 请求头映射：由**下游请求头**构建**上游请求头**（四意图）。
//!
//! 求值顺序固定为 `内置底座 → forward(透传) → replace(替换) → extra_headers(添加)
//! → remove(移除) → 硬黑名单`。鉴权头不在这里注入——由 `forward::send` 在规则结果
//! 之后无条件覆盖，保证规则永远无法伪造鉴权。硬黑名单则无条件剥离，防止把客户端的
//! 传输层头带坏上游。

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use std::collections::BTreeMap;

use crate::db::models::ProviderHeaderRules;

/// 无条件剥离的硬黑名单：传输层 / 鉴权头，任何规则都不能让它们出现在上游请求里。
pub const HARD_BLOCKLIST: &[&str] = &[
    "host",
    "content-length",
    "connection",
    "transfer-encoding",
    "authorization",
    "x-api-key",
    "x-goog-api-key",
];

/// 内置默认放行底座：这些客户端头 Lumen 认为安全，开箱即转发，无需用户配置。
/// 保守起步；条目可被 `remove` 否决。前端 `providerHeaderModel.ts` 镜像本表。
pub const DEFAULT_FORWARD_HEADERS: &[&str] = &[
    "user-agent",
    "accept-language",
    "traceparent",
    "tracestate",
    "anthropic-version",
    "anthropic-beta",
    "openai-beta",
    "openai-organization",
    "openai-project",
];

/// 通配匹配：`*` 匹配任意序列，大小写不敏感。头名均为 ASCII，按字节处理。
fn name_matches(pattern: &str, name: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let name = name.to_ascii_lowercase();
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    if !name.starts_with(parts[0]) {
        return false;
    }
    let mut cursor = parts[0].len();
    for (index, part) in parts.iter().enumerate().skip(1) {
        if index == parts.len() - 1 {
            return name.len() - cursor >= part.len() && name[cursor..].ends_with(part);
        }
        if part.is_empty() {
            continue;
        }
        match name[cursor..].find(part) {
            Some(position) => cursor += position + part.len(),
            None => return false,
        }
    }
    true
}

fn header_name(raw: &str) -> Option<HeaderName> {
    HeaderName::from_bytes(raw.trim().as_bytes()).ok()
}

/// 按 provider 的四意图构建上游请求头（不含鉴权；鉴权由调用方最后注入）。
///
/// 顺序固定：内置底座 → `forward`（透传）→ `replace`（替换）→ `extra_headers`
/// （添加）→ `remove`（移除，deny wins）→ 硬黑名单。
pub fn build_upstream_headers(
    client: &HeaderMap,
    rules: &ProviderHeaderRules,
    extra_headers: &BTreeMap<String, String>,
) -> HeaderMap {
    let mut output = HeaderMap::new();

    // 1. 内置底座：安全头默认放行。
    for name in DEFAULT_FORWARD_HEADERS {
        for value in client.get_all(*name) {
            output.append(HeaderName::from_static(name), value.clone());
        }
    }

    // 2. 透传：按白名单 / 通配从下游追加放行（多值保留）。
    for (name, value) in client.iter() {
        if rules
            .forward
            .iter()
            .any(|pattern| name_matches(pattern, name.as_str()))
        {
            output.append(name.clone(), value.clone());
        }
    }

    // 3. 替换：把客户端 `from` 的值以 `to` 的名字写出（覆盖目标名既有值）。
    for rule in &rules.replace {
        let Some(to_name) = header_name(&rule.to) else {
            continue;
        };
        let values: Vec<HeaderValue> = client.get_all(rule.from.as_str()).iter().cloned().collect();
        if values.is_empty() {
            continue;
        }
        output.remove(to_name.clone());
        for value in values {
            output.append(to_name.clone(), value);
        }
    }

    // 4. 添加：常量覆盖。
    for (name, value) in extra_headers {
        if let (Some(header), Ok(header_value)) =
            (header_name(name), HeaderValue::from_str(value))
        {
            output.insert(header, header_value);
        }
    }

    // 5. 移除：按通配删除（最高优先级）。
    let to_remove: Vec<HeaderName> = output
        .keys()
        .filter(|name| {
            rules
                .remove
                .iter()
                .any(|pattern| name_matches(pattern, name.as_str()))
        })
        .cloned()
        .collect();
    for name in to_remove {
        output.remove(&name);
    }

    // 6. 硬黑名单：无条件剥离。
    for blocked in HARD_BLOCKLIST {
        output.remove(*blocked);
    }

    output
}

/// 保存前校验 provider 头规则：通配合法、替换名非空且精确、不得指向受保护的头。
pub fn validate_provider_header_rules(rules: &ProviderHeaderRules) -> Result<(), String> {
    for pattern in &rules.forward {
        validate_pattern("forward", pattern)?;
    }
    for pattern in &rules.remove {
        if pattern.trim().is_empty() {
            return Err("remove 含空的头名模式".to_string());
        }
    }
    for rule in &rules.replace {
        validate_exact_name("replace.from", &rule.from)?;
        validate_exact_name("replace.to", &rule.to)?;
        reject_blocklisted("replace", &rule.from)?;
        reject_blocklisted("replace", &rule.to)?;
    }
    Ok(())
}

/// 精确头名校验：非空、无通配、token 合法。
fn validate_exact_name(label: &str, name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(format!("{label} 不得为空"));
    }
    if name.contains('*') {
        return Err(format!("{label} 不支持通配：{name}"));
    }
    if !name.chars().all(is_pattern_char) {
        return Err(format!("{label} 含非法字符：{name}"));
    }
    Ok(())
}

/// RFC 7230 token 字符集（头名的全部合法字符；`*` 在本模块兼作通配）。
/// 放开到完整 token，才能放行 `session_id` 这类非 `x-` 前缀 / 带下划线的头名。
fn is_pattern_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || matches!(
            ch,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
        )
}

fn validate_pattern(label: &str, pattern: &str) -> Result<(), String> {
    if pattern.trim().is_empty() {
        return Err(format!("{label} 含空的头名模式"));
    }
    if !pattern.chars().all(is_pattern_char) {
        return Err(format!("{label} 含非法字符：{pattern}"));
    }
    reject_blocklisted(label, pattern)
}

fn reject_blocklisted(label: &str, name: &str) -> Result<(), String> {
    if HARD_BLOCKLIST
        .iter()
        .any(|blocked| name_matches(name, blocked))
    {
        return Err(format!("{label} 不得指向受保护的头：{name}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{HeaderReplace, ProviderHeaderRules};
    use std::collections::BTreeMap;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    fn provider_rules(forward: &[&str], remove: &[&str]) -> ProviderHeaderRules {
        ProviderHeaderRules {
            forward: forward.iter().map(|name| name.to_string()).collect(),
            replace: Vec::new(),
            remove: remove.iter().map(|name| name.to_string()).collect(),
        }
    }

    #[test]
    fn default_base_forwards_safe_headers_only() {
        let client = headers(&[("user-agent", "codex/1.0"), ("x-secret", "no")]);
        let out =
            build_upstream_headers(&client, &ProviderHeaderRules::default(), &BTreeMap::new());
        assert_eq!(out.get("user-agent").unwrap(), "codex/1.0");
        assert!(out.get("x-secret").is_none(), "非底座头默认不放行");
    }

    #[test]
    fn forward_allows_non_x_prefixed_names() {
        let client = headers(&[("session_id", "ses_1"), ("other", "no")]);
        let rules = provider_rules(&["session_id"], &[]);
        let out = build_upstream_headers(&client, &rules, &BTreeMap::new());
        assert_eq!(out.get("session_id").unwrap(), "ses_1");
        assert!(out.get("other").is_none());
    }

    #[test]
    fn replace_maps_session_header_to_target_name() {
        let client = headers(&[("session_id", "ses_1")]);
        let mut rules = ProviderHeaderRules::default();
        rules.replace.push(HeaderReplace {
            from: "session_id".into(),
            to: "x-opencode-session".into(),
        });
        let out = build_upstream_headers(&client, &rules, &BTreeMap::new());
        assert_eq!(out.get("x-opencode-session").unwrap(), "ses_1");
        assert!(out.get("session_id").is_none(), "替换不保留原名");
    }

    #[test]
    fn extra_headers_override_base_and_forwarded_values() {
        let client = headers(&[("user-agent", "original")]);
        let extra = BTreeMap::from([("user-agent".to_string(), "opencode/local".to_string())]);
        let out = build_upstream_headers(&client, &ProviderHeaderRules::default(), &extra);
        assert_eq!(out.get("user-agent").unwrap(), "opencode/local");
    }

    #[test]
    fn remove_wins_over_base_forward_and_add() {
        let client = headers(&[("user-agent", "ua"), ("traceparent", "t")]);
        let extra = BTreeMap::from([("keep-me".to_string(), "1".to_string())]);
        let rules = provider_rules(&["*"], &["user-agent", "keep-me"]);
        let out = build_upstream_headers(&client, &rules, &extra);
        assert!(out.get("user-agent").is_none());
        assert!(out.get("keep-me").is_none());
        assert_eq!(out.get("traceparent").unwrap(), "t");
    }

    #[test]
    fn hard_blocklist_survives_provider_add() {
        let extra = BTreeMap::from([
            ("authorization".to_string(), "Bearer forged".to_string()),
            ("host".to_string(), "evil".to_string()),
        ]);
        let out = build_upstream_headers(&HeaderMap::new(), &ProviderHeaderRules::default(), &extra);
        assert!(out.get("authorization").is_none());
        assert!(out.get("host").is_none());
    }

    #[test]
    fn validate_provider_rules_checks_replace_names() {
        let mut wildcard = ProviderHeaderRules::default();
        wildcard.replace.push(HeaderReplace {
            from: "session_id".into(),
            to: "*".into(),
        });
        assert!(validate_provider_header_rules(&wildcard).is_err());

        let mut valid = ProviderHeaderRules::default();
        valid.forward.push("session_*".into());
        valid.replace.push(HeaderReplace {
            from: "session_id".into(),
            to: "x-opencode-session".into(),
        });
        assert!(validate_provider_header_rules(&valid).is_ok());
    }
}
