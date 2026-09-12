use rusqlite::{params, Connection};

use super::models::{is_known_capability, Provider, ProviderInput, UpstreamModel, UpstreamModelInput};
use crate::error::AppError;

pub fn list_providers(conn: &Connection) -> Result<Vec<Provider>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM providers ORDER BY created_at ASC")?;
    let rows = stmt.query_map([], Provider::from_row)?;
    let mut providers = Vec::new();
    for row in rows {
        providers.push(row?);
    }
    Ok(providers)
}

pub fn get_provider(conn: &Connection, id: &str) -> Result<Option<Provider>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM providers WHERE id = ?1")?;
    let mut rows = stmt.query_map([id], Provider::from_row)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// 引用该提供商名下模型的路由别名，用于协议变更前的冲突检测。
fn routes_using_provider(conn: &Connection, provider_id: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT r.alias
           FROM routes r
           JOIN route_targets t   ON t.route_id = r.id
           JOIN upstream_models m ON m.id = t.upstream_model_id
          WHERE m.provider_id = ?1
          ORDER BY r.alias ASC",
    )?;
    let rows = stmt.query_map([provider_id], |row| row.get(0))?;
    let mut aliases = Vec::new();
    for row in rows {
        aliases.push(row?);
    }
    Ok(aliases)
}

pub fn save_provider(conn: &Connection, input: &ProviderInput) -> Result<Provider, AppError> {
    // 头规则与网关求值共用一套校验，非法配置在保存期就拒绝。
    crate::gateway::headers::validate_provider_header_rules(&input.header_rules)
        .map_err(AppError::message)?;
    let id = input
        .id
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    // 协议创建后锁定：若改协议会让既有路由的目标失去协议同构，则阻断并报出受影响的路由。
    if let Some(existing) = get_provider(conn, &id)? {
        if existing.protocol != input.protocol {
            let aliases = routes_using_provider(conn, &id)?;
            if !aliases.is_empty() {
                return Err(AppError::message(format!(
                    "协议创建后不可修改：路由 {} 仍引用该提供商的模型，请先移除或重建这些路由",
                    aliases.join("、")
                )));
            }
        }
    }
    let created_at = chrono::Utc::now().to_rfc3339();
    let extra_headers = serde_json::to_string(&input.extra_headers)?;
    let header_rules = serde_json::to_string(&input.header_rules)?;
    conn.execute(
        "INSERT INTO providers
            (id, name, base_url, api_key, auth_scheme, protocol, extra_headers, header_rules, icon, icon_tint, enabled, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            base_url = excluded.base_url,
            api_key = excluded.api_key,
            auth_scheme = excluded.auth_scheme,
            protocol = excluded.protocol,
            extra_headers = excluded.extra_headers,
            header_rules = excluded.header_rules,
            icon = excluded.icon,
            icon_tint = excluded.icon_tint,
            enabled = excluded.enabled",
        params![
            id,
            input.name,
            input.base_url,
            input.api_key,
            input.auth_scheme,
            input.protocol,
            extra_headers,
            header_rules,
            input.icon,
            input.icon_tint,
            input.enabled as i64,
            created_at,
        ],
    )?;
    get_provider(conn, &id)?.ok_or_else(|| AppError::Message("保存提供商失败".into()))
}

pub fn delete_provider(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM providers WHERE id = ?1", [id])?;
    Ok(())
}

pub fn list_upstream_models(conn: &Connection) -> Result<Vec<UpstreamModel>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM upstream_models ORDER BY display_name ASC")?;
    let rows = stmt.query_map([], UpstreamModel::from_row)?;
    let mut models = Vec::new();
    for row in rows {
        models.push(row?);
    }
    Ok(models)
}

/// 提供商名下首个启用的上游模型（按展示名排序），供连通性探测选样。
pub fn first_enabled_model(
    conn: &Connection,
    provider_id: &str,
) -> Result<Option<UpstreamModel>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT * FROM upstream_models
          WHERE provider_id = ?1 AND enabled = 1
          ORDER BY display_name ASC
          LIMIT 1",
    )?;
    let mut rows = stmt.query_map([provider_id], UpstreamModel::from_row)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// 所有启用上游模型的真实模型名（去重），供网关启动时后台预热 tokenizer。
pub fn list_enabled_model_ids(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt =
        conn.prepare("SELECT DISTINCT model_id FROM upstream_models WHERE enabled = 1")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row?);
    }
    Ok(ids)
}

