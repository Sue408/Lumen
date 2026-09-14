use rusqlite::Connection;
use std::collections::{BTreeMap, HashMap};

use crate::db::models::{
    parse_provider_header_rules, Provider, ProviderEndpoint, ProviderHeaderRules,
};
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
    /// 实际发给上游的协议。当前同协议透传，等于本次请求的入站协议；
    /// 接入协议转换 SDK 后可能与入站协议不同。
    pub upstream_protocol: String,
    pub extra_headers: BTreeMap<String, String>,
    /// 上游 provider 的请求头映射（透传 / 替换 / 移除）；与 `extra_headers` 同为 provider 级。
    pub header_rules: ProviderHeaderRules,
}

/// 仅凭 provider 与端点拼出一个 `ResolvedRoute`，供探测这类「只发请求、不落库」的场景复用。
/// 模型相关字段留空、计费价为 0——只读请求不参与计费与日志，这些字段不会被读取。
pub fn endpoint_route(provider: &Provider, endpoint: &ProviderEndpoint) -> ResolvedRoute {
    ResolvedRoute {
        route_id: String::new(),
        upstream_model_id: String::new(),
        model_id: String::new(),
        display_name: String::new(),
        input_price: 0.0,
        output_price: 0.0,
        cache_read_price: 0.0,
        cache_creation_price: 0.0,
        provider_id: provider.id.clone(),
        base_url: endpoint.base_url.clone(),
        api_key: provider.api_key.clone(),
        auth_scheme: endpoint.auth_scheme.clone(),
        upstream_protocol: endpoint.protocol.clone(),
        extra_headers: provider.extra_headers.clone(),
        header_rules: provider.header_rules.clone(),
    }
}

/// 为入站协议挑选一个上游端点。当前只做「同协议透传」；接入协议转换 SDK
/// 后，在此补「无同协议端点则回退到可转换端点」的分支即可，调用方与 SQL
/// 结构无需改动。
fn select_endpoint<'a>(
    endpoints: &'a [ProviderEndpoint],
    inbound_protocol: &str,
) -> Option<&'a ProviderEndpoint> {
    endpoints
        .iter()
        .find(|endpoint| endpoint.protocol == inbound_protocol)
}

struct CandidateBase {
    route_id: String,
    upstream_model_id: String,
    model_id: String,
    display_name: String,
    input_price: f64,
    output_price: f64,
    cache_read_price: f64,
    cache_creation_price: f64,
    provider_id: String,
    api_key: String,
    extra_headers: BTreeMap<String, String>,
    header_rules: ProviderHeaderRules,
}

