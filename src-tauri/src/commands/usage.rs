use std::sync::Arc;

use chrono::Local;
use serde::Serialize;
use tauri::State;

use crate::db::logs::{
    count_logs, list_log_aliases, list_logs, list_sessions, summarize_logs, LogFilter,
    LogSummary, SessionSummary,
};
use crate::db::models::RequestLog;
use crate::db::stats::{query_overview, KeyScope, Period, UsageOverview};
use crate::db::with_db;
use crate::error::AppError;
use crate::state::AppState;

/// 日志页一次取回：当前页记录、总条数与顶部三个口径计数。
/// 逐条查询都在同一把锁内完成，前端只需一次 IPC。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPageDto {
    pub logs: Vec<RequestLog>,
    pub total: i64,
    pub summary: LogSummary,
}

#[tauri::command]
pub async fn query_log_page_cmd(
    state: State<'_, Arc<AppState>>,
    filter: Option<LogFilter>,
    summary_filter: Option<LogFilter>,
) -> Result<LogPageDto, AppError> {
    let filter = filter.unwrap_or_default();
    let summary_filter = summary_filter.unwrap_or_else(|| filter.clone());
    with_db(&state.db, move |conn| {
        Ok(LogPageDto {
            logs: list_logs(conn, &filter)?,
            total: count_logs(conn, &filter)?,
            summary: summarize_logs(conn, &summary_filter)?,
        })
    })
    .await
}

#[tauri::command]
pub async fn list_log_aliases_cmd(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<String>, AppError> {
    with_db(&state.db, list_log_aliases).await
}

/// 会话维度聚合：按 `(virtual_key_id, session_id)` 分组，默认取最近 50 个会话。
#[tauri::command]
pub async fn list_sessions_cmd(
    state: State<'_, Arc<AppState>>,
    filter: Option<LogFilter>,
    limit: Option<i64>,
) -> Result<Vec<SessionSummary>, AppError> {
    let mut filter = filter.unwrap_or_default();
    if let Some(limit) = limit {
        filter.limit = Some(limit);
    }
    with_db(&state.db, move |conn| list_sessions(conn, &filter)).await
}

#[tauri::command]
pub async fn query_usage_overview_cmd(
    state: State<'_, Arc<AppState>>,
    period: String,
    anchor: Option<String>,
    virtual_key_id: Option<String>,
) -> Result<UsageOverview, AppError> {
    let period = Period::parse(&period)?;
    let anchor = match anchor.filter(|value| !value.is_empty()) {
        Some(raw) => chrono::DateTime::parse_from_rfc3339(&raw)
            .map(|value| value.with_timezone(&Local))
            .map_err(|_| AppError::message(format!("无法解析时间：{raw}")))?,
        None => Local::now(),
    };
    let scope = KeyScope::from_filter(virtual_key_id);
    query_overview(&state.db, period, anchor, scope).await
}
