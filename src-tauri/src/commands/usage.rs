use std::sync::Arc;

use chrono::Local;
use tauri::State;

use crate::db::logs::{count_logs, list_log_aliases, list_logs, LogFilter};
use crate::db::models::RequestLog;
use crate::db::stats::{query_overview, Period, UsageOverview};
use crate::db::with_db;
use crate::error::AppError;
use crate::state::AppState;

#[tauri::command]
pub async fn list_logs_cmd(
    state: State<'_, Arc<AppState>>,
    filter: Option<LogFilter>,
) -> Result<Vec<RequestLog>, AppError> {
    let filter = filter.unwrap_or_default();
    with_db(&state.db, move |conn| list_logs(conn, &filter)).await
}

#[tauri::command]
pub async fn count_logs_cmd(
    state: State<'_, Arc<AppState>>,
    filter: Option<LogFilter>,
) -> Result<i64, AppError> {
    let filter = filter.unwrap_or_default();
    with_db(&state.db, move |conn| count_logs(conn, &filter)).await
}

#[tauri::command]
pub async fn list_log_aliases_cmd(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<String>, AppError> {
    with_db(&state.db, list_log_aliases).await
}

#[tauri::command]
pub async fn query_usage_overview_cmd(
    state: State<'_, Arc<AppState>>,
    period: String,
    anchor: Option<String>,
) -> Result<UsageOverview, AppError> {
    let period = Period::parse(&period)?;
    let anchor = match anchor.filter(|value| !value.is_empty()) {
        Some(raw) => chrono::DateTime::parse_from_rfc3339(&raw)
            .map(|value| value.with_timezone(&Local))
            .map_err(|_| AppError::message(format!("无法解析时间：{raw}")))?,
        None => Local::now(),
    };
    query_overview(&state.db, period, anchor).await
}
