mod commands;
mod db;
mod error;
mod gateway;
mod state;
mod tray;
mod util;

use std::sync::Arc;

use tauri::{Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_window_state::StateFlags;

use crate::db::models::RequestLog;
use crate::state::{AppState, EventSink, GatewayStatus};

/// dev 与 release 使用不同的数据库文件：开发构建落在 `lumen-dev.db`，发布构建落在
/// `lumen.db`。两者共用同一个 `app_data_dir`，若不隔离，`tauri dev` 时的调试 / 演示
/// 注入（会清空全部业务数据）将污染真实账本。
const fn db_file_name() -> &'static str {
    if cfg!(debug_assertions) {
        "lumen-dev.db"
    } else {
        "lumen.db"
    }
}

/// 把网关事件桥接到 Tauri 事件总线，使 `gateway/` 保持与 Tauri 解耦。
struct TauriEventSink {
    app: tauri::AppHandle,
}

impl EventSink for TauriEventSink {
    fn status(&self, status: &GatewayStatus) {
        let _ = self.app.emit("gateway://status", status);
        tray::refresh(&self.app, status.running);
    }

    fn log(&self, log: &RequestLog) {
        let _ = self.app.emit("gateway://log", log);
    }
}

/// 公共命令注册。`$extra` 仅在 debug 构建下用于追加开发命令，
/// release 构建不编译对应模块，因此不会出现在二进制里。
macro_rules! register_handlers {
    ($($extra:path),* $(,)?) => {
        tauri::generate_handler![
            commands::gateway_status,
            commands::start_gateway,
            commands::stop_gateway,
            commands::list_providers_cmd,
            commands::save_provider_cmd,
            commands::delete_provider_cmd,
            commands::list_upstream_models_cmd,
            commands::save_upstream_model_cmd,
            commands::delete_upstream_model_cmd,
            commands::list_routes_cmd,
            commands::save_route_cmd,
            commands::delete_route_cmd,
            commands::list_virtual_keys_cmd,
            commands::save_virtual_key_cmd,
            commands::delete_virtual_key_cmd,
            commands::query_virtual_keys_usage_cmd,
            commands::query_log_page_cmd,
            commands::list_log_aliases_cmd,
            commands::list_sessions_cmd,
            commands::query_usage_overview_cmd,
            commands::get_settings_cmd,
            commands::save_settings_cmd,
            commands::export_seed_cmd,
            commands::import_seed_cmd,
            commands::reset_data_cmd,
            commands::get_autostart_cmd,
            commands::set_autostart_cmd
            $(, $extra)*
        ]
    };
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .try_init()
        .ok();

    let mut builder = tauri::Builder::default();

    // 单实例插件必须最先注册，第二次启动时唤起已运行的主窗口。
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }));
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .build(),
        )
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let db_path = data_dir.join(db_file_name());
            #[cfg(debug_assertions)]
            tracing::info!("开发构建使用独立数据库：{}", db_path.display());
            let db = db::open(&db_path)?;

            let http = reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()?;
            let events = Arc::new(TauriEventSink {
                app: app.handle().clone(),
            });
            let (port, close_to_tray, session_headers) = {
                let conn = db
                    .lock()
                    .map_err(|_| error::AppError::message("数据库锁已中毒"))?;
                let settings = db::settings::get_settings(&conn)?;
                (settings.port, settings.close_to_tray, settings.session_headers)
            };
            let state = Arc::new(AppState::new(db, http, events, port));
            state.set_close_to_tray(close_to_tray);
            state.set_session_headers(session_headers);
            app.manage(state);

            tray::setup(app.handle())?;
            tray::refresh(app.handle(), false);

            // 开机自启注册时附带 `--minimized`，实现静默进入托盘。
            if std::env::args().any(|arg| arg == "--minimized") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let close_to_tray = window
                    .app_handle()
                    .try_state::<Arc<AppState>>()
                    .map(|state| state.close_to_tray())
                    .unwrap_or(false);
                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler({
            #[cfg(debug_assertions)]
            {
                register_handlers![commands::inject_demo_cmd]
            }
            #[cfg(not(debug_assertions))]
            {
                register_handlers![]
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app_handle.try_state::<Arc<AppState>>() {
                    if let Ok(mut guard) = state.gateway.lock() {
                        if let Some(mut handle) = guard.take() {
                            if let Some(shutdown) = handle.shutdown.take() {
                                let _ = shutdown.send(());
                            }
                        }
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::db_file_name;

    #[test]
    fn database_file_follows_the_build_profile() {
        if cfg!(debug_assertions) {
            assert_eq!(db_file_name(), "lumen-dev.db");
        } else {
            assert_eq!(db_file_name(), "lumen.db");
        }
    }
}
