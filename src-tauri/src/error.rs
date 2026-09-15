use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("数据库错误：{0}")]
    Db(#[from] rusqlite::Error),
    #[error("上游请求失败：{0}")]
    Http(#[from] reqwest::Error),
    #[error("序列化错误：{0}")]
    Serde(#[from] serde_json::Error),
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("后台任务错误：{0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("网关已在运行")]
    AlreadyRunning,
    #[error("网关未在运行")]
    NotRunning,
    #[error("未找到模型：{0}")]
    ModelNotFound(String),
    #[error("缺少或无效的虚拟密钥")]
    Unauthorized,
    #[error("虚拟密钥「{name}」已超出额度：已用 ${spent:.2} / 上限 ${limit:.2}（{period}）")]
    QuotaExceeded {
        name: String,
        spent: f64,
        limit: f64,
        period: String,
    },
    #[error("{0}")]
    Message(String),
}

impl AppError {
    pub fn message(message: impl Into<String>) -> Self {
        AppError::Message(message.into())
    }

    /// 唯一约束冲突映射为面向用户的提示，其它数据库错误原样上报。
    pub fn from_constraint(error: rusqlite::Error, message: impl Into<String>) -> Self {
        if is_unique_violation(&error) {
            AppError::Message(message.into())
        } else {
            AppError::Db(error)
        }
    }

    /// 该错误对应的 HTTP 状态码。IPC 与网关响应共用此映射，避免两处漂移。
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::ModelNotFound(_) => StatusCode::NOT_FOUND,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::QuotaExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
            AppError::NotRunning | AppError::AlreadyRunning => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// SQLite 唯一约束（UNIQUE / PRIMARY KEY）冲突。
fn is_unique_violation(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(inner, _)
            if inner.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = json!({
            "error": {
                "message": self.to_string(),
                "type": "lumen_error",
                "code": error_code(&self),
            }
        });
        (status, axum::Json(body)).into_response()
    }
}

fn error_code(error: &AppError) -> &'static str {
    match error {
        AppError::ModelNotFound(_) => "model_not_found",
        AppError::Unauthorized => "unauthorized",
        AppError::QuotaExceeded { .. } => "quota_exceeded",
        AppError::AlreadyRunning => "already_running",
        AppError::NotRunning => "not_running",
        _ => "internal_error",
    }
}

/// 归因主体：这个问题**记在谁头上**。
///
/// 注意「链路」（DNS / 连接 / TLS / 代理 / 超时 / 流中断）**不是第四方**：它归
/// [`ErrorDomain::Upstream`]，用 [`ErrorKind`] 的 `link_*` 前缀标明「问题出在去上游的
/// 路上，不在对面」。客户端腿恒为回环，因此客户端侧的断开是**行为**而非网络。
// 本段归因词表在 Task 3～8 逐步接线；全部消费前保留 `dead_code` 豁免，Task 9 清理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorDomain {
    /// 去上游那一跳（含链路 transport）。
    Upstream,
    /// Lumen 网关自身：配置 / 编排 / 存储 / 内部。
    Gateway,
    /// 客户端：凭据 / 请求形状 / 主动取消。
    Client,
}

impl ErrorDomain {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorDomain::Upstream => "upstream",
            ErrorDomain::Gateway => "gateway",
            ErrorDomain::Client => "client",
        }
    }
}

