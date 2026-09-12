use std::sync::Arc;

use tauri::State;

use crate::db::seed::{export_seed, import_seed_json, ImportSummary};
use crate::db::settings::{get_settings, save_settings, Settings};
use crate::db::{clear_business_data, with_db};
use crate::error::AppError;
use crate::state::{build_http_client, AppState};

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
    // 先校验代理并构建新 client（非法地址在落库前拒绝），保存成功后即时换入。
    let client = build_http_client(input.proxy_url.as_deref())?;
    let saved = with_db(&state.db, move |conn| save_settings(conn, &input)).await?;
    state.set_port(saved.port);
    state.set_close_to_tray(saved.close_to_tray);
    state.set_session_headers(saved.session_headers.clone());
    state.set_http(client);
    Ok(saved)
}

#[tauri::command]
pub async fn export_seed_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<String, AppError> {
    let state = state.inner().clone();
    let json = with_db(&state.db, export_seed).await?;
    tokio::fs::write(&path, json).await?;
    Ok(path)
}

#[tauri::command]
pub async fn import_seed_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    preview: bool,
) -> Result<ImportSummary, AppError> {
    let state = state.inner().clone();
    let raw = tokio::fs::read_to_string(&path).await?;
    with_db(&state.db, move |conn| import_seed_json(conn, &raw, !preview)).await
}

#[tauri::command]
pub async fn reset_data_cmd(state: State<'_, Arc<AppState>>) -> Result<(), AppError> {
    let state = state.inner().clone();
    if state.status().running {
        return Err(AppError::AlreadyRunning);
    }
    with_db(&state.db, clear_business_data).await
}
