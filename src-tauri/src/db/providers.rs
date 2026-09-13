use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};

use super::models::{
    is_known_capability, is_known_protocol, Provider, ProviderEndpoint, ProviderEndpointInput,
    ProviderInput, ProviderWithEndpoints, UpstreamModel, UpstreamModelInput,
};
use crate::error::AppError;

pub fn list_providers(conn: &Connection) -> Result<Vec<ProviderWithEndpoints>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM providers ORDER BY created_at ASC")?;
    let rows = stmt.query_map([], Provider::from_row)?;
    let mut providers = Vec::new();
    for row in rows {
        let provider = row?;
        let endpoints = list_endpoints(conn, &provider.id)?;
        providers.push(ProviderWithEndpoints { provider, endpoints });
    }
    Ok(providers)
}

pub fn get_provider(
    conn: &Connection,
    id: &str,
) -> Result<Option<ProviderWithEndpoints>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM providers WHERE id = ?1")?;
    let mut rows = stmt.query_map([id], Provider::from_row)?;
    match rows.next() {
        Some(row) => {
            let provider = row?;
            let endpoints = list_endpoints(conn, &provider.id)?;
            Ok(Some(ProviderWithEndpoints { provider, endpoints }))
        }
        None => Ok(None),
    }
}

/// 某提供商的全部协议端点，按插入顺序返回。
pub fn list_endpoints(
    conn: &Connection,
    provider_id: &str,
) -> Result<Vec<ProviderEndpoint>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT * FROM provider_endpoints WHERE provider_id = ?1 ORDER BY rowid ASC",
    )?;
    let rows = stmt.query_map([provider_id], ProviderEndpoint::from_row)?;
    let mut endpoints = Vec::new();
    for row in rows {
        endpoints.push(row?);
    }
    Ok(endpoints)
}

/// 引用该提供商、且路由协议为 `protocol` 的路由别名。用于端点移除前的冲突检测。
fn routes_using_protocol(
    conn: &Connection,
    provider_id: &str,
    protocol: &str,
) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT r.alias
           FROM routes r
           JOIN route_targets t   ON t.route_id = r.id
           JOIN upstream_models m ON m.id = t.upstream_model_id
          WHERE m.provider_id = ?1 AND r.protocol = ?2
          ORDER BY r.alias ASC",
    )?;
    let rows = stmt.query_map(params![provider_id, protocol], |row| row.get(0))?;
    let mut aliases = Vec::new();
    for row in rows {
        aliases.push(row?);
    }
    Ok(aliases)
}

/// 校验端点集合：至少一个、协议合法且不重复、上游地址非空。
fn validate_endpoints(endpoints: &[ProviderEndpointInput]) -> Result<(), AppError> {
    if endpoints.is_empty() {
        return Err(AppError::message("提供商至少需要一个协议端点"));
    }
    let mut seen = HashSet::new();
    for endpoint in endpoints {
        if !is_known_protocol(&endpoint.protocol) {
            return Err(AppError::message(format!("未知协议：{}", endpoint.protocol)));
        }
        if !seen.insert(endpoint.protocol.as_str()) {
            return Err(AppError::message(format!(
                "协议端点重复：{}",
                endpoint.protocol
            )));
        }
        if endpoint.base_url.trim().is_empty() {
            return Err(AppError::message(format!(
                "协议 {} 缺少上游地址",
                endpoint.protocol
            )));
        }
    }
    Ok(())
}

