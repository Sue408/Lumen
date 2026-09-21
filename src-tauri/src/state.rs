use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::db::models::RequestLog;
use crate::db::settings::default_session_headers;
use crate::db::Db;
use crate::error::{AppError, ErrorKind};

pub const DEFAULT_PORT: u16 = 8787;

/// 瞬时类失败（链路 / 5xx）的冷却阶梯，单位秒。一次网络抖动只压两秒——冷却是给上游
/// 恢复的余地，不是惩罚；同一目标持续失败才逐级拉长。
const TRANSIENT_COOLDOWN_LADDER: &[u64] = &[2, 5, 15, 45];
/// 额度类失败（429）的冷却阶梯，单位秒。上游明说了额度问题，起步就得给足恢复时间。
const EXHAUSTED_COOLDOWN_LADDER: &[u64] = &[30, 60, 150, 300];
/// 半开试探的租约：一次试探最长占位多久。带过期是兜底——试探请求被丢弃时，
/// 目标不能被永久冻结在「有人正在试」的状态里。
const PROBE_LEASE: Duration = Duration::from_secs(30);

/// 某类失败在第 `fails` 次连续失败时的冷却时长（阶梯封顶）。
fn cooldown_for(kind: ErrorKind, fails: u32) -> Duration {
    let ladder = if kind == ErrorKind::UpstreamRateLimited {
        EXHAUSTED_COOLDOWN_LADDER
    } else {
        TRANSIENT_COOLDOWN_LADDER
    };
    let step = fails.saturating_sub(1) as usize;
    Duration::from_secs(ladder[step.min(ladder.len() - 1)])
}

/// 一条冷却记录：哪个上游模型、被哪一类失败触发、连续失败几次、冷却到何时。
#[derive(Debug, Clone)]
struct Cooling {
    provider_id: String,
    kind: ErrorKind,
    /// 连续失败次数（被清除或自然过期即归零）。冷却时长由它决定。
    fails: u32,
    until: Instant,
    /// 半开试探租约：`Some` 且未过期表示已有请求在试探该目标。
    probe_until: Option<Instant>,
}

/// 冷却快照：把 `Instant` 折算成可序列化的剩余秒数，供日志与连通性展示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoolingView {
    pub upstream_model_id: String,
    pub provider_id: String,
    /// 触发冷却的失败类目（`ErrorKind` 的 snake_case）。
    pub error_kind: String,
    pub remaining_secs: u64,
}

/// 降级链中目标的临时冷却表：内存态、带过期，重启即清空。
#[derive(Default)]
struct Cooldowns {
    until: HashMap<String, Cooling>,
}

impl Cooldowns {
    fn mark(&mut self, key: &str, provider_id: &str, kind: ErrorKind, now: Instant) {
        let previous = self.until.get(key);
        // 连续失败累加：记录在被清除或自然过期前一直存在。
        let fails = previous.map_or(1, |cooling| cooling.fails + 1);
        let proposed = now + cooldown_for(kind, fails);
        // 两类失败混用时取更长的那个：不因后一次是「瞬时」就把已判定的额度冷却缩短。
        let until = previous.map_or(proposed, |cooling| cooling.until.max(proposed));
        self.until.insert(
            key.to_string(),
            Cooling {
                provider_id: provider_id.to_string(),
                kind,
                fails,
                until,
                probe_until: None,
            },
        );
    }

    /// 抢占一次半开试探权；已有请求在试探且租约未过期时返回 `false`。
    ///
    /// 必须在**同一把锁内**完成「判定 + 占位」——否则并发的几个请求会一起捶同一个
    /// 冷却中的上游，把试探变成惊群。
    fn try_claim_probe(&mut self, key: &str, now: Instant) -> bool {
        let Some(cooling) = self.until.get_mut(key) else {
            return true; // 不在冷却里：走正常路径，不占租约
        };
        match cooling.probe_until {
            Some(until) if until > now => false,
            _ => {
                cooling.probe_until = Some(now + PROBE_LEASE);
                true
            }
        }
    }

