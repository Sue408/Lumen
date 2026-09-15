use axum::http::StatusCode;

use crate::error::{AppError, LogError};
use crate::gateway::resolve::ResolvedRoute;
use crate::gateway::usage::{build_log, record, LogContext, UsageTotals};
use crate::state::AppState;

/// 在进入转发前拒绝请求（未授权 / 超限 / 未找到模型 / 协议不匹配 / 配置不一致），
/// 并落一条 error 日志。归因取 `AppError::log_error()` 的默认映射。
#[allow(clippy::too_many_arguments)]
pub async fn reject(
    state: &AppState,
    endpoint: &str,
    alias: &str,
    is_stream: bool,
    route: Option<ResolvedRoute>,
    virtual_key_id: Option<String>,
    error: &AppError,
    trace_id: Option<String>,
) {
    reject_with(
        state,
        endpoint,
        alias,
        is_stream,
        route,
        virtual_key_id,
        error.status_code(),
        error.log_error(),
        trace_id,
    )
    .await;
}

/// 与 `reject` 同，但允许调用方给出更精确的状态码与归因——例如区分「别名不存在」
/// 与「别名在但无可用候选」这两种同为 404 的情况。
#[allow(clippy::too_many_arguments)]
pub async fn reject_with(
    state: &AppState,
    endpoint: &str,
    alias: &str,
    is_stream: bool,
    route: Option<ResolvedRoute>,
    virtual_key_id: Option<String>,
    status: StatusCode,
    error: LogError,
    trace_id: Option<String>,
) {
    let log = build_log(LogContext {
        endpoint: endpoint.to_string(),
        alias: alias.to_string(),
        is_stream,
        route,
        latency_ms: 0,
        status: "error".to_string(),
        http_status: Some(status.as_u16() as i64),
        error: Some(error),
        request_id: None,
        virtual_key_id,
        usage: UsageTotals::missing(),
        attempt_index: 0,
        session_id: None,
        trace_id,
    });
    let _ = record(state, log).await;
}
