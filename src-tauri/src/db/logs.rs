use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

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
    /// 会话等值过滤；与列表页的会话筛选对应。
    #[serde(default)]
    pub session_id: Option<String>,
    /// 只保留需要关注的记录：status = 'error' 或 usage_source 为 missing / partial。
    #[serde(default)]
    pub attention_only: Option<bool>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

fn non_empty(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

/// 转义 LIKE 的通配符，避免用户输入的 `%` / `_` 被当成模式。调用方以 `ESCAPE '\'` 配套。
fn escape_like(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
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

/// 把过滤条件编译成「只含激活条件」的 WHERE 子句和绑定值。
///
/// 不能用 `?N IS NULL OR col = ?N` 的模板——那样规划器看不到常量的真实取值，
/// 任何查询都选不了索引，连「无过滤的 COUNT」都会退化成全表扫描。
/// 动态拼接后，无过滤的 COUNT 走 SQLite 的快路径，有过滤的按列走对应索引。
fn build_where(filter: &LogFilter) -> Result<(String, Vec<Box<dyn rusqlite::ToSql>>), AppError> {
    let mut clauses: Vec<&str> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(alias) = non_empty(&filter.route_alias) {
        clauses.push("route_alias = ?");
        params.push(Box::new(alias));
    }
    if let Some(status) = non_empty(&filter.status) {
        clauses.push("status = ?");
        params.push(Box::new(status));
    }
    if let Some(query) = non_empty(&filter.query) {
        clauses.push(
            "(COALESCE(route_alias, '') || ' ' || COALESCE(upstream_model_name, '') || ' ' || kind || ' ' || CAST(total_tokens AS TEXT)) LIKE ? ESCAPE '\\'",
        );
        params.push(Box::new(format!("%{}%", escape_like(&query))));
    }
    if let Some(from) = canonical_time(&filter.from)? {
        clauses.push("occurred_at >= ?");
        params.push(Box::new(from));
    }
    if let Some(to) = canonical_time(&filter.to)? {
        clauses.push("occurred_at < ?");
        params.push(Box::new(to));
    }
    if let Some(source) = non_empty(&filter.usage_source) {
        if source == "unreliable" {
            clauses.push("usage_source IN ('missing', 'partial')");
        } else {
            clauses.push("usage_source = ?");
            params.push(Box::new(source));
        }
    }
    if let Some(session) = non_empty(&filter.session_id) {
        clauses.push("session_id = ?");
        params.push(Box::new(session));
    }
    if filter.attention_only == Some(true) {
        clauses.push("(status = 'error' OR usage_source IN ('missing', 'partial'))");
    }

    let clause = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    Ok((clause, params))
}

pub fn insert_log(conn: &Connection, log: &RequestLog) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO request_logs (
            id, occurred_at, endpoint, method, route_alias, route_id,
            upstream_model_id, upstream_model_name, model_real, provider_id, virtual_key_id,
            kind, input_tokens, output_tokens, total_tokens,
            cache_read_tokens, cache_creation_tokens, cache_read_in_input, reasoning_tokens,
            cost, usage_source, status, http_status, latency_ms, error_message, request_id, is_stream,
            attempt_index, session_id
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
            ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29
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
            log.cache_read_in_input as i64,
            log.reasoning_tokens,
            log.cost,
            log.usage_source,
            log.status,
            log.http_status,
            log.latency_ms,
            log.error_message,
            log.request_id,
            log.is_stream as i64,
            log.attempt_index,
            log.session_id,
        ],
    )?;
    Ok(())
}

pub fn list_logs(conn: &Connection, filter: &LogFilter) -> Result<Vec<RequestLog>, AppError> {
    let limit = filter.limit.unwrap_or(200).clamp(1, 1000);
    let offset = filter.offset.unwrap_or(0).max(0);
    let (where_sql, params) = build_where(filter)?;
    // 「待处理」与用量口径是 OR / IN 谓词。规划器会为它们选 MULTI-INDEX OR，
    // 代价是丢掉时间序、被迫排序；而列表本来就「按时间倒序、够数即停」，那更慢。
    // 这两类显式钉在 occurred 索引上；status / 别名 / 时间范围仍交给规划器挑索引。
    let index_hint = if filter.attention_only == Some(true) || non_empty(&filter.usage_source).is_some()
    {
        " INDEXED BY idx_request_logs_occurred"
    } else {
        ""
    };
    let sql = format!(
        "SELECT * FROM request_logs{index_hint}{where_sql} ORDER BY occurred_at DESC LIMIT ? OFFSET ?"
    );
    let mut bound: Vec<&dyn rusqlite::ToSql> = params.iter().map(|item| item.as_ref()).collect();
    bound.push(&limit);
    bound.push(&offset);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(bound), RequestLog::from_row)?;
    let mut logs = Vec::new();
    for row in rows {
        logs.push(row?);
    }
    Ok(logs)
}

