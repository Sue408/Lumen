use std::sync::Arc;

use tauri::AppHandle;
use tauri::State;
use tauri_plugin_autostart::ManagerExt;

use crate::db::settings::{autostart_gateway, set_autostart_gateway, set_autostart_intent};
use crate::db::with_db;
use crate::error::AppError;
use crate::state::AppState;

fn autostart_error(error: impl std::fmt::Display) -> AppError {
    AppError::message(format!("开机自启设置失败：{error}"))
}

/// 读取操作系统真实的自启动状态（不落库，避免与注册表状态不一致）。
#[tauri::command]
pub fn get_autostart_cmd(app: AppHandle) -> Result<bool, AppError> {
    app.autolaunch().is_enabled().map_err(autostart_error)
}

/// 开启 / 关闭开机自启：同时写入注册表与「用户意图」，返回操作后的真实状态。
///
/// 意图用于启动时自愈——重装会改 exe 路径而不改注册表项名，单看 `is_enabled()` 无法
/// 察觉「开着但指向旧路径」。
#[tauri::command]
pub async fn set_autostart_cmd(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<bool, AppError> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(autostart_error)?;
    } else {
        manager.disable().map_err(autostart_error)?;
    }
    let actual = manager.is_enabled().map_err(autostart_error)?;
    let db = state.db.clone();
    with_db(&db, move |conn| set_autostart_intent(conn, actual)).await?;
    Ok(actual)
}

/// 读取「随系统启动时自动开启网关」设置。
#[tauri::command]
pub async fn get_autostart_gateway_cmd(
    state: State<'_, Arc<AppState>>,
) -> Result<bool, AppError> {
    let db = state.db.clone();
    with_db(&db, |conn| Ok(autostart_gateway(conn))).await
}

/// 保存「随系统启动时自动开启网关」设置。
#[tauri::command]
pub async fn set_autostart_gateway_cmd(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<bool, AppError> {
    let db = state.db.clone();
    with_db(&db, move |conn| set_autostart_gateway(conn, enabled)).await?;
    Ok(enabled)
}
