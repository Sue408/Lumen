mod commands;
mod db;
mod error;
mod gateway;
mod state;

use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::db::models::RequestLog;
use crate::state::{AppState, EventSink, GatewayStatus};

/// 把网关事件桥接到 Tauri 事件总线，使 `gateway/` 保持与 Tauri 解耦。
struct TauriEventSink {
    app: tauri::AppHandle,
}

impl EventSink for TauriEventSink {
    fn status(&self, status: &GatewayStatus) {
        let _ = self.app.emit("gateway://status", status);
    }

    fn log(&self, log: &RequestLog) {
        let _ = self.app.emit("gateway://log", log);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .try_init()
        .ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let db_path = data_dir.join("lumen.db");
            let db = db::open(&db_path)?;

            {
                let conn = db
                    .lock()
                    .map_err(|_| error::AppError::message("数据库锁已中毒"))?;
                let mut candidates = db::seed::seed_path_candidates();
                candidates.push(data_dir.join("lumen.seed.json"));
                for path in candidates {
                    if path.exists() {
                        let imported = db::seed::seed_if_empty(&conn, &path)?;
                        if imported > 0 {
                            tracing::info!("已从 {} 导入 {} 条路由", path.display(), imported);
                            break;
                        }
                    }
                }
            }

            let http = reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()?;
            let events = Arc::new(TauriEventSink {
                app: app.handle().clone(),
            });
            let port = {
                let conn = db
                    .lock()
                    .map_err(|_| error::AppError::message("数据库锁已中毒"))?;
                let settings = db::settings::get_settings(&conn)?;
                std::env::var("LUMEN_PORT")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(settings.port)
            };
            let state = Arc::new(AppState::new(db, http, events, port));
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
            commands::list_logs_cmd,
            commands::count_logs_cmd,
            commands::list_log_aliases_cmd,
            commands::query_usage_overview_cmd,
            commands::get_settings_cmd,
            commands::save_settings_cmd,
        ])
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
