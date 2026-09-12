use axum::http::{header, HeaderMap};

use crate::db::keys::find_enabled_virtual_key;
use crate::db::models::VirtualKey;
use crate::db::with_db;
use crate::error::AppError;
use crate::state::AppState;

fn header_key(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// 从 `Authorization: Bearer`、`x-api-key` 或 `x-goog-api-key` 中取出密钥原文。
/// 后者用于兼容原生 Gemini 客户端的鉴权习惯。
fn extract_key(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        let value = value.trim();
        if let Some(token) = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
        {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    header_key(headers, "x-api-key").or_else(|| header_key(headers, "x-goog-api-key"))
}

/// 解析并校验虚拟密钥；缺失、未知或已停用一律视为未授权。
pub async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<VirtualKey, AppError> {
    let key = extract_key(headers).ok_or(AppError::Unauthorized)?;
    with_db(&state.db, move |conn| find_enabled_virtual_key(conn, &key))
        .await?
        .ok_or(AppError::Unauthorized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        headers
    }

    #[test]
    fn extracts_bearer_x_api_key_and_goog_key() {
        assert_eq!(
            extract_key(&headers(&[("authorization", "Bearer sk-lumen-a")])).as_deref(),
            Some("sk-lumen-a")
        );
        assert_eq!(
            extract_key(&headers(&[("x-api-key", "sk-lumen-b")])).as_deref(),
            Some("sk-lumen-b")
        );
        assert_eq!(
            extract_key(&headers(&[("x-goog-api-key", "sk-lumen-c")])).as_deref(),
            Some("sk-lumen-c")
        );
    }

    #[test]
    fn ignores_empty_or_malformed_authorization() {
        assert!(extract_key(&headers(&[])).is_none());
        assert!(extract_key(&headers(&[("authorization", "Bearer "), ("x-api-key", "  ")])).is_none());
        assert!(extract_key(&headers(&[("authorization", "Basic abc")])).is_none());
    }
}
