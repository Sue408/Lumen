use std::sync::Arc;

use tauri::State;

use crate::db::demo::{inject_demo, DemoScenario, DemoSummary};
use crate::db::with_db;
use crate::error::AppError;
use crate::state::AppState;

/// 开发专用：按场景完全替换业务数据。仅 debug 构建注册。
#[tauri::command]
pub async fn inject_demo_cmd(
    state: State<'_, Arc<AppState>>,
    scenario: String,
) -> Result<DemoSummary, AppError> {
    let scenario = DemoScenario::parse(&scenario)?;
    with_db(&state.db, move |conn| inject_demo(conn, scenario)).await
}
