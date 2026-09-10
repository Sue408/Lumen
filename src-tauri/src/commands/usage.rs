use std::sync::Arc;

use tauri::State;

use crate::db::logs::{count_logs, list_logs, LogFilter};
use crate::db::models::RequestLog;
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
pub async fn count_logs_cmd(state: State<'_, Arc<AppState>>) -> Result<i64, AppError> {
    with_db(&state.db, count_logs).await
}
