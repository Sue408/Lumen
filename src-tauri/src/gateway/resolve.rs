use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

use crate::db::models::{
    parse_provider_header_rules, Provider, ProviderEndpoint, ProviderHeaderRules,
    PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI, PROTOCOL_RESPONSES,
};
use crate::db::with_db;
use crate::error::AppError;
use crate::gateway::convert;
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
    /// 实际发给上游的协议。优先与入站协议一致（字节透传）；无同协议端点时回退到
    /// 可转换端点，此时与入站协议不同，由 `gateway::convert` 负责转换。
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

/// 为入站协议挑选一个上游端点：同协议端点优先（字节透传）；没有则回退到该 provider
/// 第一个**可转换**端点（openai / anthropic / responses 之间）。Gemini 不在转换核内，
/// 既不能作为回退目标，其入站请求也不会回退到别的协议。
fn select_endpoint<'a>(
    endpoints: &'a [ProviderEndpoint],
    inbound_protocol: &str,
) -> Option<&'a ProviderEndpoint> {
    endpoints
        .iter()
        .find(|endpoint| endpoint.protocol == inbound_protocol)
        .or_else(|| {
            endpoints
                .iter()
                .find(|endpoint| convert::needs_conversion(inbound_protocol, &endpoint.protocol))
        })
}

/// 端点按 provider 归组，顺序稳定（rowid 升序）——`select_endpoint` 的回退选择依赖它。
/// 热路径解析与诊断预览共用，避免两处各自取端点而产生语义漂移。
fn load_endpoints(
    conn: &Connection,
) -> Result<HashMap<String, Vec<ProviderEndpoint>>, AppError> {
    let mut by_provider: HashMap<String, Vec<ProviderEndpoint>> = HashMap::new();
    let mut stmt = conn.prepare("SELECT * FROM provider_endpoints ORDER BY rowid ASC")?;
    let rows = stmt.query_map([], ProviderEndpoint::from_row)?;
    for row in rows {
        let endpoint = row?;
        by_provider
            .entry(endpoint.provider_id.clone())
            .or_default()
            .push(endpoint);
    }
    Ok(by_provider)
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
    let endpoints_by_provider = load_endpoints(conn)?;

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

/// 诊断预览覆盖的入站协议：参与跨协议转换回退的三个。Gemini 的别名在 URL path 上
/// 而非请求体，不走同一套候选解析，故不在此列。
pub const EXPLAIN_PROTOCOLS: &[&str] =
    &[PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI, PROTOCOL_RESPONSES];

/// 一个目标为何没能进入某协议的候选链。
///
/// 热路径的 `resolve_candidates` 是**静默过滤**——目标停用、提供商停用、端点服务不了
/// 入站协议，全都只表现为「这个目标没出现在候选里」；路由页照着 `route_targets` 渲染，
/// 三条目标看着一模一样。这些原因在此显式化，供路由页预览与空候选时的错误文案使用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// 路由本身已停用。
    RouteDisabled,
    /// 该目标在路由里被停用。
    TargetDisabled,
    /// 上游模型被停用。
    ModelDisabled,
    /// 提供商被停用。
    ProviderDisabled,
    /// 提供商一个协议端点都没有。
    NoEndpoint,
    /// 现有端点都无法服务该入站协议：既不同协议，也不在转换核内。
    NoEndpointForProtocol { available: Vec<String> },
}

impl SkipReason {
    pub fn describe(&self, inbound_protocol: &str) -> String {
        match self {
            SkipReason::RouteDisabled => "路由已停用".to_string(),
            SkipReason::TargetDisabled => "目标已停用".to_string(),
            SkipReason::ModelDisabled => "上游模型已停用".to_string(),
            SkipReason::ProviderDisabled => "提供商已停用".to_string(),
            SkipReason::NoEndpoint => "提供商没有任何协议端点".to_string(),
            SkipReason::NoEndpointForProtocol { available } => {
                let list = if available.is_empty() {
                    "无".to_string()
                } else {
                    available.join(" / ")
                };
                format!("没有可服务 {inbound_protocol} 的端点（现有：{list}）")
            }
        }
    }
}

/// 一个被跳过的目标及其原因（面向 UI 的扁平形状，原因已是人话）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedTarget {
    pub upstream_model_id: String,
    pub display_name: String,
    pub reason: String,
}

/// 某个入站协议下的解析结果：会按序尝试谁、谁被跳过、为什么。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolResolution {
    pub protocol: String,
    /// 该协议下实际会按序尝试的上游模型（与热路径候选同序）。
    pub candidates: Vec<String>,
    pub skipped: Vec<SkippedTarget>,
}

/// 一条别名解析的完整解释。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteExplanation {
    pub alias: String,
    /// 别名是否存在（不存在时 `protocols` 为空）。
    pub found: bool,
    /// 路由是否启用。
    pub enabled: bool,
    pub protocols: Vec<ProtocolResolution>,
}

