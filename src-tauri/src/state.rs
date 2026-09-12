use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::db::models::RequestLog;
use crate::db::settings::default_session_headers;
use crate::db::Db;
use crate::error::AppError;

pub const DEFAULT_PORT: u16 = 8787;

/// 瞬时失败（5xx / 连接失败 / 短限流耗尽）后的目标冷却时长。
pub const COOLDOWN_TRANSIENT: Duration = Duration::from_secs(60);
/// 疑似额度耗尽（长 `Retry-After` / 额度类错误）后的目标冷却时长。
pub const COOLDOWN_EXHAUSTED: Duration = Duration::from_secs(300);

/// 降级链中目标的临时冷却表：内存态、带过期，重启即清空。
#[derive(Default)]
struct Cooldowns {
    until: HashMap<String, Instant>,
}

impl Cooldowns {
    fn is_cooling(&mut self, key: &str, now: Instant) -> bool {
        match self.until.get(key) {
            Some(&deadline) if deadline > now => true,
            Some(_) => {
                self.until.remove(key);
                false
            }
            None => false,
        }
    }

    fn mark(&mut self, key: &str, duration: Duration, now: Instant) {
        self.until.insert(key.to_string(), now + duration);
    }
}

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
    pub http: RwLock<reqwest::Client>,
    pub events: Arc<dyn EventSink>,
    pub gateway: Mutex<Option<GatewayHandle>>,
    port: Mutex<u16>,
    close_to_tray: Mutex<bool>,
    /// 会话候选头名表（随设置保存刷新）。
    session_headers: Mutex<Vec<String>>,
    cooldowns: Mutex<Cooldowns>,
}

/// 统一构建出站 client：connect 超时固定 10s；传入非空代理 URL 时经该代理发出
/// （HTTP/HTTPS 代理，`Proxy::all` 同时覆盖 http 与 https 目标）。
pub fn build_http_client(proxy_url: Option<&str>) -> Result<reqwest::Client, AppError> {
    let mut builder = reqwest::Client::builder().connect_timeout(Duration::from_secs(10));
    if let Some(url) = proxy_url.map(str::trim).filter(|url| !url.is_empty()) {
        let proxy = reqwest::Proxy::all(url)
            .map_err(|error| AppError::message(format!("代理地址无效：{error}")))?;
        builder = builder.proxy(proxy);
    }
    builder.build().map_err(AppError::from)
}

impl AppState {
    pub fn new(
        db: Db,
        http: reqwest::Client,
        events: Arc<dyn EventSink>,
        port: u16,
    ) -> Self {
        Self {
            db,
            http: RwLock::new(http),
            events,
            gateway: Mutex::new(None),
            port: Mutex::new(port),
            close_to_tray: Mutex::new(true),
            session_headers: Mutex::new(default_session_headers()),
            cooldowns: Mutex::new(Cooldowns::default()),
        }
    }

    /// 取当前出站 client。`reqwest::Client` 内部是 Arc，clone 极廉价；在途请求继续持
    /// 旧 client 直至结束，新请求立即用最新 client。
    pub fn http(&self) -> reqwest::Client {
        self.http
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|poison| poison.into_inner().clone())
    }

    /// 替换出站 client（代理设置保存后调用，即时生效）。
    pub fn set_http(&self, client: reqwest::Client) {
        match self.http.write() {
            Ok(mut guard) => *guard = client,
            Err(poison) => *poison.into_inner() = client,
        }
    }

    pub fn session_headers(&self) -> Vec<String> {
        self.session_headers
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub fn set_session_headers(&self, session_headers: Vec<String>) {
        if let Ok(mut guard) = self.session_headers.lock() {
            *guard = session_headers;
        }
    }

    pub fn close_to_tray(&self) -> bool {
        self.close_to_tray
            .lock()
            .map(|guard| *guard)
            .unwrap_or(true)
    }

    pub fn set_close_to_tray(&self, close_to_tray: bool) {
        if let Ok(mut guard) = self.close_to_tray.lock() {
            *guard = close_to_tray;
        }
    }

    pub fn port(&self) -> u16 {
        self.port.lock().map(|guard| *guard).unwrap_or(DEFAULT_PORT)
    }

    pub fn set_port(&self, port: u16) {
        if let Ok(mut guard) = self.port.lock() {
            *guard = port;
        }
    }

    /// 该上游模型是否处于降级冷却中（过期即自动清理）。
    pub fn is_cooling(&self, upstream_model_id: &str) -> bool {
        self.cooldowns
            .lock()
            .map(|mut guard| guard.is_cooling(upstream_model_id, Instant::now()))
            .unwrap_or(false)
    }

    /// 将某上游模型标记为冷却一段时间。
    pub fn mark_cooling(&self, upstream_model_id: &str, duration: Duration) {
        if let Ok(mut guard) = self.cooldowns.lock() {
            guard.mark(upstream_model_id, duration, Instant::now());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_and_reads_cooling() {
        let mut cooldowns = Cooldowns::default();
        let now = Instant::now();
        cooldowns.mark("m1", Duration::from_secs(60), now);

        assert!(cooldowns.is_cooling("m1", now));
        assert!(cooldowns.is_cooling("m1", now + Duration::from_secs(59)));
        assert!(!cooldowns.is_cooling("m1", now + Duration::from_secs(60)));
        assert!(!cooldowns.is_cooling("m2", now));
    }

    #[test]
    fn expired_cooldown_is_pruned() {
        let mut cooldowns = Cooldowns::default();
        let now = Instant::now();
        cooldowns.mark("m1", Duration::from_secs(1), now);
        assert!(!cooldowns.is_cooling("m1", now + Duration::from_secs(2)));
        assert!(cooldowns.until.is_empty(), "过期项应被清理");
    }

    #[test]
    fn builds_client_without_proxy() {
        assert!(build_http_client(None).is_ok());
        assert!(build_http_client(Some("")).is_ok());
        assert!(build_http_client(Some("   ")).is_ok());
    }

    #[test]
    fn builds_client_with_http_proxy() {
        assert!(build_http_client(Some("http://127.0.0.1:7890")).is_ok());
        assert!(build_http_client(Some("https://127.0.0.1:7890")).is_ok());
    }

    #[test]
    fn rejects_invalid_proxy_url() {
        let error = build_http_client(Some("not a url")).unwrap_err();
        assert!(matches!(error, AppError::Message(_)), "got {error:?}");
    }
}
