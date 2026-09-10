use std::sync::Arc;

use tauri::State;

use crate::db::seed::export_seed;
use crate::db::settings::{get_settings, save_settings, Settings};
use crate::db::{clear_business_data, with_db};
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
    if state.status().running && input.port != state.port() {
        return Err(AppError::AlreadyRunning);
    }
    let saved = with_db(&state.db, move |conn| save_settings(conn, &input)).await?;
    state.set_port(saved.port);
    state.set_close_to_tray(saved.close_to_tray);
    Ok(saved)
}

#[tauri::command]
pub async fn export_seed_cmd(state: State<'_, Arc<AppState>>) -> Result<String, AppError> {
    let state = state.inner().clone();
    let dir = state.data_dir().to_path_buf();
    let json = with_db(&state.db, export_seed).await?;
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join("lumen.seed.json");
    tokio::fs::write(&path, json).await?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn reset_data_cmd(state: State<'_, Arc<AppState>>) -> Result<(), AppError> {
    let state = state.inner().clone();
    if state.status().running {
        return Err(AppError::AlreadyRunning);
    }
    with_db(&state.db, clear_business_data).await
}
