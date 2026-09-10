use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::error::AppError;

fn autostart_error(error: impl std::fmt::Display) -> AppError {
    AppError::message(format!("开机自启设置失败：{error}"))
}

/// 读取操作系统真实的自启动状态（不落库，避免与注册表状态不一致）。
#[tauri::command]
pub fn get_autostart_cmd(app: AppHandle) -> Result<bool, AppError> {
    app.autolaunch().is_enabled().map_err(autostart_error)
}

/// 开启 / 关闭开机自启，返回操作后的真实状态。
#[tauri::command]
pub fn set_autostart_cmd(app: AppHandle, enabled: bool) -> Result<bool, AppError> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(autostart_error)?;
    } else {
        manager.disable().map_err(autostart_error)?;
    }
    manager.is_enabled().map_err(autostart_error)
}
