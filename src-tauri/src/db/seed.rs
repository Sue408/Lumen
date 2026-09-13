use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::models::{
    default_auth_scheme, default_icon_tint, default_protocol, default_quota_period, default_true,
    is_known_protocol, ProviderHeaderRules,
};
use crate::error::AppError;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeedFile {
    #[serde(default)]
    providers: Vec<SeedProvider>,
    #[serde(default)]
    upstream_models: Vec<SeedModel>,
    #[serde(default)]
    routes: Vec<SeedRoute>,
    #[serde(default)]
    virtual_keys: Vec<SeedVirtualKey>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeedEndpoint {
    protocol: String,
    base_url: String,
    #[serde(default = "default_auth_scheme")]
    auth_scheme: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeedProvider {
    name: String,
    /// 新格式：协议端点列表。
    #[serde(default)]
    endpoints: Vec<SeedEndpoint>,
    #[serde(default)]
    api_key: String,
    // 旧格式兼容字段（仅反序列化，导出不再写出）：当 `endpoints` 为空时合成一条端点。
    #[serde(default, skip_serializing)]
    base_url: String,
    #[serde(default = "default_auth_scheme", skip_serializing)]
    auth_scheme: String,
    #[serde(default = "default_protocol", skip_serializing)]
    protocol: String,
    #[serde(default)]
    extra_headers: BTreeMap<String, String>,
    #[serde(default)]
    header_rules: ProviderHeaderRules,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default = "default_icon_tint")]
    icon_tint: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

/// 归一化提供商端点：优先新格式；否则用旧格式的 `protocol / base_url / auth_scheme`
/// 合成一条端点。两者皆空则报错。
fn normalize_endpoints(provider: &SeedProvider) -> Result<Vec<SeedEndpoint>, AppError> {
    if !provider.endpoints.is_empty() {
        return Ok(provider.endpoints.clone());
    }
    if provider.base_url.trim().is_empty() {
        return Err(AppError::message(format!(
            "seed 提供商 {} 缺少协议端点（endpoints 或 baseUrl）",
            provider.name
        )));
    }
    Ok(vec![SeedEndpoint {
        protocol: provider.protocol.clone(),
        base_url: provider.base_url.clone(),
        auth_scheme: provider.auth_scheme.clone(),
        enabled: true,
    }])
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeedModel {
    provider: String,
    model_id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    input_price: f64,
    #[serde(default)]
    output_price: f64,
    #[serde(default)]
    cache_read_price: f64,
    #[serde(default)]
    cache_creation_price: f64,
    #[serde(default)]
    context_window: i64,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default = "default_icon_tint")]
    icon_tint: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeedRoute {
    alias: String,
    #[serde(default)]
    display_name: String,
    #[serde(default = "default_protocol")]
    protocol: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    icon_tint: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    targets: Vec<SeedTarget>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeedTarget {
    upstream_model: String,
    #[serde(default)]
    priority: i64,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeedVirtualKey {
    key: String,
    name: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    quota_limit: Option<f64>,
    #[serde(default = "default_quota_period")]
    quota_period: String,
}

/// 单类配置的导入结果：新建与覆盖的条数。
#[derive(Debug, Default, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSummary {
    pub created: usize,
    pub updated: usize,
}

/// 合并导入汇总，供前端展示。
#[derive(Debug, Default, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub providers: ItemSummary,
    pub models: ItemSummary,
    pub routes: ItemSummary,
    pub virtual_keys: ItemSummary,
}

/// 查询自然键命中的已存在主键；无匹配返回 `None`。
fn existing_id<P: rusqlite::Params>(
    tx: &rusqlite::Transaction,
    sql: &str,
    params: P,
) -> Result<Option<String>, AppError> {
    let mut stmt = tx.prepare(sql)?;
    let mut rows = stmt.query_map(params, |row| row.get::<_, String>(0))?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// 解析迁移 JSON 并在单事务内合并导入；`commit = false` 时仅预演并回滚。
pub fn import_seed_json(
    conn: &Connection,
    raw: &str,
    commit: bool,
) -> Result<ImportSummary, AppError> {
    let seed: SeedFile = serde_json::from_str(raw)?;
    merge_seed(conn, &seed, commit)
}

/// 合并导入：按自然键覆盖已存在条目、追加新条目，未提及的条目保留。
/// provider / upstream_models / routes 的引用按名称解析；全程单事务，任一步失败整体回滚。
fn merge_seed(conn: &Connection, seed: &SeedFile, commit: bool) -> Result<ImportSummary, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    let mut summary = ImportSummary::default();
    // 先载入库中已有的 provider / model：这样「只含部分条目」的文件也能引用
    // 未在本次文件里出现的既有 provider 与 model，而不是只能整包导入。
    let mut provider_ids: HashMap<String, String> = HashMap::new();
    // provider_id -> 该提供商已有的协议端点集合，供路由目标校验。
    let mut provider_protocols: HashMap<String, HashSet<String>> = HashMap::new();
    {
        let mut stmt = tx.prepare("SELECT id, name FROM providers")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, name) = row?;
            provider_ids.insert(name, id);
        }
        let mut stmt = tx.prepare("SELECT provider_id, protocol FROM provider_endpoints")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (provider_id, protocol) = row?;
            provider_protocols
                .entry(provider_id)
                .or_default()
                .insert(protocol);
        }
    }
    for provider in &seed.providers {
        let endpoints = normalize_endpoints(provider)?;
        for endpoint in &endpoints {
            if !is_known_protocol(&endpoint.protocol) {
                return Err(AppError::message(format!(
                    "seed 提供商 {} 使用了未知协议：{}",
                    provider.name, endpoint.protocol
                )));
            }
        }
        let extra_headers = serde_json::to_string(&provider.extra_headers)?;
        let header_rules = serde_json::to_string(&provider.header_rules)?;
        let id = match provider_ids.get(&provider.name).cloned() {
            Some(id) => {
                tx.execute(
                    "UPDATE providers SET api_key = ?1, extra_headers = ?2, header_rules = ?3,
                        icon = ?4, icon_tint = ?5, enabled = ?6
                     WHERE id = ?7",
                    params![
                        provider.api_key,
                        extra_headers,
                        header_rules,
                        provider.icon,
                        provider.icon_tint,
                        provider.enabled as i64,
                        id,
                    ],
                )?;
                summary.providers.updated += 1;
                id
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO providers
                        (id, name, api_key, extra_headers, header_rules, icon, icon_tint, enabled, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        id,
                        provider.name,
                        provider.api_key,
                        extra_headers,
                        header_rules,
                        provider.icon,
                        provider.icon_tint,
                        provider.enabled as i64,
                        now,
                    ],
                )?;
                summary.providers.created += 1;
                id
            }
        };
        // 端点全量替换：seed 是配置合并，不要求端点 id 稳定。
        tx.execute("DELETE FROM provider_endpoints WHERE provider_id = ?1", [&id])?;
        for endpoint in &endpoints {
            tx.execute(
                "INSERT INTO provider_endpoints
                    (id, provider_id, protocol, base_url, auth_scheme, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    id,
                    endpoint.protocol,
                    endpoint.base_url,
                    endpoint.auth_scheme,
                    endpoint.enabled as i64,
                ],
            )?;
        }
        provider_protocols.insert(
            id.clone(),
            endpoints
                .iter()
                .map(|endpoint| endpoint.protocol.clone())
                .collect(),
        );
        provider_ids.insert(provider.name.clone(), id);
    }

    let mut model_ids: HashMap<String, String> = HashMap::new();
    // model_id -> provider_id，供路由目标校验该提供商是否提供目标协议端点。
    let mut model_providers: HashMap<String, String> = HashMap::new();
    {
        let mut stmt =
            tx.prepare("SELECT m.id, m.model_id, m.display_name, m.provider_id FROM upstream_models m")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (id, model_id, display_name, provider_id) = row?;
            model_providers.insert(id.clone(), provider_id);
            model_ids.insert(model_id, id.clone());
            model_ids.entry(display_name).or_insert(id);
        }
    }
    for model in &seed.upstream_models {
        let provider_id = provider_ids.get(&model.provider).ok_or_else(|| {
            AppError::message(format!("seed 引用了不存在的提供商：{}", model.provider))
        })?;
        let display_name = if model.display_name.is_empty() {
            model.model_id.clone()
        } else {
            model.display_name.clone()
        };
        let capabilities = serde_json::to_string(&model.capabilities)?;
        let id = match existing_id(
            &tx,
            "SELECT id FROM upstream_models WHERE provider_id = ?1 AND model_id = ?2",
            params![provider_id, model.model_id],
        )? {
            Some(id) => {
                tx.execute(
                    "UPDATE upstream_models SET display_name = ?1, input_price = ?2,
                        output_price = ?3, cache_read_price = ?4, cache_creation_price = ?5,
                        context_window = ?6, capabilities = ?7, icon = ?8, icon_tint = ?9, enabled = ?10
                     WHERE id = ?11",
                    params![
                        display_name,
                        model.input_price,
                        model.output_price,
                        model.cache_read_price,
                        model.cache_creation_price,
                        model.context_window,
                        capabilities,
                        model.icon,
                        model.icon_tint,
                        model.enabled as i64,
                        id,
                    ],
                )?;
                summary.models.updated += 1;
                id
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO upstream_models
                        (id, provider_id, model_id, display_name, input_price, output_price,
                         cache_read_price, cache_creation_price, context_window, capabilities,
                         icon, icon_tint, enabled)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        id,
                        provider_id,
                        model.model_id,
                        display_name,
                        model.input_price,
                        model.output_price,
                        model.cache_read_price,
                        model.cache_creation_price,
                        model.context_window,
                        capabilities,
                        model.icon,
                        model.icon_tint,
                        model.enabled as i64,
                    ],
                )?;
                summary.models.created += 1;
                id
            }
        };
        model_providers.insert(id.clone(), provider_id.clone());
        model_ids.insert(model.model_id.clone(), id.clone());
        model_ids.entry(display_name).or_insert(id);
    }

    for route in &seed.routes {
        if !is_known_protocol(&route.protocol) {
            return Err(AppError::message(format!(
                "seed 路由 {} 使用了未知协议：{}",
                route.alias, route.protocol
            )));
        }
        let display_name = if route.display_name.is_empty() {
            route.alias.clone()
        } else {
            route.display_name.clone()
        };
        let route_id = match existing_id(
            &tx,
            "SELECT id FROM routes WHERE alias = ?1",
            params![route.alias],
        )? {
            Some(id) => {
                tx.execute(
                    "UPDATE routes SET display_name = ?1, protocol = ?2, icon = ?3, icon_tint = ?4, enabled = ?5
                       WHERE id = ?6",
                    params![
                        display_name,
                        route.protocol,
                        route.icon,
                        route.icon_tint,
                        route.enabled as i64,
                        id
                    ],
                )?;
                tx.execute("DELETE FROM route_targets WHERE route_id = ?1", params![id])?;
                summary.routes.updated += 1;
                id
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO routes (id, alias, display_name, protocol, icon, icon_tint, enabled, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        id,
                        route.alias,
                        display_name,
                        route.protocol,
                        route.icon,
                        route.icon_tint,
                        route.enabled as i64,
                        now
                    ],
                )?;
                summary.routes.created += 1;
                id
            }
        };
        for target in &route.targets {
            let model_id = model_ids.get(&target.upstream_model).ok_or_else(|| {
                AppError::message(format!(
                    "seed 路由 {} 引用了不存在的上游模型：{}",
                    route.alias, target.upstream_model
                ))
            })?;
            // 同协议透传：目标所属提供商必须提供路由协议的端点。
            let supported = model_providers
                .get(model_id)
                .and_then(|provider_id| provider_protocols.get(provider_id))
                .map(|protocols| protocols.contains(&route.protocol))
                .unwrap_or(false);
            if !supported {
                return Err(AppError::message(format!(
                    "seed 路由 {} 的目标提供商未提供 {} 协议端点：{}",
                    route.alias, route.protocol, target.upstream_model
                )));
            }
            tx.execute(
                "INSERT INTO route_targets
                    (id, route_id, upstream_model_id, priority, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    route_id,
                    model_id,
                    target.priority,
                    target.enabled as i64,
                ],
            )?;
        }
    }

    for virtual_key in &seed.virtual_keys {
        match existing_id(
            &tx,
            "SELECT id FROM virtual_keys WHERE key = ?1",
            params![virtual_key.key],
        )? {
            Some(id) => {
                tx.execute(
                    "UPDATE virtual_keys SET name = ?1, enabled = ?2, quota_limit = ?3, quota_period = ?4
                     WHERE id = ?5",
                    params![
                        virtual_key.name,
                        virtual_key.enabled as i64,
                        virtual_key.quota_limit,
                        virtual_key.quota_period,
                        id,
                    ],
                )?;
                summary.virtual_keys.updated += 1;
            }
            None => {
                tx.execute(
                    "INSERT INTO virtual_keys (id, key, name, enabled, quota_limit, quota_period, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        uuid::Uuid::new_v4().to_string(),
                        virtual_key.key,
                        virtual_key.name,
                        virtual_key.enabled as i64,
                        virtual_key.quota_limit,
                        virtual_key.quota_period,
                        now,
                    ],
                )?;
                summary.virtual_keys.created += 1;
            }
        }
    }

    if commit {
        tx.commit()?;
    } else {
        tx.rollback()?;
    }
    Ok(summary)
}