/// 解析别名对应的**全部启用候选**，按 `priority` 升序（同级按插入顺序）。
/// 候选按**入站协议**匹配端点：provider 没有该协议端点的目标自动落选，
/// 因此一条别名可在其目标支持的任意入站协议下使用。降级链即由此顺序决定。
pub fn resolve_candidates(
    conn: &Connection,
    alias: &str,
    inbound_protocol: &str,
) -> Result<Vec<ResolvedRoute>, AppError> {
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
            p.api_key       AS api_key,
            p.extra_headers AS extra_headers,
            p.header_rules  AS header_rules
         FROM routes r
         JOIN route_targets t   ON t.route_id = r.id
         JOIN upstream_models m ON m.id = t.upstream_model_id
         JOIN providers p       ON p.id = m.provider_id
         WHERE r.alias = ?1
           AND r.enabled = 1
           AND t.enabled = 1
           AND m.enabled = 1
           AND p.enabled = 1
         ORDER BY t.priority ASC, t.rowid ASC",
    )?;
    let rows = stmt.query_map([alias], |row| {
        let raw_headers: String = row.get("extra_headers")?;
        Ok(CandidateBase {
            route_id: row.get("route_id")?,
            upstream_model_id: row.get("upstream_model_id")?,
            model_id: row.get("model_id")?,
            display_name: row.get("display_name")?,
            input_price: row.get("input_price")?,
            output_price: row.get("output_price")?,
            cache_read_price: row.get("cache_read_price")?,
            cache_creation_price: row.get("cache_creation_price")?,
            provider_id: row.get("provider_id")?,
            api_key: row.get("api_key")?,
            extra_headers: serde_json::from_str(&raw_headers).unwrap_or_default(),
            header_rules: parse_provider_header_rules(&row.get::<_, String>("header_rules")?),
        })
    })?;
    let mut bases = Vec::new();
    for row in rows {
        bases.push(row?);
    }

    // 端点按 provider 归组，交给 `select_endpoint` 按入站协议挑选。
    let mut endpoints_by_provider: HashMap<String, Vec<ProviderEndpoint>> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT * FROM provider_endpoints ORDER BY rowid ASC")?;
        let rows = stmt.query_map([], ProviderEndpoint::from_row)?;
        for row in rows {
            let endpoint = row?;
            endpoints_by_provider
                .entry(endpoint.provider_id.clone())
                .or_default()
                .push(endpoint);
        }
    }

    let mut candidates = Vec::new();
    for base in bases {
        let Some(endpoints) = endpoints_by_provider.get(&base.provider_id) else {
            continue;
        };
        let Some(endpoint) = select_endpoint(endpoints, inbound_protocol) else {
            continue;
        };
        let base_url = endpoint.base_url.clone();
        let auth_scheme = endpoint.auth_scheme.clone();
        let upstream_protocol = endpoint.protocol.clone();
        candidates.push(ResolvedRoute {
            route_id: base.route_id,
            upstream_model_id: base.upstream_model_id,
            model_id: base.model_id,
            display_name: base.display_name,
            input_price: base.input_price,
            output_price: base.output_price,
            cache_read_price: base.cache_read_price,
            cache_creation_price: base.cache_creation_price,
            provider_id: base.provider_id,
            base_url,
            api_key: base.api_key,
            auth_scheme,
            upstream_protocol,
            extra_headers: base.extra_headers,
            header_rules: base.header_rules,
        });
    }
    Ok(candidates)
}

/// 解析别名的全部有序候选，供降级链按序尝试；`inbound_protocol` 为本次请求的入站协议。
pub async fn resolve_all(
    state: &AppState,
    alias: &str,
    inbound_protocol: &str,
) -> Result<Vec<ResolvedRoute>, AppError> {
    let alias = alias.to_string();
    let inbound_protocol = inbound_protocol.to_string();
    with_db(&state.db, move |conn| {
        resolve_candidates(conn, &alias, &inbound_protocol)
    })
    .await
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
        let candidates = resolve_candidates(&conn, "lumen/x", "openai").unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].model_id, "gpt-x");
        assert_eq!(candidates[0].input_price, 1.0);
        assert_eq!(candidates[0].output_price, 2.0);
    }

    #[test]
    fn unknown_alias_yields_no_candidates() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        assert!(resolve_candidates(&conn, "missing", "openai").unwrap().is_empty());
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
        let models: Vec<String> = resolve_candidates(&conn, "lumen/x", "openai")
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
        let models: Vec<String> = resolve_candidates(&conn, "lumen/x", "openai")
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
        assert!(resolve_candidates(&conn, "lumen/x", "openai").unwrap().is_empty());
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
    fn implicit_multi_protocol_resolves_same_route_per_inbound() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = multi_provider(&conn);
        let model = seed_model(&conn, &provider, "gpt");
        // 一条路由挂一个多协议目标，即可在其 provider 支持的各入站协议下命中。
        route_for_protocol(&conn, "shared", "openai", &model);

        let openai = resolve_candidates(&conn, "shared", "openai").unwrap();
        assert_eq!(openai.len(), 1);
        assert_eq!(openai[0].upstream_protocol, "openai");
        assert_eq!(openai[0].base_url, "https://a/v1");
        assert_eq!(openai[0].auth_scheme, "bearer");

        let anthropic = resolve_candidates(&conn, "shared", "anthropic").unwrap();
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
        route_for_protocol(&conn, "shared", "openai", &model);
        // 删掉 openai 端点：该入站协议下无候选，其它协议不受影响。
        conn.execute("DELETE FROM provider_endpoints WHERE protocol = 'openai'", [])
            .unwrap();
        assert!(resolve_candidates(&conn, "shared", "openai").unwrap().is_empty());
        assert_eq!(resolve_candidates(&conn, "shared", "anthropic").unwrap().len(), 1);
    }
}
