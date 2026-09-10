use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::db::models::RequestLog;
use crate::db::Db;

pub const DEFAULT_PORT: u16 = 8787;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayStatus {
    pub running: bool,
    pub port: u16,
    pub base_url: String,
    pub error: Option<String>,
}

impl GatewayStatus {
    pub fn base_url_for(port: u16) -> String {
        format!("http://127.0.0.1:{port}")
    }

    pub fn running(port: u16) -> Self {
        Self {
            running: true,
            port,
            base_url: Self::base_url_for(port),
            error: None,
        }
    }

    pub fn stopped(port: u16) -> Self {
        Self {
            running: false,
            port,
            base_url: Self::base_url_for(port),
            error: None,
        }
    }

    pub fn failed(port: u16, error: impl Into<String>) -> Self {
        Self {
            running: false,
            port,
            base_url: Self::base_url_for(port),
            error: Some(error.into()),
        }
    }
}

/// 网关通过该接口向外推送状态与流水，使 `gateway/` 不直接依赖 Tauri。
pub trait EventSink: Send + Sync + 'static {
    fn status(&self, status: &GatewayStatus);
    fn log(&self, log: &RequestLog);
}

pub struct GatewayHandle {
    pub port: u16,
    pub shutdown: Option<oneshot::Sender<()>>,
    pub task: JoinHandle<()>,
}

pub struct AppState {
    pub db: Db,
    pub http: reqwest::Client,
    pub events: Arc<dyn EventSink>,
    pub gateway: Mutex<Option<GatewayHandle>>,
    port: Mutex<u16>,
    data_dir: PathBuf,
}

impl AppState {
    pub fn new(
        db: Db,
        http: reqwest::Client,
        events: Arc<dyn EventSink>,
        port: u16,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            db,
            http,
            events,
            gateway: Mutex::new(None),
            port: Mutex::new(port),
            data_dir,
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn port(&self) -> u16 {
        self.port.lock().map(|guard| *guard).unwrap_or(DEFAULT_PORT)
    }

    pub fn set_port(&self, port: u16) {
        if let Ok(mut guard) = self.port.lock() {
            *guard = port;
        }
    }

    pub fn status(&self) -> GatewayStatus {
        let running = self
            .gateway
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false);
        let port = self.port();
        GatewayStatus {
            running,
            port,
            base_url: GatewayStatus::base_url_for(port),
            error: None,
        }
    }
}
