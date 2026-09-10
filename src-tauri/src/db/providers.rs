use rusqlite::{params, Connection};

use super::models::{Provider, ProviderInput, UpstreamModel, UpstreamModelInput};
use crate::error::AppError;

pub fn list_providers(conn: &Connection) -> Result<Vec<Provider>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM providers ORDER BY created_at ASC")?;
    let rows = stmt.query_map([], Provider::from_row)?;
    let mut providers = Vec::new();
    for row in rows {
        providers.push(row?);
    }
    Ok(providers)
}

pub fn get_provider(conn: &Connection, id: &str) -> Result<Option<Provider>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM providers WHERE id = ?1")?;
    let mut rows = stmt.query_map([id], Provider::from_row)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn save_provider(conn: &Connection, input: &ProviderInput) -> Result<Provider, AppError> {
    let id = input
        .id
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let created_at = chrono::Utc::now().to_rfc3339();
    let extra_headers = serde_json::to_string(&input.extra_headers)?;
    conn.execute(
        "INSERT INTO providers
            (id, name, base_url, api_key, auth_scheme, protocol, extra_headers, enabled, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            base_url = excluded.base_url,
            api_key = excluded.api_key,
            auth_scheme = excluded.auth_scheme,
            protocol = excluded.protocol,
            extra_headers = excluded.extra_headers,
            enabled = excluded.enabled",
        params![
            id,
            input.name,
            input.base_url,
            input.api_key,
            input.auth_scheme,
            input.protocol,
            extra_headers,
            input.enabled as i64,
            created_at,
        ],
    )?;
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

pub fn save_upstream_model(
    conn: &Connection,
    input: &UpstreamModelInput,
) -> Result<UpstreamModel, AppError> {
    let id = input
        .id
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    conn.execute(
        "INSERT INTO upstream_models
            (id, provider_id, model_id, display_name, input_price, output_price,
             cache_read_price, cache_creation_price, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            provider_id = excluded.provider_id,
            model_id = excluded.model_id,
            display_name = excluded.display_name,
            input_price = excluded.input_price,
            output_price = excluded.output_price,
            cache_read_price = excluded.cache_read_price,
            cache_creation_price = excluded.cache_creation_price,
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