    fn snapshot(&mut self, now: Instant) -> Vec<CoolingView> {
        self.until.retain(|_, cooling| cooling.until > now);
        self.until
            .iter()
            .map(|(id, cooling)| CoolingView {
                upstream_model_id: id.clone(),
                provider_id: cooling.provider_id.clone(),
                error_kind: cooling.kind.as_str().to_string(),
                remaining_secs: cooling.until.saturating_duration_since(now).as_secs(),
            })
            .collect()
    }

    /// 清除冷却：`key` 为 `None` 时清空全部，返回被清除的条数。
    ///
    /// 两类调用点共用它：用户手动清除，以及**履约成功**——语义都是「当作没发生过」，
    /// 整条记录丢掉，连将来的连续失败升级也一并归零，而不是只把倒计时拨回零、留着
    /// 记忆继续惩罚下一个请求。
    ///
    /// 试探失败的收尾不在这里：失败路径最终会 `mark`，`mark` 重建记录时会把租约一起
    /// 清掉；若该次失败没有走到 `mark`（请求提前返回），租约由 `PROBE_LEASE` 过期兜底。
    fn clear(&mut self, key: Option<&str>) -> usize {
        match key {
            Some(key) => usize::from(self.until.remove(key).is_some()),
            None => {
                let count = self.until.len();
                self.until.clear();
                count
            }
        }
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
    pub fn new(db: Db, http: reqwest::Client, events: Arc<dyn EventSink>, port: u16) -> Self {
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

    /// 当前冷却快照（顺带清理过期项）：带原因与剩余时长。供日志与连通性展示。
    pub fn cooling_snapshot(&self) -> Vec<CoolingView> {
        let now = Instant::now();
        self.cooldowns
            .lock()
            .map(|mut guard| guard.snapshot(now))
            .unwrap_or_default()
    }

    /// 将某上游模型标记为冷却。时长由失败类目与**连续**失败次数决定（见 `cooldown_for`），
    /// 调用方不再传时长——同一条记录在被清除或自然过期前会一直累加。
    pub fn mark_cooling(&self, upstream_model_id: &str, provider_id: &str, kind: ErrorKind) {
        if let Ok(mut guard) = self.cooldowns.lock() {
            guard.mark(upstream_model_id, provider_id, kind, Instant::now());
        }
    }

    /// 抢占某目标的半开试探权；已有请求在试探中时返回 `false`。
    pub fn try_claim_cooling_probe(&self, upstream_model_id: &str) -> bool {
        self.cooldowns
            .lock()
            .map(|mut guard| guard.try_claim_probe(upstream_model_id, Instant::now()))
            .unwrap_or(true)
    }

    /// 清除冷却：手动（`model` 为 `None` 时清空全部）与履约成功共用。返回被清除的条数。
    pub fn clear_cooling(&self, upstream_model_id: Option<&str>) -> usize {
        self.cooldowns
            .lock()
            .map(|mut guard| guard.clear(upstream_model_id))
            .unwrap_or(0)
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
        cooldowns.mark("m1", "p1", ErrorKind::UpstreamUnavailable, now);

        assert_eq!(cooldowns.snapshot(now).len(), 1);
        assert_eq!(cooldowns.snapshot(now + Duration::from_secs(1)).len(), 1);
        assert!(cooldowns.snapshot(now + Duration::from_secs(2)).is_empty());
    }

    #[test]
    fn expired_cooldown_is_pruned() {
        let mut cooldowns = Cooldowns::default();
        let now = Instant::now();
        cooldowns.mark("m1", "p1", ErrorKind::UpstreamUnavailable, now);
        assert!(cooldowns.snapshot(now + Duration::from_secs(3)).is_empty());
        assert!(cooldowns.until.is_empty(), "过期项应被清理");
    }

    /// 连续失败逐级拉长；两侧各走各的阶梯，混用时取更长的那个。
    #[test]
    fn cooldown_ladder_escalates_per_kind() {
        let step = |kind: ErrorKind, fails: u32| cooldown_for(kind, fails).as_secs();

        assert_eq!(step(ErrorKind::LinkTimeout, 1), 2, "一次网络抖动只压两秒");
        assert_eq!(step(ErrorKind::LinkTimeout, 2), 5);
        assert_eq!(step(ErrorKind::LinkTimeout, 3), 15);
        assert_eq!(step(ErrorKind::LinkTimeout, 4), 45);
        assert_eq!(step(ErrorKind::LinkTimeout, 9), 45, "阶梯封顶");

        assert_eq!(step(ErrorKind::UpstreamRateLimited, 1), 30, "额度类起步就得给足");
        assert_eq!(step(ErrorKind::UpstreamRateLimited, 4), 300);

        let mut cooldowns = Cooldowns::default();
        let now = Instant::now();
        cooldowns.mark("m", "p", ErrorKind::LinkTimeout, now);
        assert_eq!(cooldowns.until["m"].until, now + Duration::from_secs(2));
        cooldowns.mark("m", "p", ErrorKind::LinkTimeout, now);
        assert_eq!(cooldowns.until["m"].fails, 2);
        assert_eq!(cooldowns.until["m"].until, now + Duration::from_secs(5));

        // 后一次是「瞬时」不得把已判定的额度冷却缩短。
        cooldowns.mark("m", "p", ErrorKind::UpstreamRateLimited, now);
        let quota_until = cooldowns.until["m"].until;
        cooldowns.mark("m", "p", ErrorKind::LinkTimeout, now);
        assert_eq!(cooldowns.until["m"].until, quota_until, "取更长的那个");
    }

    /// 半开试探：同一时刻只有一个请求抢得到，且租约带过期。
    #[test]
    fn probe_claim_is_exclusive_and_released_on_success() {
        let mut cooldowns = Cooldowns::default();
        let now = Instant::now();
        cooldowns.mark("m1", "p1", ErrorKind::LinkTimeout, now);

        assert!(cooldowns.try_claim_probe("m1", now), "首个请求抢到试探权");
        assert!(
            !cooldowns.try_claim_probe("m1", now + Duration::from_secs(1)),
            "租约内别人抢不到"
        );
        assert!(
            cooldowns.try_claim_probe("m1", now + Duration::from_secs(31)),
            "租约过期后可再抢"
        );

        cooldowns.clear(Some("m1"));
        assert!(
            cooldowns.snapshot(now + Duration::from_secs(1)).is_empty(),
            "试探成功即解除冷却"
        );

        // 失败路径不在这里收尾：租约由下一次 `mark` 重建记录时清掉。
        cooldowns.mark("m2", "p1", ErrorKind::LinkTimeout, now);
        assert!(cooldowns.try_claim_probe("m2", now));
        cooldowns.mark("m2", "p1", ErrorKind::LinkTimeout, now);
        assert!(cooldowns.until["m2"].probe_until.is_none(), "mark 一并清掉租约");
        assert!(cooldowns.try_claim_probe("m2", now + Duration::from_secs(1)));
    }

    #[test]
    fn snapshot_carries_reason_and_remaining() {
        let mut cooldowns = Cooldowns::default();
        let now = Instant::now();
        cooldowns.mark("m1", "p1", ErrorKind::LinkConnect, now);
        let view = cooldowns.snapshot(now + Duration::from_secs(1));
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].upstream_model_id, "m1");
        assert_eq!(view[0].provider_id, "p1");
        assert_eq!(view[0].error_kind, "link_connect");
        assert_eq!(view[0].remaining_secs, 1);
        assert!(cooldowns.snapshot(now + Duration::from_secs(2)).is_empty());
    }

    #[test]
    fn clears_one_cooldown_or_all() {
        let mut cooldowns = Cooldowns::default();
        let now = Instant::now();
        cooldowns.mark("m1", "p1", ErrorKind::UpstreamUnavailable, now);
        cooldowns.mark("m2", "p1", ErrorKind::LinkTimeout, now);

        assert_eq!(cooldowns.clear(Some("m1")), 1);
        assert_eq!(cooldowns.snapshot(now).len(), 1, "只清掉指定的那个");
        assert_eq!(cooldowns.clear(Some("m1")), 0, "重复清除不计数");

        assert_eq!(cooldowns.clear(None), 1, "清空返回剩余条数");
        assert!(cooldowns.snapshot(now).is_empty());
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
