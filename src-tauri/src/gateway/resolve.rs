use rusqlite::Connection;
use std::collections::BTreeMap;

use crate::db::models::{parse_provider_header_rules, ProviderHeaderRules};
use crate::db::with_db;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct ResolvedRoute {
    pub route_id: String,
    pub upstream_model_id: String,
    pub model_id: String,
    pub display_name: String,
    pub input_price: f64,
    pub output_price: f64,
    pub cache_read_price: f64,
    pub cache_creation_price: f64,
    pub provider_id: String,
    /// 命中协议端点的上游地址。
    pub base_url: String,
    pub api_key: String,
    /// 命中协议端点的鉴权方式。
    pub auth_scheme: String,
    /// 路由声明的入站协议（端点必须与之匹配）。
    pub route_protocol: String,
    /// 上游协议。同协议透传，恒等于 `route_protocol`；决定转发路径与计费口径。
    pub upstream_protocol: String,
    pub extra_headers: BTreeMap<String, String>,
    /// 上游 provider 的请求头映射（透传 / 替换 / 移除）；与 `extra_headers` 同为 provider 级。
    pub header_rules: ProviderHeaderRules,
}

/// 解析别名对应的**全部启用候选**，按 `priority` 升序（同级按插入顺序）。
/// 降级链即由此顺序决定；调用方按序尝试。
pub fn resolve_candidates(
    conn: &Connection,
    alias: &str,
) -> Result<Vec<ResolvedRoute>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT
            r.id            AS route_id,
            r.protocol      AS route_protocol,
            m.id            AS upstream_model_id,
            m.model_id      AS model_id,
            m.display_name  AS display_name,
            m.input_price   AS input_price,
            m.output_price  AS output_price,
            m.cache_read_price AS cache_read_price,
            m.cache_creation_price AS cache_creation_price,
            p.id            AS provider_id,
            e.base_url      AS base_url,
            p.api_key       AS api_key,
            e.auth_scheme   AS auth_scheme,
            r.protocol      AS upstream_protocol,
            p.extra_headers AS extra_headers,
            p.header_rules  AS header_rules
         FROM routes r
         JOIN route_targets t   ON t.route_id = r.id
         JOIN upstream_models m ON m.id = t.upstream_model_id
         JOIN providers p       ON p.id = m.provider_id
         JOIN provider_endpoints e
           ON e.provider_id = p.id
          AND e.protocol = r.protocol
         WHERE r.alias = ?1
           AND r.enabled = 1
           AND t.enabled = 1
           AND m.enabled = 1
           AND p.enabled = 1
         ORDER BY t.priority ASC, t.rowid ASC",
    )?;
    let rows = stmt.query_map([alias], |row| {
        let raw_headers: String = row.get("extra_headers")?;
        Ok(ResolvedRoute {
            route_id: row.get("route_id")?,
            route_protocol: row.get("route_protocol")?,
            upstream_model_id: row.get("upstream_model_id")?,
            model_id: row.get("model_id")?,
            display_name: row.get("display_name")?,
            input_price: row.get("input_price")?,
            output_price: row.get("output_price")?,
            cache_read_price: row.get("cache_read_price")?,
            cache_creation_price: row.get("cache_creation_price")?,
            provider_id: row.get("provider_id")?,
            base_url: row.get("base_url")?,
            api_key: row.get("api_key")?,
            auth_scheme: row.get("auth_scheme")?,
            upstream_protocol: row.get("upstream_protocol")?,
            extra_headers: serde_json::from_str(&raw_headers).unwrap_or_default(),
            header_rules: parse_provider_header_rules(&row.get::<_, String>("header_rules")?),
        })
    })?;
    let mut candidates = Vec::new();
    for row in rows {
        candidates.push(row?);
    }
    Ok(candidates)
}

