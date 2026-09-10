use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::gateway;
use crate::state::{AppState, GatewayStatus};

const TRAY_ID: &str = "lumen-tray";
const MENU_SHOW: &str = "show";
const MENU_TOGGLE: &str = "toggle-gateway";
const MENU_QUIT: &str = "quit";

const LABEL_START: &str = "启动网关";
const LABEL_STOP: &str = "停止网关";

/// 托盘菜单句柄，供网关状态变化时刷新动态文案。
pub struct TrayHandles {
    toggle_item: MenuItem<Wry>,
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// 创建系统托盘：左键唤起主界面，右键菜单提供打开 / 启停网关 / 退出。
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, MENU_SHOW, "打开主界面", true, None::<&str>)?;
    let toggle_item = MenuItem::with_id(app, MENU_TOGGLE, LABEL_START, true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, MENU_QUIT, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &toggle_item, &quit_item])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Lumen")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            MENU_SHOW => show_main_window(app),
            MENU_QUIT => app.exit(0),
            MENU_TOGGLE => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<Arc<AppState>>().inner().clone();
                    if state.status().running {
                        let _ = gateway::stop(&state).await;
                    } else if let Err(error) = gateway::start(state.clone()).await {
                        state
                            .events
                            .status(&GatewayStatus::failed(state.port(), error.to_string()));
                    }
                });
            }
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    app.manage(TrayHandles { toggle_item });
    Ok(())
}

/// 网关运行状态变化时更新「启动 / 停止网关」菜单文案。
pub fn refresh(app: &AppHandle, running: bool) {
    if let Some(handles) = app.try_state::<TrayHandles>() {
        let _ = handles
            .toggle_item
            .set_text(if running { LABEL_STOP } else { LABEL_START });
    }
}
