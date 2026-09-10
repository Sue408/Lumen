use rusqlite::{params, Connection};
use serde::Deserialize;

use super::models::RequestLog;
use crate::error::AppError;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFilter {
    #[serde(default)]
    pub route_alias: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

pub fn insert_log(conn: &Connection, log: &RequestLog) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO request_logs (
            id, occurred_at, endpoint, method, route_alias, route_id,
            upstream_model_id, upstream_model_name, model_real, provider_id, virtual_key_id,
            kind, input_tokens, output_tokens, total_tokens,
            cache_read_tokens, cache_creation_tokens, reasoning_tokens,
            cost, usage_source, status, http_status, latency_ms, error_message, request_id, is_stream
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
            ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26
         )",
        params![
            log.id,
            log.occurred_at,
            log.endpoint,
            log.method,
            log.route_alias,
            log.route_id,
            log.upstream_model_id,
            log.upstream_model_name,
            log.model_real,
            log.provider_id,
            log.virtual_key_id,
            log.kind,
            log.input_tokens,
            log.output_tokens,
            log.total_tokens,
            log.cache_read_tokens,
            log.cache_creation_tokens,
            log.reasoning_tokens,
            log.cost,
            log.usage_source,
            log.status,
            log.http_status,
            log.latency_ms,
            log.error_message,
            log.request_id,
            log.is_stream as i64,
        ],
    )?;
    Ok(())
}

pub fn list_logs(conn: &Connection, filter: &LogFilter) -> Result<Vec<RequestLog>, AppError> {
    let limit = filter.limit.unwrap_or(200).clamp(1, 1000);
    let offset = filter.offset.unwrap_or(0).max(0);
    let mut stmt = conn.prepare(
        "SELECT * FROM request_logs
         WHERE (?1 IS NULL OR route_alias = ?1)
           AND (?2 IS NULL OR status = ?2)
           AND (?3 IS NULL OR (
                COALESCE(route_alias, '') || ' ' ||
                COALESCE(upstream_model_name, '') || ' ' ||
                kind || ' ' ||
                CAST(total_tokens AS TEXT)
           ) LIKE '%' || ?3 || '%')
         ORDER BY occurred_at DESC
         LIMIT ?4 OFFSET ?5",
    )?;
    let query = filter
        .query
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let rows = stmt.query_map(
        params![filter.route_alias, filter.status, query, limit, offset],
        RequestLog::from_row,
    )?;
    let mut logs = Vec::new();
    for row in rows {
        logs.push(row?);
    }
    Ok(logs)
}

pub fn count_logs(conn: &Connection, filter: &LogFilter) -> Result<i64, AppError> {
    let query = filter
        .query
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM request_logs
         WHERE (?1 IS NULL OR route_alias = ?1)
           AND (?2 IS NULL OR status = ?2)
           AND (?3 IS NULL OR (
                COALESCE(route_alias, '') || ' ' ||
                COALESCE(upstream_model_name, '') || ' ' ||
                kind || ' ' ||
                CAST(total_tokens AS TEXT)
           ) LIKE '%' || ?3 || '%')",
        params![filter.route_alias, filter.status, query],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// 日志中出现过的别名，供筛选下拉使用。
pub fn list_log_aliases(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT route_alias FROM request_logs
         WHERE route_alias IS NOT NULL
         ORDER BY route_alias ASC",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut aliases = Vec::new();
    for row in rows {
        aliases.push(row?);
    }
    Ok(aliases)
}