pub fn save_provider(
    conn: &Connection,
    input: &ProviderInput,
) -> Result<ProviderWithEndpoints, AppError> {
    // 头规则与网关求值共用一套校验，非法配置在保存期就拒绝。
    crate::gateway::headers::validate_provider_header_rules(&input.header_rules)
        .map_err(AppError::message)?;
    validate_endpoints(&input.endpoints)?;

    let id = input
        .id
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // 端点是路由的协议依赖：被移除的协议若仍被路由引用，则阻断并报出受影响的路由。
    if let Some(existing) = get_provider(conn, &id)? {
        let existing_protocols: HashSet<&str> = existing
            .endpoints
            .iter()
            .map(|endpoint| endpoint.protocol.as_str())
            .collect();
        let new_protocols: HashSet<&str> = input
            .endpoints
            .iter()
            .map(|endpoint| endpoint.protocol.as_str())
            .collect();
        for removed in existing_protocols.difference(&new_protocols) {
            let aliases = routes_using_protocol(conn, &id, removed)?;
            if !aliases.is_empty() {
                return Err(AppError::message(format!(
                    "{removed} 协议端点仍被路由引用，无法移除：{}",
                    aliases.join("、")
                )));
            }
        }
    }

    let created_at = chrono::Utc::now().to_rfc3339();
    let extra_headers = serde_json::to_string(&input.extra_headers)?;
    let header_rules = serde_json::to_string(&input.header_rules)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO providers
            (id, name, api_key, extra_headers, header_rules, icon, icon_tint, enabled, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            api_key = excluded.api_key,
            extra_headers = excluded.extra_headers,
            header_rules = excluded.header_rules,
            icon = excluded.icon,
            icon_tint = excluded.icon_tint,
            enabled = excluded.enabled",
        params![
            id,
            input.name,
            input.api_key,
            extra_headers,
            header_rules,
            input.icon,
            input.icon_tint,
            input.enabled as i64,
            created_at,
        ],
    )?;
    // 端点按自然键 `(provider_id, protocol)` upsert，使既有端点 id 保持稳定。
    let mut existing: HashMap<String, String> = HashMap::new();
    {
        let mut stmt =
            tx.prepare("SELECT id, protocol FROM provider_endpoints WHERE provider_id = ?1")?;
        let rows = stmt.query_map([&id], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, String>(0)?))
        })?;
        for row in rows {
            let (protocol, endpoint_id) = row?;
            existing.insert(protocol, endpoint_id);
        }
    }
    for endpoint in &input.endpoints {
        match existing.remove(&endpoint.protocol) {
            Some(endpoint_id) => {
                tx.execute(
                    "UPDATE provider_endpoints
                        SET base_url = ?2, auth_scheme = ?3
                      WHERE id = ?1",
                    params![endpoint_id, endpoint.base_url, endpoint.auth_scheme],
                )?;
            }
            None => {
                tx.execute(
                    "INSERT INTO provider_endpoints
                        (id, provider_id, protocol, base_url, auth_scheme)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        uuid::Uuid::new_v4().to_string(),
                        id,
                        endpoint.protocol,
                        endpoint.base_url,
                        endpoint.auth_scheme,
                    ],
                )?;
            }
        }
    }
    // 输入中未出现的旧端点即被移除。
    for endpoint_id in existing.values() {
        tx.execute("DELETE FROM provider_endpoints WHERE id = ?1", [endpoint_id])?;
    }
    tx.commit()?;
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

    fn endpoint(protocol: &str, base_url: &str) -> ProviderEndpointInput {
        ProviderEndpointInput {
            id: None,
            protocol: protocol.into(),
            base_url: base_url.into(),
            auth_scheme: "bearer".into(),
        }
    }

    fn provider_input(name: &str, endpoints: Vec<ProviderEndpointInput>) -> ProviderInput {
        ProviderInput {
            id: None,
            name: name.into(),
            api_key: "secret".into(),
            endpoints,
            extra_headers: BTreeMap::new(),
            header_rules: Default::default(),
            icon: None,
            icon_tint: "ink".into(),
            enabled: true,
        }
    }

    fn seed_provider(conn: &Connection, protocol: &str) -> ProviderWithEndpoints {
        save_provider(
            conn,
            &provider_input(
                &format!("p-{protocol}"),
                vec![endpoint(protocol, "https://example.com/v1")],
            ),
        )
        .unwrap()
    }

    fn seed_model(conn: &Connection, provider_id: &str, model_id: &str) -> String {
        save_upstream_model(
            conn,
            &UpstreamModelInput {
                id: None,
                provider_id: provider_id.to_string(),
                model_id: model_id.into(),
                display_name: model_id.into(),
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
        .unwrap()
        .id
    }

    #[test]
    fn saves_multiple_protocol_endpoints() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let saved = save_provider(
            &conn,
            &provider_input(
                "opencode-go",
                vec![
                    endpoint("openai", "https://go.example.com/v1"),
                    endpoint("anthropic", "https://go.example.com/anthropic/v1"),
                    endpoint("responses", "https://go.example.com/v1"),
                ],
            ),
        )
        .unwrap();
        assert_eq!(saved.endpoints.len(), 3);
        let protocols: Vec<&str> = saved
            .endpoints
            .iter()
            .map(|item| item.protocol.as_str())
            .collect();
        assert!(protocols.contains(&"openai"));
        assert!(protocols.contains(&"anthropic"));
        assert!(protocols.contains(&"responses"));
    }

    #[test]
    fn rejects_duplicate_protocol_endpoints() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let result = save_provider(
            &conn,
            &provider_input(
                "dup",
                vec![
                    endpoint("openai", "https://a/v1"),
                    endpoint("openai", "https://b/v1"),
                ],
            ),
        );
        assert!(result.is_err());
    }

    #[test]
    fn rejects_unknown_protocol_endpoint() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let result = save_provider(&conn, &provider_input("x", vec![endpoint("cohere", "https://a")]));
        assert!(result.is_err());
    }

    #[test]
    fn rejects_empty_endpoints() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        assert!(save_provider(&conn, &provider_input("empty", Vec::new())).is_err());
    }

    #[test]
    fn rejects_removing_protocol_referenced_by_route() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = save_provider(
            &conn,
            &provider_input(
                "multi",
                vec![
                    endpoint("openai", "https://a/v1"),
                    endpoint("anthropic", "https://a/anthropic/v1"),
                ],
            ),
        )
        .unwrap();
        let model = seed_model(&conn, &provider.provider.id, "m");
        save_route(
            &conn,
            &RouteInput {
                id: None,
                alias: "r".into(),
                display_name: "R".into(),
                protocol: "anthropic".into(),
                icon: None,
                icon_tint: None,
                enabled: true,
                targets: vec![RouteTargetInput {
                    upstream_model_id: model,
                    priority: 0,
                    enabled: true,
                }],
            },
        )
        .unwrap();

        // 移除被 anthropic 路由引用的端点 → 拒绝。
        let blocked = save_provider(
            &conn,
            &ProviderInput {
                id: Some(provider.provider.id.clone()),
                endpoints: vec![endpoint("openai", "https://a/v1")],
                ..provider_input("multi", Vec::new())
            },
        );
        assert!(blocked.is_err());

        // 保留 anthropic 端点即可保存。
        let ok = save_provider(
            &conn,
            &ProviderInput {
                id: Some(provider.provider.id.clone()),
                endpoints: vec![
                    endpoint("openai", "https://a/v1"),
                    endpoint("anthropic", "https://a/anthropic/v1"),
                ],
                ..provider_input("multi", Vec::new())
            },
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn endpoint_ids_are_stable_across_resaves() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let saved = save_provider(
            &conn,
            &provider_input(
                "stable",
                vec![
                    endpoint("openai", "https://a/v1"),
                    endpoint("anthropic", "https://a/anthropic/v1"),
                ],
            ),
        )
        .unwrap();
        let before: HashMap<String, String> = saved
            .endpoints
            .iter()
            .map(|item| (item.protocol.clone(), item.id.clone()))
            .collect();

        let resaved = save_provider(
            &conn,
            &ProviderInput {
                id: Some(saved.provider.id.clone()),
                endpoints: vec![
                    // 顺序调换、改地址，协议集合不变。
                    endpoint("anthropic", "https://a/anthropic/v2"),
                    endpoint("openai", "https://a/v2"),
                ],
                ..provider_input("stable", Vec::new())
            },
        )
        .unwrap();
        for item in &resaved.endpoints {
            assert_eq!(
                before.get(&item.protocol),
                Some(&item.id),
                "端点 id 应保持不变"
            );
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
                api_key: "secret".into(),
                endpoints: vec![endpoint("openai", "https://example.com/v1")],
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
        let saved = get_provider(&conn, &provider.provider.id).unwrap().unwrap();
        assert_eq!(saved.provider.header_rules, provider.provider.header_rules);

        // 空规则落库为 `{}`。
        save_provider(
            &conn,
            &provider_input("anthropic", vec![endpoint("anthropic", "https://a/v1")]),
        )
        .unwrap();
        let raw: String = conn
            .query_row(
                "SELECT header_rules FROM providers WHERE name = 'anthropic'",
                [],
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
            &UpstreamModelInput {
                id: None,
                provider_id: provider.provider.id.clone(),
                model_id: "m".into(),
                display_name: "M".into(),
                input_price: 0.0,
                output_price: 0.0,
                cache_read_price: 0.0,
                cache_creation_price: 0.0,
                context_window: 0,
                capabilities: vec!["vision".into(), "tools".into(), "reasoning".into()],
                icon: None,
                icon_tint: "ink".into(),
                enabled: true,
            },
        )
        .unwrap();
        assert_eq!(saved.capabilities, vec!["vision", "tools", "reasoning"]);
    }

    #[test]
    fn save_upstream_model_rejects_unknown_capability() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = seed_provider(&conn, "openai");
        let error = save_upstream_model(
            &conn,
            &UpstreamModelInput {
                id: None,
                provider_id: provider.provider.id.clone(),
                model_id: "m".into(),
                display_name: "M".into(),
                input_price: 0.0,
                output_price: 0.0,
                cache_read_price: 0.0,
                cache_creation_price: 0.0,
                context_window: 0,
                capabilities: vec!["audio".into()],
                icon: None,
                icon_tint: "ink".into(),
                enabled: true,
            },
        )
        .unwrap_err();
        assert!(matches!(error, AppError::Message(_)), "got {error:?}");
    }
}
