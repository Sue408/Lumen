use rusqlite::Connection;
use std::collections::BTreeMap;

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
    pub base_url: String,
    pub api_key: String,
    pub auth_scheme: String,
    pub protocol: String,
    pub extra_headers: BTreeMap<String, String>,
}

pub fn resolve_route(conn: &Connection, alias: &str) -> Result<Option<ResolvedRoute>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT
            r.id            AS route_id,
            m.id            AS upstream_model_id,
            m.model_id      AS model_id,
            m.display_name  AS display_name,
            m.input_price   AS input_price,
            m.output_price  AS output_price,
            m.cache_read_price AS cache_read_price,
            m.cache_creation_price AS cache_creation_price,
            p.id            AS provider_id,
            p.base_url      AS base_url,
            p.api_key       AS api_key,
            p.auth_scheme   AS auth_scheme,
            p.protocol      AS protocol,
            p.extra_headers AS extra_headers
         FROM routes r
         JOIN route_targets t   ON t.route_id = r.id
         JOIN upstream_models m ON m.id = t.upstream_model_id
         JOIN providers p       ON p.id = m.provider_id
         WHERE r.alias = ?1
           AND r.enabled = 1
           AND t.enabled = 1
           AND m.enabled = 1
           AND p.enabled = 1
         ORDER BY t.priority ASC, t.rowid ASC
         LIMIT 1",
    )?;
    let mut rows = stmt.query_map([alias], |row| {
        let raw_headers: String = row.get("extra_headers")?;
        Ok(ResolvedRoute {
            route_id: row.get("route_id")?,
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
            protocol: row.get("protocol")?,
            extra_headers: serde_json::from_str(&raw_headers).unwrap_or_default(),
        })
    })?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub async fn resolve(state: &AppState, alias: &str) -> Result<Option<ResolvedRoute>, AppError> {
    let alias = alias.to_string();
    with_db(&state.db, move |conn| resolve_route(conn, &alias)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{ProviderInput, RouteInput, RouteTargetInput, UpstreamModelInput};
    use crate::db::{open_in_memory, providers, routes, Db};

    fn seed(db: &Db) {
        let conn = db.lock().unwrap();
        let provider = providers::save_provider(
            &conn,
            &ProviderInput {
                id: None,
                name: "示例".into(),
                base_url: "https://example.com/v1".into(),
                api_key: "secret".into(),
                auth_scheme: "bearer".into(),
                protocol: "openai".into(),
                extra_headers: BTreeMap::new(),
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
                provider_id: provider.id,
                model_id: "gpt-x".into(),
                display_name: "GPT X".into(),
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
        .unwrap();
        routes::save_route(
            &conn,
            &RouteInput {
                id: None,
                alias: "lumen/x".into(),
                display_name: "X".into(),
                enabled: true,
                targets: vec![RouteTargetInput {
                    upstream_model_id: model.id,
                    priority: 0,
                    enabled: true,
                }],
            },
        )
        .unwrap();
    }

    #[test]
    fn resolves_enabled_alias_to_upstream() {
        let db = open_in_memory().unwrap();
        seed(&db);
        let conn = db.lock().unwrap();
        let resolved = resolve_route(&conn, "lumen/x").unwrap().expect("应能解析");
        assert_eq!(resolved.model_id, "gpt-x");
        assert_eq!(resolved.input_price, 1.0);
        assert_eq!(resolved.output_price, 2.0);
    }

    #[test]
    fn unknown_alias_returns_none() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        assert!(resolve_route(&conn, "missing").unwrap().is_none());
    }
}