/// 单个目标在诊断中的中间态。
struct ExplainTarget {
    upstream_model_id: String,
    display_name: String,
    target_enabled: bool,
    model_enabled: bool,
    provider_enabled: bool,
    provider_id: String,
    endpoints: Vec<ProviderEndpoint>,
}

/// 目标在某入站协议下能否成为候选；`None` 表示可以。判定顺序与热路径的过滤条件一致。
fn skip_reason(
    route_enabled: bool,
    target_enabled: bool,
    model_enabled: bool,
    provider_enabled: bool,
    endpoints: &[ProviderEndpoint],
    inbound_protocol: &str,
) -> Option<SkipReason> {
    if !route_enabled {
        return Some(SkipReason::RouteDisabled);
    }
    if !target_enabled {
        return Some(SkipReason::TargetDisabled);
    }
    if !model_enabled {
        return Some(SkipReason::ModelDisabled);
    }
    if !provider_enabled {
        return Some(SkipReason::ProviderDisabled);
    }
    if endpoints.is_empty() {
        return Some(SkipReason::NoEndpoint);
    }
    if select_endpoint(endpoints, inbound_protocol).is_none() {
        return Some(SkipReason::NoEndpointForProtocol {
            available: endpoints
                .iter()
                .map(|endpoint| endpoint.protocol.clone())
                .collect(),
        });
    }
    None
}