/// 把当前配置导出为 seed JSON 文本，格式与导入一致（不含日志、密钥偏好等运行时数据）。
pub fn export_seed(conn: &Connection) -> Result<String, AppError> {
    let mut providers = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, name, api_key, extra_headers, header_rules, icon, icon_tint, enabled
             FROM providers ORDER BY created_at, name",
        )?;
        let rows = stmt.query_map([], |row| {
            let extra_raw: String = row.get(3)?;
            let rules_raw: String = row.get(4)?;
            Ok((
                row.get::<_, String>(0)?,
                SeedProvider {
                    name: row.get(1)?,
                    endpoints: Vec::new(),
                    api_key: row.get(2)?,
                    base_url: String::new(),
                    auth_scheme: default_auth_scheme(),
                    protocol: default_protocol(),
                    extra_headers: serde_json::from_str(&extra_raw).unwrap_or_default(),
                    header_rules: serde_json::from_str(&rules_raw).unwrap_or_default(),
                    icon: row.get(5)?,
                    icon_tint: row.get(6)?,
                    enabled: row.get::<_, i64>(7)? != 0,
                },
            ))
        })?;
        let mut endpoint_stmt = conn.prepare(
            "SELECT protocol, base_url, auth_scheme, enabled
               FROM provider_endpoints WHERE provider_id = ?1 ORDER BY rowid ASC",
        )?;
        for row in rows {
            let (id, mut provider) = row?;
            provider.endpoints = endpoint_stmt
                .query_map([&id], |row| {
                    Ok(SeedEndpoint {
                        protocol: row.get(0)?,
                        base_url: row.get(1)?,
                        auth_scheme: row.get(2)?,
                        enabled: row.get::<_, i64>(3)? != 0,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            providers.push(provider);
        }
    }

    let mut model_names: HashMap<String, String> = HashMap::new();
    let mut upstream_models = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT m.id, p.name, m.model_id, m.display_name, m.input_price, m.output_price,
                    m.cache_read_price, m.cache_creation_price, m.context_window, m.capabilities,
                    m.icon, m.icon_tint, m.enabled
             FROM upstream_models m JOIN providers p ON p.id = m.provider_id
             ORDER BY p.name, m.model_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                SeedModel {
                    provider: row.get(1)?,
                    model_id: row.get(2)?,
                    display_name: row.get(3)?,
                    input_price: row.get(4)?,
                    output_price: row.get(5)?,
                    cache_read_price: row.get(6)?,
                    cache_creation_price: row.get(7)?,
                    context_window: row.get(8)?,
                    capabilities: serde_json::from_str(&row.get::<_, String>(9)?)
                        .unwrap_or_default(),
                    icon: row.get(10)?,
                    icon_tint: row.get(11)?,
                    enabled: row.get::<_, i64>(12)? != 0,
                },
            ))
        })?;
        for row in rows {
            let (id, model) = row?;
            model_names.insert(id, model.model_id.clone());
            upstream_models.push(model);
        }
    }

    let route_rows: Vec<(String, SeedRoute)> = {
        let mut stmt = conn.prepare(
            "SELECT id, alias, display_name, protocol, enabled, icon, icon_tint
             FROM routes ORDER BY created_at, alias",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                SeedRoute {
                    alias: row.get(1)?,
                    display_name: row.get(2)?,
                    protocol: row.get(3)?,
                    icon: row.get(5)?,
                    icon_tint: row.get(6)?,
                    enabled: row.get::<_, i64>(4)? != 0,
                    targets: Vec::new(),
                },
            ))
        })?;
        rows.collect::<Result<_, _>>()?
    };

    let mut routes = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT upstream_model_id, priority, enabled FROM route_targets
             WHERE route_id = ?1 ORDER BY priority, id",
        )?;
        for (route_id, mut route) in route_rows {
            let targets = stmt
                .query_map([route_id.as_str()], |row| {
                    Ok(SeedTarget {
                        upstream_model: row.get(0)?,
                        priority: row.get(1)?,
                        enabled: row.get::<_, i64>(2)? != 0,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            route.targets = targets
                .into_iter()
                .map(|mut target| {
                    if let Some(model_id) = model_names.get(&target.upstream_model) {
                        target.upstream_model = model_id.clone();
                    }
                    target
                })
                .collect();
            routes.push(route);
        }
    }

    let virtual_keys = {
        let mut stmt = conn.prepare(
            "SELECT key, name, enabled, quota_limit, quota_period
             FROM virtual_keys ORDER BY created_at, name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SeedVirtualKey {
                key: row.get(0)?,
                name: row.get(1)?,
                enabled: row.get::<_, i64>(2)? != 0,
                quota_limit: row.get(3)?,
                quota_period: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let seed = SeedFile {
        providers,
        upstream_models,
        routes,
        virtual_keys,
    };
    Ok(serde_json::to_string_pretty(&seed)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::settings::{get_settings, save_settings, Settings};
    use crate::db::{clear_business_data, open_in_memory};

    const SAMPLE: &str = r#"{
      "providers": [
        { "name": "OpenAI", "baseUrl": "https://api.openai.com/v1", "apiKey": "sk-x",
          "icon": "openai", "iconTint": "brand", "enabled": true },
        { "name": "Anthropic", "baseUrl": "https://api.anthropic.com/v1",
          "authScheme": "x-api-key", "protocol": "anthropic", "enabled": false }
      ],
      "upstreamModels": [
        { "provider": "OpenAI", "modelId": "gpt-4o", "displayName": "GPT-4o",
          "inputPrice": 2.5, "outputPrice": 10.0, "enabled": true }
      ],
      "routes": [
        { "alias": "lumen/main", "displayName": "Main", "enabled": true,
          "targets": [ { "upstreamModel": "gpt-4o", "priority": 0, "enabled": true } ] }
      ],
      "virtualKeys": [
        { "key": "sk-lumen-a", "name": "手机", "enabled": true,
          "quotaLimit": 5.0, "quotaPeriod": "monthly" }
      ]
    }"#;

    fn seed(db: &crate::db::Db) -> SeedFile {
        let seed: SeedFile = serde_json::from_str(SAMPLE).unwrap();
        let conn = db.lock().unwrap();
        merge_seed(&conn, &seed, true).unwrap();
        seed
    }

    #[test]
    fn export_roundtrips_imported_seed() {
        let db = open_in_memory().unwrap();
        seed(&db);
        let exported = {
            let conn = db.lock().unwrap();
            export_seed(&conn).unwrap()
        };
        let value: serde_json::Value = serde_json::from_str(&exported).unwrap();
        let providers = value["providers"].as_array().unwrap();
        assert_eq!(providers.len(), 2);
        assert_eq!(value["upstreamModels"].as_array().unwrap().len(), 1);
        assert_eq!(value["routes"].as_array().unwrap().len(), 1);
        assert_eq!(value["routes"][0]["targets"][0]["upstreamModel"], "gpt-4o");
        assert_eq!(value["virtualKeys"].as_array().unwrap().len(), 1);
        assert_eq!(value["virtualKeys"][0]["quotaLimit"], 5.0);
        let openai = providers.iter().find(|item| item["name"] == "OpenAI").unwrap();
        let anthropic = providers.iter().find(|item| item["name"] == "Anthropic").unwrap();
        assert_eq!(openai["iconTint"], "brand");
        assert_eq!(anthropic["iconTint"], "ink");
        assert_eq!(anthropic["enabled"], false);
        // 旧格式导入合成端点，导出统一为新格式。
        assert_eq!(openai["endpoints"][0]["protocol"], "openai");
        assert_eq!(openai["endpoints"][0]["baseUrl"], "https://api.openai.com/v1");
        assert_eq!(anthropic["endpoints"][0]["protocol"], "anthropic");
        assert_eq!(anthropic["endpoints"][0]["authScheme"], "x-api-key");
    }

    #[test]
    fn imports_new_format_endpoints() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let seed: SeedFile = serde_json::from_str(
            r#"{ "providers": [ { "name": "opencode-go", "apiKey": "sk-x",
                 "endpoints": [
                   { "protocol": "openai", "baseUrl": "https://go/v1" },
                   { "protocol": "anthropic", "baseUrl": "https://go/anthropic/v1", "authScheme": "x-api-key" }
                 ] } ] }"#,
        )
        .unwrap();
        merge_seed(&conn, &seed, true).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM provider_endpoints", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
        let anthropic_base: String = conn
            .query_row(
                "SELECT base_url FROM provider_endpoints WHERE protocol = 'anthropic'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(anthropic_base, "https://go/anthropic/v1");
    }

    #[test]
    fn clear_business_data_keeps_settings() {
        let db = open_in_memory().unwrap();
        seed(&db);
        {
            let conn = db.lock().unwrap();
            save_settings(
                &conn,
                &Settings {
                    port: 9999,
                    close_to_tray: false,
                    ..Settings::default()
                },
            )
            .unwrap();
            clear_business_data(&conn).unwrap();
            let remaining: i64 = ["providers", "upstream_models", "routes", "route_targets"]
                .iter()
                .map(|table| {
                    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap()
                })
                .sum();
            assert_eq!(remaining, 0);
            let settings = get_settings(&conn).unwrap();
            assert_eq!(settings.port, 9999);
            assert!(!settings.close_to_tray);
        }
    }

    #[test]
    fn merge_reimport_updates_without_duplicating() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let seed: SeedFile = serde_json::from_str(SAMPLE).unwrap();

        let first = merge_seed(&conn, &seed, true).unwrap();
        assert_eq!(first.providers.created, 2);
        assert_eq!(first.models.created, 1);
        assert_eq!(first.routes.created, 1);
        assert_eq!(first.virtual_keys.created, 1);

        let second = merge_seed(&conn, &seed, true).unwrap();
        assert_eq!(second.providers.created, 0);
        assert_eq!(second.providers.updated, 2);
        assert_eq!(second.models.updated, 1);
        assert_eq!(second.routes.updated, 1);
        assert_eq!(second.virtual_keys.updated, 1);

        let providers: i64 =
            conn.query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0)).unwrap();
        let targets: i64 =
            conn.query_row("SELECT COUNT(*) FROM route_targets", [], |row| row.get(0)).unwrap();
        assert_eq!(providers, 2);
        assert_eq!(targets, 1);
    }

    #[test]
    fn merge_preserves_entries_not_in_file() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let base: SeedFile = serde_json::from_str(SAMPLE).unwrap();
        merge_seed(&conn, &base, true).unwrap();

        let extra: SeedFile = serde_json::from_str(
            r#"{ "providers": [ { "name": "DeepSeek", "baseUrl": "https://api.deepseek.com" } ] }"#,
        )
        .unwrap();
        let summary = merge_seed(&conn, &extra, true).unwrap();
        assert_eq!(summary.providers.created, 1);

        let providers: i64 =
            conn.query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0)).unwrap();
        let openai: i64 = conn
            .query_row("SELECT COUNT(*) FROM providers WHERE name = 'OpenAI'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(providers, 3);
        assert_eq!(openai, 1);
    }

    #[test]
    fn partial_import_can_reference_existing_entities() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        // 首次导入建立 provider + model。
        let base: SeedFile = serde_json::from_str(SAMPLE).unwrap();
        merge_seed(&conn, &base, true).unwrap();

        // 只给一条 route，引用本次文件之外、库中已存在的模型。
        let route_only: SeedFile = serde_json::from_str(
            r#"{ "routes": [ { "alias": "lumen/extra", "protocol": "openai",
                 "targets": [ { "upstreamModel": "gpt-4o", "priority": 0, "enabled": true } ] } ] }"#,
        )
        .unwrap();
        assert_eq!(
            merge_seed(&conn, &route_only, true).unwrap().routes.created,
            1
        );

        // 只给一个 model，引用本次文件之外、库中已存在的 provider。
        let model_only: SeedFile = serde_json::from_str(
            r#"{ "upstreamModels": [ { "provider": "OpenAI", "modelId": "gpt-4o-mini" } ] }"#,
        )
        .unwrap();
        assert_eq!(
            merge_seed(&conn, &model_only, true).unwrap().models.created,
            1
        );

        let routes: i64 = conn
            .query_row("SELECT COUNT(*) FROM routes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(routes, 2);
    }

    #[test]
    fn merge_upserts_virtual_keys_by_key() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let first: SeedFile = serde_json::from_str(
            r#"{ "virtualKeys": [ { "key": "sk-lumen-a", "name": "手机", "enabled": true,
                 "quotaLimit": 5.0, "quotaPeriod": "monthly" } ] }"#,
        )
        .unwrap();
        assert_eq!(merge_seed(&conn, &first, true).unwrap().virtual_keys.created, 1);

        let second: SeedFile = serde_json::from_str(
            r#"{ "virtualKeys": [ { "key": "sk-lumen-a", "name": "平板", "enabled": false,
                 "quotaLimit": 12.5, "quotaPeriod": "weekly" } ] }"#,
        )
        .unwrap();
        assert_eq!(merge_seed(&conn, &second, true).unwrap().virtual_keys.updated, 1);

        let (name, enabled, limit, period): (String, i64, f64, String) = conn
            .query_row(
                "SELECT name, enabled, quota_limit, quota_period FROM virtual_keys WHERE key = 'sk-lumen-a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(name, "平板");
        assert_eq!(enabled, 0);
        assert_eq!(limit, 12.5);
        assert_eq!(period, "weekly");
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM virtual_keys", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn merge_rejects_dangling_reference_and_rolls_back() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let bad: SeedFile = serde_json::from_str(
            r#"{ "providers": [ { "name": "OpenAI", "baseUrl": "https://x" } ],
                 "upstreamModels": [ { "provider": "Ghost", "modelId": "m", "displayName": "M" } ] }"#,
        )
        .unwrap();
        assert!(merge_seed(&conn, &bad, true).is_err());
        let providers: i64 =
            conn.query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0)).unwrap();
        assert_eq!(providers, 0);
    }

    #[test]
    fn preview_merge_rolls_back() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let seed: SeedFile = serde_json::from_str(SAMPLE).unwrap();
        let preview = merge_seed(&conn, &seed, false).unwrap();
        assert_eq!(preview.providers.created, 2);
        let providers: i64 =
            conn.query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0)).unwrap();
        assert_eq!(providers, 0);
    }
}
