use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::db::models::PROTOCOL_GEMINI;
use crate::db::providers::get_provider;
use crate::db::with_db;
use crate::error::AppError;
use crate::gateway::forward::{describe_upstream_error, fetch};
use crate::gateway::resolve::endpoint_route;
use crate::state::AppState;

/// 拉取模型列表的超时：与探测同量级，避免卡住界面。
const MODELS_TIMEOUT: Duration = Duration::from_secs(15);

/// 上游返回的一个模型条目。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteModel {
    /// 上游真实模型名，可直接写入 `upstream_models.model_id`。
    pub id: String,
    /// 上游给的展示名（若提供）。
    pub display_name: Option<String>,
}

fn display_of(item: &Value) -> Option<String> {
    item.get("display_name")
        .or_else(|| item.get("displayName"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

/// 从各协议的 `/models` 响应里抽出模型名。纯函数，便于单测。
/// OpenAI / Anthropic / Responses 是 `{ data: [{ id, display_name? }] }`，
/// Gemini 是 `{ models: [{ name: "models/xxx", displayName? }] }`（前缀要剥掉）。
pub fn parse_remote_models(protocol: &str, value: &Value) -> Vec<RemoteModel> {
    let gemini = protocol == PROTOCOL_GEMINI;
    let items = if gemini {
        value.get("models")
    } else {
        value.get("data")
    };
    let Some(items) = items.and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let raw = if gemini {
                item.get("name").and_then(Value::as_str)
            } else {
                item.get("id").and_then(Value::as_str)
            }?;
            let id = if gemini {
                raw.strip_prefix("models/").unwrap_or(raw)
            } else {
                raw
            };
            if id.is_empty() {
                return None;
            }
            Some(RemoteModel {
                id: id.to_string(),
                display_name: display_of(item),
            })
        })
        .collect()
}

/// 向上游某协议端点拉取它暴露的模型列表；`protocol` 缺省用提供商的第一个端点。
/// 与探测一样**不写日志、不计费、不触发冷却**。返回按模型名排序。
pub async fn list_remote_models(
    state: Arc<AppState>,
    provider_id: String,
    protocol: Option<String>,
) -> Result<Vec<RemoteModel>, AppError> {
    let provider = with_db(&state.db, move |conn| {
        get_provider(conn, &provider_id)?.ok_or_else(|| AppError::message("上游提供商不存在"))
    })
    .await?;

    let protocol = protocol.filter(|value| !value.is_empty());
    let endpoint = match protocol.as_deref() {
        Some(protocol) => provider
            .endpoints
            .iter()
            .find(|endpoint| endpoint.protocol == protocol)
            .ok_or_else(|| {
                AppError::message(format!(
                    "「{}」没有 {protocol} 协议的端点",
                    provider.provider.name
                ))
            })?,
        None => provider.endpoints.first().ok_or_else(|| {
            AppError::message(format!(
                "「{}」没有协议端点，无法拉取模型",
                provider.provider.name
            ))
        })?,
    };

    let route = endpoint_route(&provider.provider, endpoint);
    let response = fetch(&state, &route, "models", Some(MODELS_TIMEOUT)).await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(AppError::message(describe_upstream_error(status, &text)));
    }

    let value: Value = response.json().await?;
    let mut models = parse_remote_models(&endpoint.protocol, &value);
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::PROTOCOL_OPENAI;
    use serde_json::json;

    #[test]
    fn parses_openai_style_list() {
        let value = json!({ "data": [
            { "id": "gpt-4o", "object": "model" },
            { "id": "gpt-4o-mini", "display_name": "GPT-4o mini" }
        ]});
        let models = parse_remote_models(PROTOCOL_OPENAI, &value);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gpt-4o");
        assert_eq!(models[0].display_name, None);
        assert_eq!(models[1].display_name.as_deref(), Some("GPT-4o mini"));
    }

    #[test]
    fn parses_gemini_list_and_strips_prefix() {
        let value = json!({ "models": [
            { "name": "models/gemini-1.5-pro", "displayName": "Gemini 1.5 Pro" },
            { "name": "models/gemini-1.5-flash" }
        ]});
        let models = parse_remote_models(PROTOCOL_GEMINI, &value);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gemini-1.5-pro");
        assert_eq!(models[0].display_name.as_deref(), Some("Gemini 1.5 Pro"));
        assert_eq!(models[1].id, "gemini-1.5-flash");
    }

    #[test]
    fn tolerates_unexpected_shape() {
        let value = json!({ "error": "nope" });
        assert!(parse_remote_models(PROTOCOL_OPENAI, &value).is_empty());
    }
}
