use crate::error::AppError;
use crate::gateway::resolve::ResolvedRoute;
use crate::gateway::usage::{build_log, record, LogContext, UsageTotals};
use crate::state::AppState;

/// 在进入转发前拒绝请求（未授权 / 超限 / 未找到模型 / 协议不匹配 / 配置不一致），
/// 并落一条 error 日志。
pub async fn reject(
    state: &AppState,
    endpoint: &str,
    alias: &str,
    is_stream: bool,
    route: Option<ResolvedRoute>,
    virtual_key_id: Option<String>,
    error: &AppError,
) {
    let log = build_log(LogContext {
        endpoint: endpoint.to_string(),
        method: "POST".to_string(),
        alias: alias.to_string(),
        kind: "chat".to_string(),
        is_stream,
        route,
        latency_ms: 0,
        status: "error".to_string(),
        http_status: Some(error.status_code().as_u16() as i64),
        error_message: Some(error.to_string()),
        request_id: None,
        virtual_key_id,
        usage: UsageTotals::missing(),
    });
    let _ = record(state, log).await;
}
