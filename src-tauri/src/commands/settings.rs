use std::sync::Arc;

use tauri::State;

use crate::db::settings::{get_settings, save_settings, Settings};
use crate::db::with_db;
use crate::error::AppError;
use crate::state::AppState;

#[tauri::command]
pub async fn get_settings_cmd(state: State<'_, Arc<AppState>>) -> Result<Settings, AppError> {
    with_db(&state.db, get_settings).await
}

#[tauri::command]
pub async fn save_settings_cmd(
    state: State<'_, Arc<AppState>>,
    input: Settings,
) -> Result<Settings, AppError> {
    let state = state.inner().clone();
    if state.status().running {
        return Err(AppError::AlreadyRunning);
    }
    let saved = with_db(&state.db, move |conn| save_settings(conn, &input)).await?;
    state.set_port(saved.port);
    Ok(saved)
}
