use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection};
use serde::Deserialize;

use super::models::{AUTH_BEARER, ICON_TINT_INK, PROTOCOL_OPENAI};
use crate::error::AppError;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeedFile {
    #[serde(default)]
    providers: Vec<SeedProvider>,
    #[serde(default)]
    upstream_models: Vec<SeedModel>,
    #[serde(default)]
    routes: Vec<SeedRoute>,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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
