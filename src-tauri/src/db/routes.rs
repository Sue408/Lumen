use rusqlite::{params, Connection, OptionalExtension};

use super::models::{is_known_protocol, Route, RouteInput, RouteTarget, RouteWithTargets};
use crate::error::AppError;

pub fn list_routes(conn: &Connection) -> Result<Vec<RouteWithTargets>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM routes ORDER BY created_at ASC")?;
    let rows = stmt.query_map([], Route::from_row)?;
    let mut routes = Vec::new();
    for row in rows {
        let route = row?;
        let targets = list_targets(conn, &route.id)?;
        routes.push(RouteWithTargets { route, targets });
    }
    Ok(routes)
}

pub fn get_route(conn: &Connection, id: &str) -> Result<Option<RouteWithTargets>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM routes WHERE id = ?1")?;
    let mut rows = stmt.query_map([id], Route::from_row)?;
    match rows.next() {
        Some(row) => {
            let route = row?;
            let targets = list_targets(conn, &route.id)?;
            Ok(Some(RouteWithTargets { route, targets }))
        }
        None => Ok(None),
    }
}

fn list_targets(conn: &Connection, route_id: &str) -> Result<Vec<RouteTarget>, AppError> {
    let mut stmt = conn
        .prepare("SELECT * FROM route_targets WHERE route_id = ?1 ORDER BY priority ASC, rowid ASC")?;
    let rows = stmt.query_map([route_id], RouteTarget::from_row)?;
    let mut targets = Vec::new();
    for row in rows {
        targets.push(row?);
    }
    Ok(targets)
}

