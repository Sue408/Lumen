use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::models::{AUTH_BEARER, ICON_TINT_INK, PROTOCOL_OPENAI};
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
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeedProvider {
    name: String,
    base_url: String,
    #[serde(default)]
    api_key: String,
    #[serde(default = "default_auth")]
    auth_scheme: String,
    #[serde(default = "default_protocol")]
    protocol: String,
    #[serde(default)]
    extra_headers: BTreeMap<String, String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default = "default_icon_tint")]
    icon_tint: String,
    #[serde(default = "default_true")]
    enabled: bool,
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

fn default_auth() -> String {
    AUTH_BEARER.to_string()
}

fn default_protocol() -> String {
    PROTOCOL_OPENAI.to_string()
}

fn default_icon_tint() -> String {
    ICON_TINT_INK.to_string()
}

fn default_true() -> bool {
    true
}

/// 首次启动（库为空）时从 `path` 导入示例配置，返回导入的路由数量。
/// 文件不存在时静默跳过。
pub fn seed_if_empty(conn: &Connection, path: &Path) -> Result<usize, AppError> {
    let existing: i64 =
        conn.query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0))?;
    let route_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM routes", [], |row| row.get(0))?;
    if existing > 0 || route_count > 0 {
        return Ok(0);
    }
    if !path.exists() {
        return Ok(0);
    }
    let raw = std::fs::read_to_string(path)?;
    let seed: SeedFile = serde_json::from_str(&raw)?;
    import(conn, &seed)
}

fn import(conn: &Connection, seed: &SeedFile) -> Result<usize, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    let mut provider_ids: HashMap<String, String> = HashMap::new();
    for provider in &seed.providers {
        let id = uuid::Uuid::new_v4().to_string();
        let extra_headers = serde_json::to_string(&provider.extra_headers)?;
        tx.execute(
            "INSERT INTO providers
                (id, name, base_url, api_key, auth_scheme, protocol, extra_headers, icon, icon_tint, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id,
                provider.name,
                provider.base_url,
                provider.api_key,
                provider.auth_scheme,
                provider.protocol,
                extra_headers,
                provider.icon,
                provider.icon_tint,
                provider.enabled as i64,
                now,
            ],
        )?;
        provider_ids.insert(provider.name.clone(), id);
    }

    let mut model_ids: HashMap<String, String> = HashMap::new();
    for model in &seed.upstream_models {
        let provider_id = provider_ids.get(&model.provider).ok_or_else(|| {
            AppError::message(format!("seed 引用了不存在的提供商：{}", model.provider))
        })?;
        let id = uuid::Uuid::new_v4().to_string();
        let display_name = if model.display_name.is_empty() {
            model.model_id.clone()
        } else {
            model.display_name.clone()
        };
        tx.execute(
            "INSERT INTO upstream_models
                (id, provider_id, model_id, display_name, input_price, output_price,
                 cache_read_price, cache_creation_price, icon, icon_tint, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id,
                provider_id,
                model.model_id,
                display_name,
                model.input_price,
                model.output_price,
                model.cache_read_price,
                model.cache_creation_price,
                model.icon,
                model.icon_tint,
                model.enabled as i64,
            ],
        )?;
        model_ids.insert(model.model_id.clone(), id.clone());
        model_ids.entry(display_name).or_insert(id);
    }

    let mut imported = 0;
    for route in &seed.routes {
        let route_id = uuid::Uuid::new_v4().to_string();
        let display_name = if route.display_name.is_empty() {
            route.alias.clone()
        } else {
            route.display_name.clone()
        };
        tx.execute(
            "INSERT INTO routes (id, alias, display_name, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![route_id, route.alias, display_name, route.enabled as i64, now],
        )?;
        for target in &route.targets {
            let model_id = model_ids.get(&target.upstream_model).ok_or_else(|| {
                AppError::message(format!(
                    "seed 路由 {} 引用了不存在的上游模型：{}",
                    route.alias, target.upstream_model
                ))
            })?;
            tx.execute(
                "INSERT INTO route_targets (id, route_id, upstream_model_id, priority, enabled)
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
        imported += 1;
    }
    tx.commit()?;
    Ok(imported)
}

/// 把当前配置导出为 seed JSON 文本，格式与导入一致（不含日志、密钥偏好等运行时数据）。
pub fn export_seed(conn: &Connection) -> Result<String, AppError> {
    let mut providers = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, api_key, auth_scheme, protocol, extra_headers, icon, icon_tint, enabled
             FROM providers ORDER BY created_at, name",
        )?;
        let rows = stmt.query_map([], |row| {
            let extra_raw: String = row.get(6)?;
            Ok(SeedProvider {
                name: row.get(1)?,
                base_url: row.get(2)?,
                api_key: row.get(3)?,
                auth_scheme: row.get(4)?,
                protocol: row.get(5)?,
                extra_headers: serde_json::from_str(&extra_raw).unwrap_or_default(),
                icon: row.get(7)?,
                icon_tint: row.get(8)?,
                enabled: row.get::<_, i64>(9)? != 0,
            })
        })?;
        for row in rows {
            providers.push(row?);
        }
    }

    let mut model_names: HashMap<String, String> = HashMap::new();
    let mut upstream_models = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT m.id, p.name, m.model_id, m.display_name, m.input_price, m.output_price,
                    m.cache_read_price, m.cache_creation_price, m.icon, m.icon_tint, m.enabled
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
                    icon: row.get(8)?,
                    icon_tint: row.get(9)?,
                    enabled: row.get::<_, i64>(10)? != 0,
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
            "SELECT id, alias, display_name, enabled FROM routes ORDER BY created_at, alias",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                SeedRoute {
                    alias: row.get(1)?,
                    display_name: row.get(2)?,
                    enabled: row.get::<_, i64>(3)? != 0,
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

    let seed = SeedFile {
        providers,
        upstream_models,
        routes,
    };
    Ok(serde_json::to_string_pretty(&seed)?)
}

pub fn seed_path_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();
    // 开发态：源码树位置，编译期固化，最稳。
    candidates.push(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lumen.seed.json"));
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("lumen.seed.json"));
        candidates.push(cwd.join("src-tauri").join("lumen.seed.json"));
    }
    candidates
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
      ]
    }"#;

    fn seed(db: &crate::db::Db) -> SeedFile {
        let seed: SeedFile = serde_json::from_str(SAMPLE).unwrap();
        let conn = db.lock().unwrap();
        import(&conn, &seed).unwrap();
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
        let openai = providers.iter().find(|item| item["name"] == "OpenAI").unwrap();
        let anthropic = providers.iter().find(|item| item["name"] == "Anthropic").unwrap();
        assert_eq!(openai["iconTint"], "brand");
        assert_eq!(anthropic["iconTint"], "ink");
        assert_eq!(anthropic["enabled"], false);
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
}
