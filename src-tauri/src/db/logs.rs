use chrono::{DateTime, Utc};
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
    /// 时间下界（含），RFC3339；返回 occurred_at >= from 的记录。
    #[serde(default)]
    pub from: Option<String>,
    /// 时间上界（不含），RFC3339；返回 occurred_at < to 的记录。
    #[serde(default)]
    pub to: Option<String>,
    /// 用量可信度。取值 UsageSource（provider / estimated / partial / missing），
    /// 或 "unreliable" 表示 missing 与 partial 两者。
    #[serde(default)]
    pub usage_source: Option<String>,
    /// 只保留需要关注的记录：status = 'error' 或 usage_source 为 missing / partial。
    #[serde(default)]
    pub attention_only: Option<bool>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

/// 过滤条件共用的 WHERE 子句（?1..?7），list 与 count 必须保持一致。
const FILTER_WHERE: &str = "WHERE (?1 IS NULL OR route_alias = ?1)
           AND (?2 IS NULL OR status = ?2)
           AND (?3 IS NULL OR (
                COALESCE(route_alias, '') || ' ' ||
                COALESCE(upstream_model_name, '') || ' ' ||
                kind || ' ' ||
                CAST(total_tokens AS TEXT)
           ) LIKE '%' || ?3 || '%')
           AND (?4 IS NULL OR occurred_at >= ?4)
           AND (?5 IS NULL OR occurred_at < ?5)
           AND (?6 IS NULL
                OR (?6 = 'unreliable' AND usage_source IN ('missing', 'partial'))
                OR (?6 <> 'unreliable' AND usage_source = ?6))
           AND (?7 IS NULL OR ?7 = 0
                OR status = 'error' OR usage_source IN ('missing', 'partial'))";

/// 绑定到 FILTER_WHERE 的规范化参数：空串一律折算为 NULL。
struct PreparedFilter {
    route_alias: Option<String>,
    status: Option<String>,
    query: Option<String>,
    from: Option<String>,
    to: Option<String>,
    usage_source: Option<String>,
    attention_only: Option<bool>,
}

fn non_empty(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

/// 把 RFC3339 时间统一折算成 Utc 的规范形式，保证字符串序与时间序一致。
fn canonical_time(value: &Option<String>) -> Result<Option<String>, AppError> {
    match non_empty(value) {
        None => Ok(None),
        Some(raw) => DateTime::parse_from_rfc3339(&raw)
            .map(|moment| moment.with_timezone(&Utc).to_rfc3339())
            .map(Some)
            .map_err(|_| AppError::message(format!("无法解析时间：{raw}"))),
    }
}

impl PreparedFilter {
    fn from_filter(filter: &LogFilter) -> Result<Self, AppError> {
        Ok(Self {
            route_alias: non_empty(&filter.route_alias),
            status: non_empty(&filter.status),
            query: non_empty(&filter.query),
            from: canonical_time(&filter.from)?,
            to: canonical_time(&filter.to)?,
            usage_source: non_empty(&filter.usage_source),
            attention_only: filter.attention_only.filter(|value| *value),
        })
    }
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
    let prepared = PreparedFilter::from_filter(filter)?;
    let sql = format!(
        "SELECT * FROM request_logs {FILTER_WHERE} ORDER BY occurred_at DESC LIMIT ?8 OFFSET ?9"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![
            prepared.route_alias,
            prepared.status,
            prepared.query,
            prepared.from,
            prepared.to,
            prepared.usage_source,
            prepared.attention_only,
            limit,
            offset,
        ],
        RequestLog::from_row,
    )?;
    let mut logs = Vec::new();
    for row in rows {
        logs.push(row?);
    }
    Ok(logs)
}

pub fn count_logs(conn: &Connection, filter: &LogFilter) -> Result<i64, AppError> {
    let prepared = PreparedFilter::from_filter(filter)?;
    let sql = format!("SELECT COUNT(*) FROM request_logs {FILTER_WHERE}");
    let count = conn.query_row(
        &sql,
        params![
            prepared.route_alias,
            prepared.status,
            prepared.query,
            prepared.from,
            prepared.to,
            prepared.usage_source,
            prepared.attention_only,
        ],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn log(n: i32, occurred_at: &str, status: &str, usage_source: &str) -> RequestLog {
        RequestLog {
            id: format!("log-{n}"),
            occurred_at: occurred_at.to_string(),
            endpoint: "/v1/chat/completions".into(),
            method: "POST".into(),
            route_alias: Some("lumen/main".into()),
            route_id: Some("r1".into()),
            upstream_model_id: Some("m1".into()),
            upstream_model_name: Some(format!("Model {n}")),
            model_real: Some("gpt-4o".into()),
            provider_id: Some("p1".into()),
            virtual_key_id: None,
            kind: "chat".into(),
            input_tokens: 100,
            output_tokens: 20,
            total_tokens: 120,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            reasoning_tokens: 0,
            cost: 0.01,
            usage_source: usage_source.into(),
            status: status.into(),
            http_status: if status == "error" { Some(500) } else { Some(200) },
            latency_ms: Some(120),
            error_message: if status == "error" { Some("上游超时".into()) } else { None },
            request_id: Some(format!("req-{n}")),
            is_stream: false,
        }
    }

    fn fixture() -> crate::db::Db {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        insert_log(&conn, &log(1, "2026-09-01T10:00:00+00:00", "success", "provider")).unwrap();
        insert_log(&conn, &log(2, "2026-09-05T10:00:00+00:00", "error", "missing")).unwrap();
        insert_log(&conn, &log(3, "2026-09-10T10:00:00+00:00", "success", "partial")).unwrap();
        drop(conn);
        db
    }

    #[test]
    fn filters_by_time_range() {
        let db = fixture();
        let conn = db.lock().unwrap();
        let filter = LogFilter {
            from: Some("2026-09-01T00:00:00Z".into()),
            to: Some("2026-09-06T00:00:00Z".into()),
            ..Default::default()
        };
        let rows = list_logs(&conn, &filter).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(count_logs(&conn, &filter).unwrap(), 2);
    }

    #[test]
    fn filters_unreliable_usage() {
        let db = fixture();
        let conn = db.lock().unwrap();
        let filter = LogFilter {
            usage_source: Some("unreliable".into()),
            ..Default::default()
        };
        let ids: Vec<String> = list_logs(&conn, &filter).unwrap().into_iter().map(|l| l.id).collect();
        assert_eq!(ids, vec!["log-3", "log-2"]);
    }

    #[test]
    fn attention_only_keeps_failures_and_unreliable() {
        let db = fixture();
        let conn = db.lock().unwrap();
        let filter = LogFilter {
            attention_only: Some(true),
            ..Default::default()
        };
        let ids: Vec<String> = list_logs(&conn, &filter).unwrap().into_iter().map(|l| l.id).collect();
        assert_eq!(ids, vec!["log-3", "log-2"]);
        assert_eq!(count_logs(&conn, &filter).unwrap(), 2);
    }

    #[test]
    fn filters_exact_usage_source_and_status() {
        let db = fixture();
        let conn = db.lock().unwrap();
        let filter = LogFilter {
            status: Some("error".into()),
            usage_source: Some("missing".into()),
            ..Default::default()
        };
        let rows = list_logs(&conn, &filter).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "log-2");
    }
}