/// 查上游模型所属提供商的协议，用于校验路由协议同构。
fn target_protocol(conn: &Connection, upstream_model_id: &str) -> Result<Option<String>, AppError> {
    conn.query_row(
        "SELECT p.protocol
           FROM upstream_models m
           JOIN providers p ON p.id = m.provider_id
          WHERE m.id = ?1",
        [upstream_model_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(AppError::from)
}

pub fn save_route(conn: &Connection, input: &RouteInput) -> Result<RouteWithTargets, AppError> {
    if !is_known_protocol(&input.protocol) {
        return Err(AppError::message(format!("未知协议：{}", input.protocol)));
    }
    // 协议创建后锁定：改协议等于换了对外契约，应删除重建而非原地修改。
    if let Some(existing_id) = input.id.as_deref().filter(|value| !value.is_empty()) {
        if let Some(existing) = get_route(conn, existing_id)? {
            if existing.route.protocol != input.protocol {
                return Err(AppError::message(
                    "协议创建后不可修改：请删除该路由后重建".to_string(),
                ));
            }
        }
    }
    // 一个路由只允许一种协议：所有目标的上游提供商协议必须与路由声明一致。
    for target in &input.targets {
        match target_protocol(conn, &target.upstream_model_id)? {
            Some(protocol) if protocol == input.protocol => {}
            Some(protocol) => {
                return Err(AppError::message(format!(
                    "路由协议与上游模型不一致：目标为 {protocol} 协议，路由声明为 {}",
                    input.protocol
                )))
            }
            None => {
                return Err(AppError::message(format!(
                    "上游模型不存在：{}",
                    target.upstream_model_id
                )))
            }
        }
    }

    let id = input
        .id
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let created_at = chrono::Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO routes (id, alias, display_name, protocol, enabled, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            alias = excluded.alias,
            display_name = excluded.display_name,
            protocol = excluded.protocol,
            enabled = excluded.enabled",
        params![
            id,
            input.alias,
            input.display_name,
            input.protocol,
            input.enabled as i64,
            created_at
        ],
    )
    .map_err(|error| {
        AppError::from_constraint(error, format!("路由别名已存在：{}", input.alias))
    })?;
    tx.execute("DELETE FROM route_targets WHERE route_id = ?1", [&id])?;
    for target in &input.targets {
        tx.execute(
            "INSERT INTO route_targets (id, route_id, upstream_model_id, priority, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                uuid::Uuid::new_v4().to_string(),
                id,
                target.upstream_model_id,
                target.priority,
                target.enabled as i64,
            ],
        )?;
    }
    tx.commit()?;
    get_route(conn, &id)?.ok_or_else(|| AppError::Message("保存路由失败".into()))
}

pub fn delete_route(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM routes WHERE id = ?1", [id])?;
    Ok(())
}

/// 返回启用中的别名及其展示名，供 `/v1/models` 使用。
pub fn list_enabled_aliases(conn: &Connection) -> Result<Vec<(String, String)>, AppError> {
    let mut stmt =
        conn.prepare("SELECT alias, display_name FROM routes WHERE enabled = 1 ORDER BY alias ASC")?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let mut aliases = Vec::new();
    for row in rows {
        aliases.push(row?);
    }
    Ok(aliases)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::db::models::{ProviderInput, RouteInput, RouteTargetInput, UpstreamModelInput};
    use crate::db::{open_in_memory, providers};

    fn seed_model(conn: &Connection, protocol: &str, model_id: &str) -> String {
        let provider = providers::save_provider(
            conn,
            &ProviderInput {
                id: None,
                name: format!("p-{protocol}-{model_id}"),
                base_url: "https://example.com/v1".into(),
                api_key: "secret".into(),
                auth_scheme: "bearer".into(),
                protocol: protocol.into(),
                extra_headers: BTreeMap::new(),
                icon: None,
                icon_tint: "ink".into(),
                enabled: true,
            },
        )
        .unwrap();
        providers::save_upstream_model(
            conn,
            &UpstreamModelInput {
                id: None,
                provider_id: provider.id,
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
    fn rejects_mixed_protocol_targets() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let openai = seed_model(&conn, "openai", "gpt");
        let anthropic = seed_model(&conn, "anthropic", "claude");
        let result = save_route(
            &conn,
            &RouteInput {
                id: None,
                alias: "mix".into(),
                display_name: "Mix".into(),
                protocol: "openai".into(),
                enabled: true,
                targets: vec![
                    RouteTargetInput {
                        upstream_model_id: openai,
                        priority: 0,
                        enabled: true,
                    },
                    RouteTargetInput {
                        upstream_model_id: anthropic,
                        priority: 1,
                        enabled: true,
                    },
                ],
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn persists_protocol_for_homogeneous_targets() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let anthropic = seed_model(&conn, "anthropic", "claude");
        let saved = save_route(
            &conn,
            &RouteInput {
                id: None,
                alias: "claude".into(),
                display_name: "Claude".into(),
                protocol: "anthropic".into(),
                enabled: true,
                targets: vec![RouteTargetInput {
                    upstream_model_id: anthropic,
                    priority: 0,
                    enabled: true,
                }],
            },
        )
        .unwrap();
        assert_eq!(saved.route.protocol, "anthropic");
    }

    #[test]
    fn rejects_protocol_change_on_existing_route() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let openai = seed_model(&conn, "openai", "gpt");
        let saved = save_route(
            &conn,
            &RouteInput {
                id: None,
                alias: "r".into(),
                display_name: "R".into(),
                protocol: "openai".into(),
                enabled: true,
                targets: vec![RouteTargetInput {
                    upstream_model_id: openai,
                    priority: 0,
                    enabled: true,
                }],
            },
        )
        .unwrap();

        let result = save_route(
            &conn,
            &RouteInput {
                id: Some(saved.route.id.clone()),
                alias: "r".into(),
                display_name: "R".into(),
                protocol: "anthropic".into(),
                enabled: true,
                targets: Vec::new(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn duplicate_alias_reports_a_friendly_error() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let model = seed_model(&conn, "openai", "gpt");
        let input = |alias: &str| RouteInput {
            id: None,
            alias: alias.into(),
            display_name: "R".into(),
            protocol: "openai".into(),
            enabled: true,
            targets: vec![RouteTargetInput {
                upstream_model_id: model.clone(),
                priority: 0,
                enabled: true,
            }],
        };
        save_route(&conn, &input("r")).unwrap();
        // 不同 id、相同 alias：命中 UNIQUE(alias)，应翻译成面向用户的提示。
        let error = save_route(&conn, &input("r")).unwrap_err();
        assert!(matches!(error, AppError::Message(_)), "got {error:?}");
    }
}