/// 解析别名的全部有序候选，供降级链按序尝试。
pub async fn resolve_all(state: &AppState, alias: &str) -> Result<Vec<ResolvedRoute>, AppError> {
    let alias = alias.to_string();
    with_db(&state.db, move |conn| resolve_candidates(conn, &alias)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{
        ProviderEndpointInput, ProviderInput, RouteInput, RouteTargetInput, UpstreamModelInput,
    };
    use crate::db::{open_in_memory, providers, routes, Db};

    fn seed_provider(conn: &Connection) -> String {
        providers::save_provider(
            conn,
            &ProviderInput {
                id: None,
                name: "示例".into(),
                api_key: "secret".into(),
                endpoints: vec![ProviderEndpointInput {
                    id: None,
                    protocol: "openai".into(),
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
        .unwrap()
        .provider
        .id
    }

    fn seed_model(conn: &Connection, provider_id: &str, model_id: &str) -> String {
        providers::save_upstream_model(
            conn,
            &UpstreamModelInput {
                id: None,
                provider_id: provider_id.to_string(),
                model_id: model_id.to_string(),
                display_name: model_id.to_string(),
                input_price: 1.0,
                output_price: 2.0,
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

    fn save_targets(conn: &Connection, targets: Vec<RouteTargetInput>) {
        routes::save_route(
            conn,
            &RouteInput {
                id: None,
                alias: "lumen/x".into(),
                display_name: "X".into(),
                protocol: "openai".into(),
                icon: None,
                icon_tint: None,
                enabled: true,
                targets,
            },
        )
        .unwrap();
    }

    fn seed(db: &Db) {
        let conn = db.lock().unwrap();
        let provider = seed_provider(&conn);
        let model = seed_model(&conn, &provider, "gpt-x");
        save_targets(
            &conn,
            vec![RouteTargetInput {
                upstream_model_id: model,
                priority: 0,
                enabled: true,
            }],
        );
    }

    #[test]
    fn resolves_enabled_alias_to_upstream() {
        let db = open_in_memory().unwrap();
        seed(&db);
        let conn = db.lock().unwrap();
        let candidates = resolve_candidates(&conn, "lumen/x").unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].model_id, "gpt-x");
        assert_eq!(candidates[0].input_price, 1.0);
        assert_eq!(candidates[0].output_price, 2.0);
    }

    #[test]
    fn unknown_alias_yields_no_candidates() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        assert!(resolve_candidates(&conn, "missing").unwrap().is_empty());
    }

    #[test]
    fn candidates_follow_priority_order() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = seed_provider(&conn);
        let primary = seed_model(&conn, &provider, "primary");
        let backup = seed_model(&conn, &provider, "backup");
        // 故意逆序传入：primary 优先级 0，backup 优先级 1。
        save_targets(
            &conn,
            vec![
                RouteTargetInput {
                    upstream_model_id: backup,
                    priority: 1,
                    enabled: true,
                },
                RouteTargetInput {
                    upstream_model_id: primary,
                    priority: 0,
                    enabled: true,
                },
            ],
        );
        let models: Vec<String> = resolve_candidates(&conn, "lumen/x")
            .unwrap()
            .into_iter()
            .map(|candidate| candidate.model_id)
            .collect();
        assert_eq!(models, vec!["primary", "backup"]);
    }

    #[test]
    fn disabled_targets_are_skipped() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = seed_provider(&conn);
        let primary = seed_model(&conn, &provider, "primary");
        let backup = seed_model(&conn, &provider, "backup");
        save_targets(
            &conn,
            vec![
                RouteTargetInput {
                    upstream_model_id: primary,
                    priority: 0,
                    enabled: true,
                },
                RouteTargetInput {
                    upstream_model_id: backup,
                    priority: 1,
                    enabled: false,
                },
            ],
        );
        let models: Vec<String> = resolve_candidates(&conn, "lumen/x")
            .unwrap()
            .into_iter()
            .map(|candidate| candidate.model_id)
            .collect();
        assert_eq!(models, vec!["primary"]);
    }

    #[test]
    fn route_without_enabled_targets_is_empty() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = seed_provider(&conn);
        let model = seed_model(&conn, &provider, "primary");
        save_targets(
            &conn,
            vec![RouteTargetInput {
                upstream_model_id: model,
                priority: 0,
                enabled: true,
            }],
        );
        // 保存校验要求启用路由至少一个启用目标，故直接停用以模拟全部不可用。
        conn.execute("UPDATE route_targets SET enabled = 0", [])
            .unwrap();
        assert!(resolve_candidates(&conn, "lumen/x").unwrap().is_empty());
    }

    fn multi_provider(conn: &Connection) -> String {
        providers::save_provider(
            conn,
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
        .unwrap()
        .provider
        .id
    }

    fn route_for_protocol(conn: &Connection, alias: &str, protocol: &str, model: &str) {
        routes::save_route(
            conn,
            &RouteInput {
                id: None,
                alias: alias.into(),
                display_name: "R".into(),
                protocol: protocol.into(),
                icon: None,
                icon_tint: None,
                enabled: true,
                targets: vec![RouteTargetInput {
                    upstream_model_id: model.to_string(),
                    priority: 0,
                    enabled: true,
                }],
            },
        )
        .unwrap();
    }

    #[test]
    fn resolves_matching_endpoint_per_route_protocol() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = multi_provider(&conn);
        let model = seed_model(&conn, &provider, "gpt");
        route_for_protocol(&conn, "r-openai", "openai", &model);
        route_for_protocol(&conn, "r-anthropic", "anthropic", &model);

        let openai = resolve_candidates(&conn, "r-openai").unwrap();
        assert_eq!(openai.len(), 1);
        assert_eq!(openai[0].upstream_protocol, "openai");
        assert_eq!(openai[0].base_url, "https://a/v1");
        assert_eq!(openai[0].auth_scheme, "bearer");

        let anthropic = resolve_candidates(&conn, "r-anthropic").unwrap();
        assert_eq!(anthropic.len(), 1);
        assert_eq!(anthropic[0].upstream_protocol, "anthropic");
        assert_eq!(anthropic[0].base_url, "https://a/anthropic/v1");
        assert_eq!(anthropic[0].auth_scheme, "x-api-key");
    }

    #[test]
    fn missing_endpoint_yields_no_candidate() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = multi_provider(&conn);
        let model = seed_model(&conn, &provider, "gpt");
        route_for_protocol(&conn, "r-openai", "openai", &model);
        // 端点被删除（等价于该协议不再被支持）→ 该协议无候选。
        conn.execute("DELETE FROM provider_endpoints WHERE protocol = 'openai'", [])
            .unwrap();
        assert!(resolve_candidates(&conn, "r-openai").unwrap().is_empty());
    }
}