pub fn save_upstream_model(
    conn: &Connection,
    input: &UpstreamModelInput,
) -> Result<UpstreamModel, AppError> {
    // 能力标签以 `MODEL_CAPABILITIES` 为权威词表，拒绝词表外的取值，避免脏数据落库。
    if let Some(value) = input
        .capabilities
        .iter()
        .find(|value| !is_known_capability(value))
    {
        return Err(AppError::message(format!("未知能力标签：{value}")));
    }
    let id = input
        .id
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let capabilities = serde_json::to_string(&input.capabilities)?;
    conn.execute(
        "INSERT INTO upstream_models
            (id, provider_id, model_id, display_name, input_price, output_price,
             cache_read_price, cache_creation_price, context_window, capabilities,
             icon, icon_tint, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO UPDATE SET
            provider_id = excluded.provider_id,
            model_id = excluded.model_id,
            display_name = excluded.display_name,
            input_price = excluded.input_price,
            output_price = excluded.output_price,
            cache_read_price = excluded.cache_read_price,
            cache_creation_price = excluded.cache_creation_price,
            context_window = excluded.context_window,
            capabilities = excluded.capabilities,
            icon = excluded.icon,
            icon_tint = excluded.icon_tint,
            enabled = excluded.enabled",
        params![
            id,
            input.provider_id,
            input.model_id,
            input.display_name,
            input.input_price,
            input.output_price,
            input.cache_read_price,
            input.cache_creation_price,
            input.context_window,
            capabilities,
            input.icon,
            input.icon_tint,
            input.enabled as i64,
        ],
    )?;
    get_upstream_model(conn, &id)?
        .ok_or_else(|| AppError::Message("保存上游模型失败".into()))
}

pub fn get_upstream_model(conn: &Connection, id: &str) -> Result<Option<UpstreamModel>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM upstream_models WHERE id = ?1")?;
    let mut rows = stmt.query_map([id], UpstreamModel::from_row)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn delete_upstream_model(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM upstream_models WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{HeaderReplace, ProviderHeaderRules, RouteInput, RouteTargetInput};
    use crate::db::open_in_memory;
    use crate::db::routes::save_route;
    use std::collections::BTreeMap;

    fn seed_provider(conn: &Connection, protocol: &str) -> Provider {
        save_provider(
            conn,
            &ProviderInput {
                id: None,
                name: format!("p-{protocol}"),
                base_url: "https://example.com/v1".into(),
                api_key: "secret".into(),
                auth_scheme: "bearer".into(),
                protocol: protocol.into(),
                extra_headers: BTreeMap::new(),
                header_rules: Default::default(),
                icon: None,
                icon_tint: "ink".into(),
                enabled: true,
            },
        )
        .unwrap()
    }

    fn change_protocol(conn: &Connection, provider: &Provider, protocol: &str) -> Result<Provider, AppError> {
        save_provider(
            conn,
            &ProviderInput {
                id: Some(provider.id.clone()),
                name: provider.name.clone(),
                base_url: provider.base_url.clone(),
                api_key: provider.api_key.clone(),
                auth_scheme: provider.auth_scheme.clone(),
                protocol: protocol.into(),
                extra_headers: BTreeMap::new(),
                header_rules: Default::default(),
                icon: None,
                icon_tint: "ink".into(),
                enabled: true,
            },
        )
    }

    #[test]
    fn rejects_protocol_change_when_routes_reference_provider() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = seed_provider(&conn, "openai");
        let model = save_upstream_model(
            &conn,
            &UpstreamModelInput {
                id: None,
                provider_id: provider.id.clone(),
                model_id: "gpt".into(),
                display_name: "GPT".into(),
                input_price: 0.0,
                output_price: 0.0,
                cache_read_price: 0.0,
                cache_creation_price: 0.0,
                context_window: 0,
                capabilities: Vec::new(),
                icon: None,
                icon_tint: "ink".into(),
                enabled: true,
            },
        )
        .unwrap();
        save_route(
            &conn,
            &RouteInput {
                id: None,
                alias: "r".into(),
                display_name: "R".into(),
                protocol: "openai".into(),
                icon: None,
                icon_tint: None,
                enabled: true,
                targets: vec![RouteTargetInput {
                    upstream_model_id: model.id,
                    priority: 0,
                    enabled: true,
                }],
            },
        )
        .unwrap();

        assert!(change_protocol(&conn, &provider, "anthropic").is_err());
    }

    #[test]
    fn allows_protocol_change_when_no_routes_reference_provider() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = seed_provider(&conn, "openai");
        let saved = change_protocol(&conn, &provider, "anthropic").unwrap();
        assert_eq!(saved.protocol, "anthropic");
    }

    fn model_input(provider_id: &str, capabilities: Vec<&str>) -> UpstreamModelInput {
        UpstreamModelInput {
            id: None,
            provider_id: provider_id.to_string(),
            model_id: "m".into(),
            display_name: "M".into(),
            input_price: 0.0,
            output_price: 0.0,
            cache_read_price: 0.0,
            cache_creation_price: 0.0,
            context_window: 0,
            capabilities: capabilities.into_iter().map(str::to_string).collect(),
            icon: None,
            icon_tint: "ink".into(),
            enabled: true,
        }
    }

    #[test]
    fn saves_and_reads_provider_header_rules() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = save_provider(
            &conn,
            &ProviderInput {
                id: None,
                name: "p".into(),
                base_url: "https://example.com/v1".into(),
                api_key: "secret".into(),
                auth_scheme: "bearer".into(),
                protocol: "openai".into(),
                extra_headers: BTreeMap::new(),
                header_rules: ProviderHeaderRules {
                    forward: vec!["session_id".into()],
                    replace: vec![HeaderReplace {
                        from: "session_id".into(),
                        to: "x-opencode-session".into(),
                    }],
                    remove: vec!["x-internal*".into()],
                },
                icon: None,
                icon_tint: "ink".into(),
                enabled: true,
            },
        )
        .unwrap();
        let saved = get_provider(&conn, &provider.id).unwrap().unwrap();
        assert_eq!(saved.header_rules, provider.header_rules);

        // 空规则落库为 `{}`。
        let empty = seed_provider(&conn, "anthropic");
        let raw: String = conn
            .query_row(
                "SELECT header_rules FROM providers WHERE id = ?1",
                [&empty.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(raw, "{}");
    }

    #[test]
    fn save_upstream_model_accepts_known_capabilities() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = seed_provider(&conn, "openai");
        let saved = save_upstream_model(
            &conn,
            &model_input(&provider.id, vec!["vision", "tools", "reasoning"]),
        )
        .unwrap();
        assert_eq!(saved.capabilities, vec!["vision", "tools", "reasoning"]);
    }

    #[test]
    fn save_upstream_model_rejects_unknown_capability() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = seed_provider(&conn, "openai");
        let error =
            save_upstream_model(&conn, &model_input(&provider.id, vec!["audio"])).unwrap_err();
        assert!(matches!(error, AppError::Message(_)), "got {error:?}");
    }
}