pub fn count_logs(conn: &Connection, filter: &LogFilter) -> Result<i64, AppError> {
    let (where_sql, params) = build_where(filter)?;
    let sql = format!("SELECT COUNT(*) FROM request_logs{where_sql}");
    let count = conn.query_row(
        &sql,
        rusqlite::params_from_iter(params.iter().map(|item| item.as_ref())),
        |row| row.get(0),
    )?;
    Ok(count)
}

/// 日志页顶部的三个口径计数（总 / 失败 / 用量存疑）。
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSummary {
    pub all: i64,
    pub failed: i64,
    pub unreliable: i64,
}

/// 三个口径各算一次 COUNT。三条都命中索引，比「一条 SUM 扫全表」快一个数量级。
pub fn summarize_logs(conn: &Connection, filter: &LogFilter) -> Result<LogSummary, AppError> {
    let failed = LogFilter {
        status: Some("error".into()),
        ..filter.clone()
    };
    let unreliable = LogFilter {
        usage_source: Some("unreliable".into()),
        ..filter.clone()
    };
    Ok(LogSummary {
        all: count_logs(conn, filter)?,
        failed: count_logs(conn, &failed)?,
        unreliable: count_logs(conn, &unreliable)?,
    })
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

/// 会话维度的聚合结果。会话不是实体：这里全部由 `request_logs` 按
/// `(virtual_key_id, session_id)` 聚合得到。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: String,
    pub virtual_key_id: Option<String>,
    /// 首次出现 / 最近出现（`MIN` / `MAX(occurred_at)`）。
    pub first_seen: String,
    pub last_seen: String,
    pub requests: i64,
    pub total_tokens: i64,
    pub cost: f64,
    /// 该会话涉及过的去重上游模型名（忽略 NULL）。
    pub models: Vec<String>,
}