/// 错误类目：问题**是哪一类**。类目唯一地决定其归属（见 [`ErrorKind::domain`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    // —— 客户端 ——
    /// 虚拟密钥缺失 / 未知 / 停用。
    Unauthorized,
    /// 请求缺 `model` 等形状问题。
    InvalidRequest,
    /// 请求体超过网关上限。
    PayloadTooLarge,
    /// JSON 畸形或 content-type 不符。
    MalformedBody,
    /// 未匹配任何路由的端点（客户端 `base_url` 写错）。
    UnknownEndpoint,
    /// 客户端中断连接（响应未完整送达）。
    ClientClosed,
    // —— 网关 ——
    /// 别名不存在 / 路由未启用。
    RouteNotFound,
    /// 别名存在，但目标 / 模型 / 协议端点全不可用。
    RouteNoCandidate,
    /// 全部候选处于冷却中。
    AllCandidatesCooling,
    /// 本机出口不可达（跨 provider 链路齐失败）。
    EgressUnreachable,
    /// 虚拟密钥额度超限。
    QuotaExceeded,
    /// 请求语义无法跨协议安全表达。
    UnsupportedConversion,
    /// 协议转换失败或出现不可恢复降级。
    ConversionFailed,
    /// 数据库 / 锁 / 磁盘错误。
    StorageError,
    /// 网关内部错误。
    InternalError,
    // —— 上游 ——
    /// 上游 4xx（非 429）。
    UpstreamRejected,
    /// 上游 401 / 403（多为 provider 凭据配置）。
    UpstreamAuthFailed,
    /// 上游 429（限流 / 额度）。
    UpstreamRateLimited,
    /// 上游 5xx（含 529）。
    UpstreamUnavailable,
    /// 上游对流式请求返回非 SSE。
    UpstreamStreamMismatch,
    /// 上游以 2xx 返回了错误体。
    UpstreamBadResponse,
    /// 上游流未正常收尾。
    UpstreamTruncated,
    // —— 链路（归 upstream）——
    /// DNS / 连接 / TLS / 代理失败（reqwest 无法再细分，已知局限）。
    LinkConnect,
    /// 等响应头超时（已发出请求，未按时收到响应）。
    LinkTimeout,
    /// 流中途 reset / 断网。
    LinkStreamReset,
    /// 流静默超时（长时间无数据）。
    LinkStreamStalled,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Unauthorized => "unauthorized",
            ErrorKind::InvalidRequest => "invalid_request",
            ErrorKind::PayloadTooLarge => "payload_too_large",
            ErrorKind::MalformedBody => "malformed_body",
            ErrorKind::UnknownEndpoint => "unknown_endpoint",
            ErrorKind::ClientClosed => "client_closed",
            ErrorKind::RouteNotFound => "route_not_found",
            ErrorKind::RouteNoCandidate => "route_no_candidate",
            ErrorKind::AllCandidatesCooling => "all_candidates_cooling",
            ErrorKind::EgressUnreachable => "egress_unreachable",
            ErrorKind::QuotaExceeded => "quota_exceeded",
            ErrorKind::UnsupportedConversion => "unsupported_conversion",
            ErrorKind::ConversionFailed => "conversion_failed",
            ErrorKind::StorageError => "storage_error",
            ErrorKind::InternalError => "internal_error",
            ErrorKind::UpstreamRejected => "upstream_rejected",
            ErrorKind::UpstreamAuthFailed => "upstream_auth_failed",
            ErrorKind::UpstreamRateLimited => "upstream_rate_limited",
            ErrorKind::UpstreamUnavailable => "upstream_unavailable",
            ErrorKind::UpstreamStreamMismatch => "upstream_stream_mismatch",
            ErrorKind::UpstreamBadResponse => "upstream_bad_response",
            ErrorKind::UpstreamTruncated => "upstream_truncated",
            ErrorKind::LinkConnect => "link_connect",
            ErrorKind::LinkTimeout => "link_timeout",
            ErrorKind::LinkStreamReset => "link_stream_reset",
            ErrorKind::LinkStreamStalled => "link_stream_stalled",
        }
    }

    pub fn domain(self) -> ErrorDomain {
        match self {
            ErrorKind::Unauthorized
            | ErrorKind::InvalidRequest
            | ErrorKind::PayloadTooLarge
            | ErrorKind::MalformedBody
            | ErrorKind::UnknownEndpoint
            | ErrorKind::ClientClosed => ErrorDomain::Client,
            ErrorKind::RouteNotFound
            | ErrorKind::RouteNoCandidate
            | ErrorKind::AllCandidatesCooling
            | ErrorKind::EgressUnreachable
            | ErrorKind::QuotaExceeded
            | ErrorKind::UnsupportedConversion
            | ErrorKind::ConversionFailed
            | ErrorKind::StorageError
            | ErrorKind::InternalError => ErrorDomain::Gateway,
            ErrorKind::UpstreamRejected
            | ErrorKind::UpstreamAuthFailed
            | ErrorKind::UpstreamRateLimited
            | ErrorKind::UpstreamUnavailable
            | ErrorKind::UpstreamStreamMismatch
            | ErrorKind::UpstreamBadResponse
            | ErrorKind::UpstreamTruncated
            | ErrorKind::LinkConnect
            | ErrorKind::LinkTimeout
            | ErrorKind::LinkStreamReset
            | ErrorKind::LinkStreamStalled => ErrorDomain::Upstream,
        }
    }

    /// 该失败是否属于「链路」——去上游那一跳的传输问题。冷却抑制据此判定。
    pub fn is_link(self) -> bool {
        matches!(
            self,
            ErrorKind::LinkConnect
                | ErrorKind::LinkTimeout
                | ErrorKind::LinkStreamReset
                | ErrorKind::LinkStreamStalled
        )
    }
}

/// 一条可落库的归因：主体 + 类目 + 人话。归属由类目唯一决定，不单独传入。
#[derive(Debug, Clone)]
pub struct LogError {
    pub kind: ErrorKind,
    pub message: String,
}

impl LogError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn domain(&self) -> ErrorDomain {
        self.kind.domain()
    }
}