/// 解释一条别名的解析：三协议各自会走谁、谁被跳过、为什么。
///
/// 刻意**不复用**热路径的 SQL：热路径的 `WHERE ... = 1` 过滤正是要诊断的东西，诊断
/// 必须看得见被过滤掉的行。两者的一致性由测试 `explain_matches_resolve` 锁定。
pub fn explain_route(conn: &Connection, alias: &str) -> Result<RouteExplanation, AppError> {
    let route = conn
        .query_row(
            "SELECT id, enabled FROM routes WHERE alias = ?1",
            [alias],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0)),
        )
        .optional()?;
    let Some((route_id, route_enabled)) = route else {
        return Ok(RouteExplanation {
            alias: alias.to_string(),
            found: false,
            enabled: false,
            protocols: Vec::new(),
        });
    };

    // 目标 / 模型 / 提供商一次读齐；外键保证模型与提供商必然存在。
    let mut targets = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT t.upstream_model_id, t.enabled,
                    m.display_name, m.enabled,
                    p.id, p.enabled
               FROM route_targets t
               JOIN upstream_models m ON m.id = t.upstream_model_id
               JOIN providers p       ON p.id = m.provider_id
              WHERE t.route_id = ?1
              ORDER BY t.priority ASC, t.rowid ASC",
        )?;
        let rows = stmt.query_map([&route_id], |row| {
            Ok(ExplainTarget {
                upstream_model_id: row.get(0)?,
                target_enabled: row.get::<_, i64>(1)? != 0,
                display_name: row.get(2)?,
                model_enabled: row.get::<_, i64>(3)? != 0,
                provider_id: row.get(4)?,
                provider_enabled: row.get::<_, i64>(5)? != 0,
                endpoints: Vec::new(),
            })
        })?;
        for row in rows {
            targets.push(row?);
        }
    }

    let endpoints_by_provider = load_endpoints(conn)?;
    for target in &mut targets {
        if let Some(endpoints) = endpoints_by_provider.get(&target.provider_id) {
            target.endpoints = endpoints.clone();
        }
    }

    let protocols = EXPLAIN_PROTOCOLS
        .iter()
        .map(|protocol| {
            let mut candidates = Vec::new();
            let mut skipped = Vec::new();
            for target in &targets {
                match skip_reason(
                    route_enabled,
                    target.target_enabled,
                    target.model_enabled,
                    target.provider_enabled,
                    &target.endpoints,
                    protocol,
                ) {
                    None => candidates.push(target.upstream_model_id.clone()),
                    Some(reason) => skipped.push(SkippedTarget {
                        upstream_model_id: target.upstream_model_id.clone(),
                        display_name: target.display_name.clone(),
                        reason: reason.describe(protocol),
                    }),
                }
            }
            ProtocolResolution {
                protocol: (*protocol).to_string(),
                candidates,
                skipped,
            }
        })
        .collect();

    Ok(RouteExplanation {
        alias: alias.to_string(),
        found: true,
        enabled: route_enabled,
        protocols,
    })
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
    fn missing_same_protocol_endpoint_falls_back_to_convertible() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = multi_provider(&conn);
        let model = seed_model(&conn, &provider, "gpt");
        route_for_protocol(&conn, "shared", "openai", &model);
        // 删掉 openai 端点：openai 入站回退到可转换的 anthropic 端点。
        conn.execute("DELETE FROM provider_endpoints WHERE protocol = 'openai'", [])
            .unwrap();
        let fallback = resolve_candidates(&conn, "shared", "openai").unwrap();
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].upstream_protocol, "anthropic");
        assert_eq!(fallback[0].base_url, "https://a/anthropic/v1");
        // anthropic 入站仍命中自身的 anthropic 端点。
        assert_eq!(resolve_candidates(&conn, "shared", "anthropic").unwrap().len(), 1);
    }

    fn gemini_provider(conn: &Connection) -> String {
        providers::save_provider(
            conn,
            &ProviderInput {
                id: None,
                name: "gemini".into(),
                api_key: "secret".into(),
                endpoints: vec![ProviderEndpointInput {
                    id: None,
                    protocol: "gemini".into(),
                    base_url: "https://g/v1beta".into(),
                    auth_scheme: "x-goog-api-key".into(),
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

    #[test]
    fn gemini_endpoint_never_serves_other_inbound() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = gemini_provider(&conn);
        let model = seed_model(&conn, &provider, "gemini-pro");
        route_for_protocol(&conn, "g", "gemini", &model);

        assert_eq!(resolve_candidates(&conn, "g", "gemini").unwrap().len(), 1);
        // Gemini 不在转换核内，其它入站协议既不回退到它，也不从它回退出去。
        assert!(resolve_candidates(&conn, "g", "openai").unwrap().is_empty());
        assert!(resolve_candidates(&conn, "g", "anthropic").unwrap().is_empty());
    }

    fn resolution<'a>(explanation: &'a RouteExplanation, name: &str) -> &'a ProtocolResolution {
        explanation
            .protocols
            .iter()
            .find(|entry| entry.protocol == name)
            .expect("预览应覆盖该协议")
    }

    /// 诊断必须与热路径给出同一份候选——两套查询一旦漂移，预览就会骗人。
    #[test]
    fn explain_matches_resolve() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = multi_provider(&conn);
        let primary = seed_model(&conn, &provider, "primary");
        let backup = seed_model(&conn, &provider, "backup");
        save_targets(
            &conn,
            vec![
                RouteTargetInput {
                    upstream_model_id: backup.clone(),
                    priority: 1,
                    enabled: true,
                },
                RouteTargetInput {
                    upstream_model_id: primary.clone(),
                    priority: 0,
                    enabled: true,
                },
            ],
        );
        // 停用一条：诊断与热路径都要把它排除，且顺序都按 priority。
        conn.execute(
            "UPDATE route_targets SET enabled = 0 WHERE upstream_model_id = ?1",
            [&backup],
        )
        .unwrap();

        let explanation = explain_route(&conn, "lumen/x").unwrap();
        assert!(explanation.found && explanation.enabled);
        for name in EXPLAIN_PROTOCOLS {
            let expected: Vec<String> = resolve_candidates(&conn, "lumen/x", name)
                .unwrap()
                .into_iter()
                .map(|candidate| candidate.upstream_model_id)
                .collect();
            assert_eq!(
                &expected,
                &resolution(&explanation, name).candidates,
                "{name} 的候选应与热路径一致"
            );
        }
    }

    #[test]
    fn explain_reports_unknown_alias() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let explanation = explain_route(&conn, "missing").unwrap();
        assert!(!explanation.found);
        assert!(explanation.protocols.is_empty());
    }

    /// 四道启用闸门按序命中，每道都给出各自的原因。
    #[test]
    fn explain_names_each_disabled_gate() {
        let db = open_in_memory().unwrap();
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

        // 逐个闸门关上、断言每次只报当前这一道；每轮结束恢复全部状态。
        for (statement, expected) in [
            ("UPDATE routes SET enabled = 0", "路由已停用"),
            ("UPDATE route_targets SET enabled = 0", "目标已停用"),
            ("UPDATE upstream_models SET enabled = 0", "上游模型已停用"),
            ("UPDATE providers SET enabled = 0", "提供商已停用"),
        ] {
            conn.execute(statement, []).unwrap();
            let explanation = explain_route(&conn, "lumen/x").unwrap();
            let skipped = &resolution(&explanation, "openai").skipped;
            assert_eq!(skipped.len(), 1, "{statement} 后应只剩一个被跳过目标");
            assert_eq!(skipped[0].reason, expected, "语句：{statement}");
            conn.execute("UPDATE routes SET enabled = 1", []).unwrap();
            conn.execute("UPDATE route_targets SET enabled = 1", []).unwrap();
            conn.execute("UPDATE upstream_models SET enabled = 1", []).unwrap();
            conn.execute("UPDATE providers SET enabled = 1", []).unwrap();
        }
    }

    /// 端点服务不了入站协议时，原因要点出「现有是哪些协议」，否则用户无从下手。
    #[test]
    fn explain_names_unserviceable_endpoints() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let provider = gemini_provider(&conn);
        let model = seed_model(&conn, &provider, "gemini-pro");
        route_for_protocol(&conn, "g", "gemini", &model);

        let explanation = explain_route(&conn, "g").unwrap();
        let anthropic = resolution(&explanation, "anthropic");
        assert!(anthropic.candidates.is_empty());
        assert_eq!(
            anthropic.skipped[0].reason,
            "没有可服务 anthropic 的端点（现有：gemini）"
        );
    }
}