fn split_models(raw: Option<String>) -> Vec<String> {
    raw.map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

/// 按 `(virtual_key_id, session_id)` 聚合会话视图，`session_id IS NULL` 的记录不入结果。
/// 按最近出现倒序，`limit` 默认 50（上限 1000）。
pub fn list_sessions(
    conn: &Connection,
    filter: &LogFilter,
) -> Result<Vec<SessionSummary>, AppError> {
    let limit = filter.limit.unwrap_or(50).clamp(1, 1000);
    let (where_sql, params) = build_where(filter)?;
    let where_sql = if where_sql.is_empty() {
        " WHERE session_id IS NOT NULL".to_string()
    } else {
        format!("{where_sql} AND session_id IS NOT NULL")
    };
    let sql = format!(
        "SELECT
            session_id,
            virtual_key_id,
            MIN(occurred_at) AS first_seen,
            MAX(occurred_at) AS last_seen,
            COUNT(*)         AS requests,
            COALESCE(SUM(total_tokens), 0) AS total_tokens,
            COALESCE(SUM(cost), 0)         AS cost,
            GROUP_CONCAT(DISTINCT upstream_model_name) AS models
         FROM request_logs{where_sql}
         GROUP BY virtual_key_id, session_id
         ORDER BY last_seen DESC
         LIMIT ?"
    );
    let mut bound: Vec<&dyn rusqlite::ToSql> = params.iter().map(|item| item.as_ref()).collect();
    bound.push(&limit);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(bound), |row| {
        Ok(SessionSummary {
            session_id: row.get("session_id")?,
            virtual_key_id: row.get("virtual_key_id")?,
            first_seen: row.get("first_seen")?,
            last_seen: row.get("last_seen")?,
            requests: row.get("requests")?,
            total_tokens: row.get("total_tokens")?,
            cost: row.get("cost")?,
            models: split_models(row.get("models")?),
        })
    })?;
    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(row?);
    }
    Ok(sessions)
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
            cache_read_in_input: false,
            reasoning_tokens: 0,
            cost: 0.01,
            usage_source: usage_source.into(),
            status: status.into(),
            http_status: if status == "error" { Some(500) } else { Some(200) },
            latency_ms: Some(120),
            error_message: if status == "error" { Some("上游超时".into()) } else { None },
            request_id: Some(format!("req-{n}")),
            is_stream: false,
            attempt_index: 0,
            session_id: None,
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
    fn persists_cache_read_boundary() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let mut with_input = log(1, "2026-09-01T10:00:00+00:00", "success", "provider");
        with_input.cache_read_tokens = 60;
        with_input.cache_read_in_input = true;
        insert_log(&conn, &with_input).unwrap();
        let mut without_input = log(2, "2026-09-01T11:00:00+00:00", "success", "provider");
        without_input.cache_read_tokens = 60;
        without_input.cache_read_in_input = false;
        insert_log(&conn, &without_input).unwrap();

        let rows = list_logs(&conn, &LogFilter::default()).unwrap();
        let by_id: std::collections::HashMap<_, _> =
            rows.into_iter().map(|row| (row.id.clone(), row)).collect();
        assert!(by_id["log-1"].cache_read_in_input);
        assert!(!by_id["log-2"].cache_read_in_input);
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

    #[test]
    fn query_treats_wildcards_as_literals() {
        let db = fixture();
        let conn = db.lock().unwrap();
        // 字面量 `%` 不应被当作 LIKE 通配符匹配全部记录。
        let none = list_logs(
            &conn,
            &LogFilter {
                query: Some("%".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(none.is_empty());

        // 正常子串仍能命中。
        let hit = list_logs(
            &conn,
            &LogFilter {
                query: Some("Model".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(hit.len(), 3);
    }

    fn session_log(
        n: i32,
        occurred_at: &str,
        session: Option<&str>,
        key: Option<&str>,
        cost: f64,
        model: &str,
    ) -> RequestLog {
        let mut entry = log(n, occurred_at, "success", "provider");
        entry.session_id = session.map(str::to_string);
        entry.virtual_key_id = key.map(str::to_string);
        entry.cost = cost;
        entry.upstream_model_name = Some(model.to_string());
        entry
    }

    #[test]
    fn sessions_aggregate_by_key_and_session() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let rows = [
            session_log(1, "2026-09-01T10:00:00+00:00", Some("ses_a"), Some("k1"), 0.10, "Model X"),
            session_log(2, "2026-09-02T10:00:00+00:00", Some("ses_a"), Some("k1"), 0.24, "Model Y"),
            session_log(3, "2026-09-03T10:00:00+00:00", Some("ses_b"), Some("k1"), 0.05, "Model X"),
            session_log(4, "2026-09-04T10:00:00+00:00", None, Some("k1"), 0.99, "Model X"),
        ];
        for entry in &rows {
            insert_log(&conn, entry).unwrap();
        }

        let sessions = list_sessions(&conn, &LogFilter::default()).unwrap();
        assert_eq!(sessions.len(), 2, "NULL 会话不进结果");
        assert_eq!(sessions[0].session_id, "ses_b", "按最近出现倒序");

        let a = sessions.iter().find(|s| s.session_id == "ses_a").unwrap();
        assert_eq!(a.virtual_key_id.as_deref(), Some("k1"));
        assert_eq!(a.requests, 2);
        assert_eq!(a.first_seen, "2026-09-01T10:00:00+00:00");
        assert_eq!(a.last_seen, "2026-09-02T10:00:00+00:00");
        assert!((a.cost - 0.34).abs() < 1e-9, "cost was {}", a.cost);
        let mut models = a.models.clone();
        models.sort();
        assert_eq!(models, vec!["Model X".to_string(), "Model Y".to_string()]);
    }

    #[test]
    fn logs_filter_by_session() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        insert_log(
            &conn,
            &session_log(1, "2026-09-01T10:00:00+00:00", Some("ses_a"), Some("k1"), 0.1, "M"),
        )
        .unwrap();
        insert_log(
            &conn,
            &session_log(2, "2026-09-02T10:00:00+00:00", Some("ses_b"), Some("k1"), 0.2, "M"),
        )
        .unwrap();

        let filter = LogFilter {
            session_id: Some("ses_a".into()),
            ..Default::default()
        };
        let rows = list_logs(&conn, &filter).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].session_id.as_deref(), Some("ses_a"));
        assert_eq!(count_logs(&conn, &filter).unwrap(), 1);
    }

    /// 手动跑的粗略基准：`cargo test bench_logs -- --nocapture --ignored`。
    #[test]
    #[ignore]
    fn bench_logs() {
        use crate::db::demo::{inject_demo, DemoScenario};

        let path = std::env::temp_dir().join("lumen-bench-logs.db");
        let _ = std::fs::remove_file(&path);
        let db = crate::db::open(&path).unwrap();
        {
            let conn = db.lock().unwrap();
            inject_demo(&conn, DemoScenario::Rich).unwrap();
        }
        let conn = db.lock().unwrap();
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))
            .unwrap();
        println!("TOTAL ROWS: {total}");

        let cases: Vec<(&str, LogFilter)> = vec![
            (
                "list attention (limit 100)",
                LogFilter {
                    attention_only: Some(true),
                    limit: Some(100),
                    ..Default::default()
                },
            ),
            (
                "count attention",
                LogFilter {
                    attention_only: Some(true),
                    ..Default::default()
                },
            ),
            ("count all", LogFilter::default()),
            (
                "count failed",
                LogFilter {
                    status: Some("error".into()),
                    ..Default::default()
                },
            ),
            (
                "count unreliable",
                LogFilter {
                    usage_source: Some("unreliable".into()),
                    ..Default::default()
                },
            ),
        ];

        for (name, filter) in &cases {
            let t = std::time::Instant::now();
            let rows = list_logs(&conn, filter).unwrap();
            let list_ms = t.elapsed();

            let t = std::time::Instant::now();
            let count = count_logs(&conn, filter).unwrap();
            let count_ms = t.elapsed();

            println!(
                "{name}: list {:?} ({} rows) / count {:?} ({count})",
                list_ms,
                rows.len(),
                count_ms
            );
        }

        let t = std::time::Instant::now();
        let aliases = list_log_aliases(&conn).unwrap();
        println!("list_aliases: {:?} ({} aliases)", t.elapsed(), aliases.len());

        // 设计依据：
        //  - 摘要用「3 条索引 COUNT」而非「一条 SUM 扫全表」——后者要全表扫；
        //  - 列表在 OR / IN 谓词下显式钉时间索引，否则规划器选 MULTI-INDEX OR、
        //    丢掉时间序被迫排序，反而更慢。
        for (name, sql) in [
            (
                "summary one scan (rejected)",
                "SELECT COUNT(*), SUM(status = 'error'), SUM(usage_source IN ('missing','partial')) FROM request_logs",
            ),
            (
                "list attention (planner, rejected)",
                "SELECT id FROM request_logs WHERE status = 'error' OR usage_source IN ('missing','partial') ORDER BY occurred_at DESC LIMIT 100",
            ),
            (
                "list attention (pinned to occurred)",
                "SELECT id FROM request_logs INDEXED BY idx_request_logs_occurred WHERE status = 'error' OR usage_source IN ('missing','partial') ORDER BY occurred_at DESC LIMIT 100",
            ),
        ] {
            let t = std::time::Instant::now();
            let n = conn
                .prepare(sql)
                .unwrap()
                .query_map([], |_| Ok(()))
                .unwrap()
                .count();
            println!("{name}: {:?} ({n} rows)", t.elapsed());
        }

        for sql in [
            "SELECT COUNT(*) FROM request_logs WHERE status = 'error' OR usage_source IN ('missing','partial')",
            "SELECT COUNT(*) FROM request_logs",
        ] {
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let rows = stmt.query_map([], |row| row.get::<_, String>(3)).unwrap();
            for row in rows {
                println!("PLAN [{sql}]: {}", row.unwrap());
            }
        }

        drop(conn);
        let _ = std::fs::remove_file(&path);
    }
}