impl AppError {
    /// 把应用错误映射成落库归因。调用点若掌握更细的语境（如 401 是上游凭据问题），
    /// 应自行构造 [`LogError`] 覆盖本默认映射。
    pub fn log_error(&self) -> LogError {
        match self {
            AppError::Unauthorized => LogError::new(ErrorKind::Unauthorized, self.to_string()),
            AppError::ModelNotFound(_) => LogError::new(ErrorKind::RouteNotFound, self.to_string()),
            AppError::QuotaExceeded { .. } => {
                LogError::new(ErrorKind::QuotaExceeded, self.to_string())
            }
            AppError::Db(_) => LogError::new(ErrorKind::StorageError, self.to_string()),
            AppError::Http(error) => {
                let (kind, hint) = classify_reqwest(error);
                LogError::new(kind, format!("{hint}：{error}"))
            }
            AppError::Serde(_) | AppError::Io(_) | AppError::Join(_) => {
                LogError::new(ErrorKind::InternalError, self.to_string())
            }
            AppError::AlreadyRunning | AppError::NotRunning | AppError::Message(_) => {
                LogError::new(ErrorKind::InternalError, self.to_string())
            }
        }
    }
}

/// 把 reqwest 的传输层错误细分为链路类目，并给一句中文诊断。
///
/// **已知局限**：reqwest 无法进一步区分 DNS / TCP / TLS / 代理，统一落
/// [`ErrorKind::LinkConnect`]。
pub fn classify_reqwest(error: &reqwest::Error) -> (ErrorKind, &'static str) {
    let kind = classify_transport(
        error.is_timeout(),
        error.is_connect(),
        error.is_request(),
        error.is_body() || error.is_decode(),
    );
    (kind, transport_hint(kind))
}

/// 纯映射，便于穷举单测（真实 `reqwest::Error` 无法构造）。
fn classify_transport(
    is_timeout: bool,
    is_connect: bool,
    is_request: bool,
    is_body_or_decode: bool,
) -> ErrorKind {
    if is_timeout {
        ErrorKind::LinkTimeout
    } else if is_connect || is_request {
        ErrorKind::LinkConnect
    } else if is_body_or_decode {
        ErrorKind::LinkStreamReset
    } else {
        ErrorKind::InternalError
    }
}

/// 按上游 HTTP 状态码归类：401 / 403 视为凭据问题，429 限流，5xx 不可用，其余 4xx 被拒。
pub fn classify_upstream_status(status: StatusCode) -> ErrorKind {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        ErrorKind::UpstreamAuthFailed
    } else if status == StatusCode::TOO_MANY_REQUESTS {
        ErrorKind::UpstreamRateLimited
    } else if status.is_server_error() {
        ErrorKind::UpstreamUnavailable
    } else {
        ErrorKind::UpstreamRejected
    }
}

