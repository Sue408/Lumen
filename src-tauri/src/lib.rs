mod commands;
mod db;
mod error;
mod gateway;
mod logging;
mod state;
mod tray;
mod util;

use std::sync::Arc;

use tauri::{Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
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
            commands::query_telemetry_cmd,
            commands::test_provider_cmd,
            commands::get_settings_cmd,
            commands::save_settings_cmd,
            commands::export_seed_cmd,
            commands::import_seed_cmd,
            commands::reset_data_cmd,
            commands::get_autostart_cmd,
            commands::set_autostart_cmd,
            commands::get_autostart_gateway_cmd,
            commands::set_autostart_gateway_cmd
            $(, $extra)*
        ]
    };
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
            // 先装日志：release 无控制台，不落盘就什么都看不到。
            logging::init(&data_dir);

            let db_path = data_dir.join(db_file_name());
            #[cfg(debug_assertions)]
            tracing::info!("开发构建使用独立数据库：{}", db_path.display());
            let db = db::open(&db_path)?;

            let events = Arc::new(TauriEventSink {
                app: app.handle().clone(),
            });
            let (port, close_to_tray, session_headers, proxy_url, autostart_gateway) = {
                let conn = db
                    .lock()
                    .map_err(|_| error::AppError::message("数据库锁已中毒"))?;
                let settings = db::settings::get_settings(&conn)?;

                // 自启意图持久化 + 启动时自愈。重装后 exe 路径变了、注册表项名没变，
                // `is_enabled()` 仍返回 true 却指向旧路径，表现为「设置里开着却不再自启」。
                // 首次没有意图记录时，以系统当前状态为准落库；之后按意图重放一次注册，
                // 把注册表刷成当前可执行文件。
                let intent = match db::settings::autostart_intent(&conn) {
                    Some(intent) => intent,
                    None => {
                        let current = app.autolaunch().is_enabled().unwrap_or(false);
                        db::settings::set_autostart_intent(&conn, current)?;
                        current
                    }
                };
                let manager = app.autolaunch();
                let applied = if intent { manager.enable() } else { manager.disable() };
                if let Err(error) = applied {
                    tracing::warn!("开机自启注册表自愈失败：{error}");
                }

                (
                    settings.port,
                    settings.close_to_tray,
                    settings.session_headers,
                    settings.proxy_url,
                    db::settings::autostart_gateway(&conn),
                )
            };
            let http = state::build_http_client(proxy_url.as_deref())?;
            let state = Arc::new(AppState::new(db, http, events, port));
            state.set_close_to_tray(close_to_tray);
            state.set_session_headers(session_headers);
            app.manage(state);

            tray::setup(app.handle())?;
            tray::refresh(app.handle(), false);

            // 开机自启注册时附带 `--minimized`：静默进入托盘，不闪主窗口。
            let start_minimized = std::env::args().any(|arg| arg == "--minimized");
            if let Some(window) = app.get_webview_window("main") {
                if start_minimized {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }

            // 仅「随系统自启」这次启动自动拉起网关；手动打开时不自动开。
            if start_minimized && autostart_gateway {
                let state = app.state::<Arc<AppState>>().inner().clone();
                tauri::async_runtime::spawn(async move {
                    tracing::info!("随系统自启：自动启动网关");
                    if let Err(error) = gateway::start(state.clone()).await {
                        tracing::warn!("随系统自启启动网关失败：{error}");
                        state
                            .events
                            .status(&GatewayStatus::failed(state.port(), error.to_string()));
                    }
                });
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
