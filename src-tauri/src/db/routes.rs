use std::collections::HashMap;

use rusqlite::{params, Connection};

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

/// 上游模型是否存在，以及其提供商是否提供任一端点。
/// 返回 `(模型是否存在, 是否有任一端点)`。
fn target_endpoint_status(
    conn: &Connection,
    upstream_model_id: &str,
) -> Result<(bool, bool), AppError> {
    let (exists, has_endpoint): (i64, i64) = conn.query_row(
        "SELECT
            EXISTS(SELECT 1 FROM upstream_models WHERE id = ?1),
            EXISTS(
                SELECT 1
                  FROM upstream_models m
                  JOIN provider_endpoints e ON e.provider_id = m.provider_id
                 WHERE m.id = ?1
            )",
        [upstream_model_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok((exists != 0, has_endpoint != 0))
}

pub fn save_route(conn: &Connection, input: &RouteInput) -> Result<RouteWithTargets, AppError> {
    if !is_known_protocol(&input.protocol) {
        return Err(AppError::message(format!("未知协议：{}", input.protocol)));
    }
    // 启用的路由至少需要一个启用中的目标，否则运行时永远解析为 404。
    if input.enabled && !input.targets.iter().any(|target| target.enabled) {
        return Err(AppError::message("启用的路由至少需要一个启用中的目标"));
    }
    // 同一路由内同一上游模型只能出现一次（也是 route_targets 的唯一索引）。
    let mut seen_models = std::collections::HashSet::new();
    for target in &input.targets {
        if !seen_models.insert(target.upstream_model_id.as_str()) {
            return Err(AppError::message(format!(
                "路由目标重复：{}",
                target.upstream_model_id
            )));
        }
    }
    // 协议仅作兼容展示，不再约束目标；改协议不会破坏对外契约。
    // 每个目标的上游提供商至少要有一个协议端点，否则入站解析时永远无候选。
    for target in &input.targets {
        let (exists, has_endpoint) = target_endpoint_status(conn, &target.upstream_model_id)?;
        if !exists {
            return Err(AppError::message(format!(
                "上游模型不存在：{}",
                target.upstream_model_id
            )));
        }
        if !has_endpoint {
            return Err(AppError::message(format!(
                "目标提供商没有任何协议端点：{}",
                target.upstream_model_id
            )));
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
        "INSERT INTO routes (id, alias, display_name, protocol, icon, icon_tint, enabled, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
            alias = excluded.alias,
            display_name = excluded.display_name,
            protocol = excluded.protocol,
            icon = excluded.icon,
            icon_tint = excluded.icon_tint,
            enabled = excluded.enabled",
        params![
            id,
            input.alias,
            input.display_name,
            input.protocol,
            input.icon,
            input.icon_tint,
            input.enabled as i64,
            created_at
        ],
    )
    .map_err(|error| {
        AppError::from_constraint(error, format!("路由别名已存在：{}", input.alias))
    })?;
    // 按自然键 `(route_id, upstream_model_id)` upsert，使已有目标的 id 保持不变：
    // 未来日志可引用实际履约的 target，而不会被「全删全插」打乱。
    let mut existing: HashMap<String, String> = HashMap::new();
    {
        let mut stmt =
            tx.prepare("SELECT id, upstream_model_id FROM route_targets WHERE route_id = ?1")?;
        let rows = stmt.query_map([&id], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, String>(0)?))
        })?;
        for row in rows {
            let (model_id, target_id) = row?;
            existing.insert(model_id, target_id);
        }
    }
    for target in &input.targets {
        match existing.remove(&target.upstream_model_id) {
            Some(target_id) => {
                tx.execute(
                    "UPDATE route_targets
                        SET priority = ?2, enabled = ?3
                      WHERE id = ?1",
                    params![target_id, target.priority, target.enabled as i64],
                )?;
            }
            None => {
                tx.execute(
                    "INSERT INTO route_targets
                        (id, route_id, upstream_model_id, priority, enabled)
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
        }
    }
    // 输入中未出现的旧目标即被移除。
    for target_id in existing.values() {
        tx.execute("DELETE FROM route_targets WHERE id = ?1", [target_id])?;
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

/// 别名是否存在且路由已启用。网关用它区分两种同为 404 的空候选：
/// 「别名根本不存在」与「别名在、但目标 / 模型 / 端点全不可用」。
pub fn enabled_alias_exists(conn: &Connection, alias: &str) -> Result<bool, AppError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM routes WHERE alias = ?1 AND enabled = 1",
        [alias],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::db::models::{
        ProviderEndpointInput, ProviderInput, RouteInput, RouteTargetInput, UpstreamModelInput,
    };
    use crate::db::{open_in_memory, providers};

    fn seed_model(conn: &Connection, protocol: &str, model_id: &str) -> String {
        let provider = providers::save_provider(
            conn,
            &ProviderInput {
                id: None,
                name: format!("p-{protocol}-{model_id}"),
                api_key: "secret".into(),
                endpoints: vec![ProviderEndpointInput {
                    id: None,
                    protocol: protocol.into(),
                    base_url: "https://example.com/v1".into(),
                    auth_scheme: "bearer".into(),
                }],
                extra_headers: BTreeMap::new(),
                header_rules: Default::default(),
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
                provider_id: provider.provider.id,
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
    fn accepts_targets_across_protocols() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let openai = seed_model(&conn, "openai", "gpt");
        let anthropic = seed_model(&conn, "anthropic", "claude");
        // 隐式多协议后，同一路由可挂不同协议端点的上游目标。
        let result = save_route(
            &conn,
            &RouteInput {
                id: None,
                alias: "mix".into(),
                display_name: "Mix".into(),
                protocol: "openai".into(),
                icon: None,
                icon_tint: None,
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
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_target_without_any_endpoint() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let model = seed_model(&conn, "openai", "gpt");
        // 端点被全部删除（等价于该提供商不再可用）→ 拒绝挂上。
        conn.execute("DELETE FROM provider_endpoints", []).unwrap();
        let result = save_route(
            &conn,
            &RouteInput {
                id: None,
                alias: "no-endpoint".into(),
                display_name: "N".into(),
                protocol: "openai".into(),
                icon: None,
                icon_tint: None,
                enabled: true,
                targets: vec![RouteTargetInput {
                    upstream_model_id: model,
                    priority: 0,
                    enabled: true,
                }],
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn multi_protocol_provider_serves_each_route_protocol() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        // 同一 provider、同一模型，挂 openai + anthropic 两个端点。
        let provider = providers::save_provider(
            &conn,
            &ProviderInput {
                id: None,
                name: "multi".into(),
                api_key: "secret".into(),
                endpoints: vec![
                    ProviderEndpointInput {
                        id: None,
                        protocol: "openai".into(),
                        base_url: "https://a/v1".into(),
                        auth_scheme: "bearer".into(),
                    },
                    ProviderEndpointInput {
                        id: None,
                        protocol: "anthropic".into(),
                        base_url: "https://a/anthropic/v1".into(),
                        auth_scheme: "x-api-key".into(),
                    },
                ],
                extra_headers: BTreeMap::new(),
                header_rules: Default::default(),
                icon: None,
                icon_tint: "ink".into(),
                enabled: true,
            },
        )
        .unwrap();
        let model = providers::save_upstream_model(
            &conn,
            &UpstreamModelInput {
                id: None,
                provider_id: provider.provider.id.clone(),
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
        .unwrap()
        .id;

        for protocol in ["openai", "anthropic"] {
            let route = save_route(
                &conn,
                &RouteInput {
                    id: None,
                    alias: format!("r-{protocol}"),
                    display_name: "R".into(),
                    protocol: protocol.into(),
                    icon: None,
                    icon_tint: None,
                    enabled: true,
                    targets: vec![RouteTargetInput {
                        upstream_model_id: model.clone(),
                        priority: 0,
                        enabled: true,
                    }],
                },
            );
            assert!(route.is_ok(), "多协议 provider 应服务 {protocol} 路由");
        }
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
                icon: None,
                icon_tint: None,
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
    fn save_route_persists_icon_overrides() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let model = seed_model(&conn, "openai", "gpt");
        let saved = save_route(
            &conn,
            &RouteInput {
                id: None,
                alias: "icon".into(),
                display_name: "Icon".into(),
                protocol: "openai".into(),
                icon: Some("openai".into()),
                icon_tint: Some("brand".into()),
                enabled: true,
                targets: vec![RouteTargetInput {
                    upstream_model_id: model,
                    priority: 0,
                    enabled: true,
                }],
            },
        )
        .unwrap();
        assert_eq!(saved.route.icon.as_deref(), Some("openai"));
        assert_eq!(saved.route.icon_tint.as_deref(), Some("brand"));

        // 显式传 None 即清回「自动推断」（继承首选目标的上游模型）。
        let cleared = save_route(
            &conn,
            &RouteInput {
                id: Some(saved.route.id.clone()),
                alias: "icon".into(),
                display_name: "Icon".into(),
                protocol: "openai".into(),
                icon: None,
                icon_tint: None,
                enabled: true,
                targets: vec![RouteTargetInput {
                    upstream_model_id: saved.targets[0].upstream_model_id.clone(),
                    priority: 0,
                    enabled: true,
                }],
            },
        )
        .unwrap();
        assert!(cleared.route.icon.is_none());
        assert!(cleared.route.icon_tint.is_none());
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
            icon: None,
            icon_tint: None,
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

    #[test]
    fn rejects_enabled_route_without_usable_target() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let model = seed_model(&conn, "openai", "gpt");
        let route = |alias: &str, enabled: bool, targets: Vec<RouteTargetInput>| RouteInput {
            id: None,
            alias: alias.into(),
            display_name: "R".into(),
            protocol: "openai".into(),
            icon: None,
            icon_tint: None,
            enabled,
            targets,
        };

        assert!(save_route(&conn, &route("empty", true, Vec::new())).is_err());
        assert!(save_route(
            &conn,
            &route(
                "disabled-target",
                true,
                vec![RouteTargetInput {
                    upstream_model_id: model.clone(),
                    priority: 0,
                    enabled: false,
                }],
            ),
        )
        .is_err());
        // 停用的路由允许暂不挂目标。
        assert!(save_route(&conn, &route("draft", false, Vec::new())).is_ok());
    }

    fn route_with(targets: Vec<RouteTargetInput>) -> RouteInput {
        RouteInput {
            id: None,
            alias: "r".into(),
            display_name: "R".into(),
            protocol: "openai".into(),
            icon: None,
            icon_tint: None,
            enabled: true,
            targets,
        }
    }

    #[test]
    fn save_route_preserves_target_ids_across_resaves() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let a = seed_model(&conn, "openai", "a");
        let b = seed_model(&conn, "openai", "b");
        let saved = save_route(
            &conn,
            &route_with(
                vec![
                    RouteTargetInput {
                        upstream_model_id: a.clone(),
                        priority: 0,
                        enabled: true,
                    },
                    RouteTargetInput {
                        upstream_model_id: b.clone(),
                        priority: 1,
                        enabled: true,
                    },
                ],
            ),
        )
        .unwrap();
        let ids_before: Vec<(String, String)> = saved
            .targets
            .iter()
            .map(|t| (t.upstream_model_id.clone(), t.id.clone()))
            .collect();

        let resaved = save_route(
            &conn,
            &RouteInput {
                id: Some(saved.route.id.clone()),
                alias: "r".into(),
                display_name: "R".into(),
                protocol: "openai".into(),
                icon: None,
                icon_tint: None,
                enabled: true,
                targets: vec![
                    RouteTargetInput {
                        upstream_model_id: a.clone(),
                        priority: 3,
                        enabled: false,
                    },
                    RouteTargetInput {
                        upstream_model_id: b.clone(),
                        priority: 0,
                        enabled: true,
                    },
                ],
            },
        )
        .unwrap();

        for (model_id, target_id) in &ids_before {
            let now = resaved
                .targets
                .iter()
                .find(|t| &t.upstream_model_id == model_id)
                .unwrap();
            assert_eq!(&now.id, target_id, "target id 应保持不变");
        }
        let a_now = resaved
            .targets
            .iter()
            .find(|t| t.upstream_model_id == a)
            .unwrap();
        assert_eq!(a_now.priority, 3);
        assert!(!a_now.enabled);
    }

    #[test]
    fn save_route_removes_dropped_targets() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let a = seed_model(&conn, "openai", "a");
        let b = seed_model(&conn, "openai", "b");
        let saved = save_route(
            &conn,
            &route_with(
                vec![
                    RouteTargetInput {
                        upstream_model_id: a.clone(),
                        priority: 0,
                        enabled: true,
                    },
                    RouteTargetInput {
                        upstream_model_id: b.clone(),
                        priority: 1,
                        enabled: true,
                    },
                ],
            ),
        )
        .unwrap();

        let resaved = save_route(
            &conn,
            &RouteInput {
                id: Some(saved.route.id.clone()),
                alias: "r".into(),
                display_name: "R".into(),
                protocol: "openai".into(),
                icon: None,
                icon_tint: None,
                enabled: true,
                targets: vec![RouteTargetInput {
                    upstream_model_id: a.clone(),
                    priority: 0,
                    enabled: true,
                }],
            },
        )
        .unwrap();

        assert_eq!(resaved.targets.len(), 1);
        assert_eq!(resaved.targets[0].upstream_model_id, a);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM route_targets WHERE route_id = ?1",
                [&saved.route.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn save_route_rejects_duplicate_target_model() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let a = seed_model(&conn, "openai", "a");
        let result = save_route(
            &conn,
            &route_with(
                vec![
                    RouteTargetInput {
                        upstream_model_id: a.clone(),
                        priority: 0,
                        enabled: true,
                    },
                    RouteTargetInput {
                        upstream_model_id: a.clone(),
                        priority: 1,
                        enabled: true,
                    },
                ],
            ),
        );
        assert!(result.is_err());
        let routes: i64 = conn
            .query_row("SELECT COUNT(*) FROM routes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(routes, 0, "校验失败不应留下路由");
    }

    #[test]
    fn target_id_survives_priority_reorder() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let a = seed_model(&conn, "openai", "a");
        let b = seed_model(&conn, "openai", "b");
        let target = |model: &str, priority: i64| RouteTargetInput {
            upstream_model_id: model.to_string(),
            priority,
            enabled: true,
        };
        let saved = save_route(
            &conn,
            &route_with(vec![target(&a, 0), target(&b, 1)]),
        )
        .unwrap();
        let id_of = |route: &crate::db::models::RouteWithTargets, model: &str| {
            route
                .targets
                .iter()
                .find(|t| t.upstream_model_id == model)
                .unwrap()
                .id
                .clone()
        };
        let a_id = id_of(&saved, &a);
        let b_id = id_of(&saved, &b);

        let reordered = save_route(
            &conn,
            &RouteInput {
                id: Some(saved.route.id.clone()),
                alias: "r".into(),
                display_name: "R".into(),
                protocol: "openai".into(),
                icon: None,
                icon_tint: None,
                enabled: true,
                targets: vec![target(&b, 0), target(&a, 1)],
            },
        )
        .unwrap();
        assert_eq!(id_of(&reordered, &a), a_id);
        assert_eq!(id_of(&reordered, &b), b_id);
    }

}