fn transport_hint(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::LinkTimeout => "连接上游超时（网络不通或上游长时间无响应）",
        ErrorKind::LinkConnect => "无法连接上游（DNS / 连接 / TLS / 代理失败）",
        ErrorKind::LinkStreamReset => "上游响应读取中断",
        _ => "上游请求发生未知错误",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_strings_and_domains_are_stable() {
        let table: &[(ErrorKind, &str, ErrorDomain)] = &[
            (ErrorKind::Unauthorized, "unauthorized", ErrorDomain::Client),
            (
                ErrorKind::InvalidRequest,
                "invalid_request",
                ErrorDomain::Client,
            ),
            (
                ErrorKind::PayloadTooLarge,
                "payload_too_large",
                ErrorDomain::Client,
            ),
            (
                ErrorKind::MalformedBody,
                "malformed_body",
                ErrorDomain::Client,
            ),
            (
                ErrorKind::UnknownEndpoint,
                "unknown_endpoint",
                ErrorDomain::Client,
            ),
            (
                ErrorKind::ClientClosed,
                "client_closed",
                ErrorDomain::Client,
            ),
            (
                ErrorKind::RouteNotFound,
                "route_not_found",
                ErrorDomain::Gateway,
            ),
            (
                ErrorKind::RouteNoCandidate,
                "route_no_candidate",
                ErrorDomain::Gateway,
            ),
            (
                ErrorKind::AllCandidatesCooling,
                "all_candidates_cooling",
                ErrorDomain::Gateway,
            ),
            (
                ErrorKind::EgressUnreachable,
                "egress_unreachable",
                ErrorDomain::Gateway,
            ),
            (
                ErrorKind::QuotaExceeded,
                "quota_exceeded",
                ErrorDomain::Gateway,
            ),
            (
                ErrorKind::UnsupportedConversion,
                "unsupported_conversion",
                ErrorDomain::Gateway,
            ),
            (
                ErrorKind::ConversionFailed,
                "conversion_failed",
                ErrorDomain::Gateway,
            ),
            (
                ErrorKind::StorageError,
                "storage_error",
                ErrorDomain::Gateway,
            ),
            (
                ErrorKind::InternalError,
                "internal_error",
                ErrorDomain::Gateway,
            ),
            (
                ErrorKind::UpstreamRejected,
                "upstream_rejected",
                ErrorDomain::Upstream,
            ),
            (
                ErrorKind::UpstreamAuthFailed,
                "upstream_auth_failed",
                ErrorDomain::Upstream,
            ),
            (
                ErrorKind::UpstreamRateLimited,
                "upstream_rate_limited",
                ErrorDomain::Upstream,
            ),
            (
                ErrorKind::UpstreamUnavailable,
                "upstream_unavailable",
                ErrorDomain::Upstream,
            ),
            (
                ErrorKind::UpstreamStreamMismatch,
                "upstream_stream_mismatch",
                ErrorDomain::Upstream,
            ),
            (
                ErrorKind::UpstreamBadResponse,
                "upstream_bad_response",
                ErrorDomain::Upstream,
            ),
            (
                ErrorKind::UpstreamTruncated,
                "upstream_truncated",
                ErrorDomain::Upstream,
            ),
            (
                ErrorKind::LinkConnect,
                "link_connect",
                ErrorDomain::Upstream,
            ),
            (
                ErrorKind::LinkTimeout,
                "link_timeout",
                ErrorDomain::Upstream,
            ),
            (
                ErrorKind::LinkStreamReset,
                "link_stream_reset",
                ErrorDomain::Upstream,
            ),
            (
                ErrorKind::LinkStreamStalled,
                "link_stream_stalled",
                ErrorDomain::Upstream,
            ),
        ];
        for (kind, text, domain) in table {
            assert_eq!(kind.as_str(), *text);
            assert_eq!(kind.domain(), *domain);
        }
    }

    #[test]
    fn link_kinds_are_flagged() {
        assert!(ErrorKind::LinkConnect.is_link());
        assert!(ErrorKind::LinkTimeout.is_link());
        assert!(ErrorKind::LinkStreamReset.is_link());
        assert!(ErrorKind::LinkStreamStalled.is_link());
        assert!(!ErrorKind::UpstreamUnavailable.is_link());
        assert!(!ErrorKind::EgressUnreachable.is_link());
    }

    #[test]
    fn transport_flags_classify_by_phase() {
        // timeout 优先：连接超时也带 connect 标志，但按「超时」记更贴切。
        assert_eq!(
            classify_transport(true, true, false, false),
            ErrorKind::LinkTimeout
        );
        assert_eq!(
            classify_transport(false, true, false, false),
            ErrorKind::LinkConnect
        );
        assert_eq!(
            classify_transport(false, false, true, false),
            ErrorKind::LinkConnect
        );
        assert_eq!(
            classify_transport(false, false, false, true),
            ErrorKind::LinkStreamReset
        );
        assert_eq!(
            classify_transport(false, false, false, false),
            ErrorKind::InternalError
        );
    }

    #[test]
    fn upstream_status_classifies_by_family() {
        use axum::http::StatusCode;
        assert_eq!(
            classify_upstream_status(StatusCode::UNAUTHORIZED),
            ErrorKind::UpstreamAuthFailed
        );
        assert_eq!(
            classify_upstream_status(StatusCode::FORBIDDEN),
            ErrorKind::UpstreamAuthFailed
        );
        assert_eq!(
            classify_upstream_status(StatusCode::TOO_MANY_REQUESTS),
            ErrorKind::UpstreamRateLimited
        );
        assert_eq!(
            classify_upstream_status(StatusCode::BAD_GATEWAY),
            ErrorKind::UpstreamUnavailable
        );
        assert_eq!(
            classify_upstream_status(StatusCode::NOT_FOUND),
            ErrorKind::UpstreamRejected
        );
        assert_eq!(
            classify_upstream_status(StatusCode::BAD_REQUEST),
            ErrorKind::UpstreamRejected
        );
    }

    #[test]
    fn app_error_maps_to_attribution() {
        assert_eq!(
            AppError::Unauthorized.log_error().kind,
            ErrorKind::Unauthorized
        );
        assert_eq!(
            AppError::ModelNotFound("x".into()).log_error().kind,
            ErrorKind::RouteNotFound
        );
        assert_eq!(
            AppError::message("boom").log_error().kind,
            ErrorKind::InternalError
        );
        let quota = AppError::QuotaExceeded {
            name: "k".into(),
            spent: 1.0,
            limit: 1.0,
            period: "每日".into(),
        };
        assert_eq!(quota.log_error().kind, ErrorKind::QuotaExceeded);
        assert_eq!(
            AppError::Db(rusqlite::Error::QueryReturnedNoRows)
                .log_error()
                .kind,
            ErrorKind::StorageError
        );
    }
}
