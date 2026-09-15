use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use serde_json::Value;

use crate::error::{ErrorKind, LogError};
use crate::gateway::forward::error_response;
use crate::gateway::usage::{build_log, record, LogContext, UsageTotals};
use crate::state::AppState;

/// 网关入口的 JSON body 上限。
///
/// axum 对 `Json` / `Bytes` 这类提取器的默认上限只有 2 MiB，而一次真实的 LLM 请求带上
/// 长上下文、图片或大 tool 结果很容易越过；上限过低会让网关在**进 handler 之前**就吐
/// 413，请求连调用流水都留不下，只余一条运行日志。这里放宽到 64 MiB：覆盖真实请求，
/// 又不至于在本地回环上完全失去兜底。
pub const MAX_REQUEST_BODY: usize = 64 * 1024 * 1024;

/// 网关入口的 JSON body 提取器。
///
/// 语义等价于裸 `Json<Value>`，差别只在失败路径：既把提取失败映射成网关既有的结构化
/// JSON 错误（`forward::error_response`，超限时补中文提示），也补落一条 `client` 归因的
/// 流水——否则这些请求在账本里完全不可见。axum 默认对超限只回纯文本 413，客户端拿不到
/// 形状稳定的错误体，极难定位。
pub struct JsonBody(pub Value);

impl FromRequest<Arc<AppState>> for JsonBody {
    type Rejection = Response;

    async fn from_request(
        request: Request,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        // 提取失败发生在任何 handler 之前，端点只能就地取。
        let endpoint = request.uri().path().to_string();
        match Json::<Value>::from_request(request, state).await {
            Ok(Json(value)) => Ok(JsonBody(value)),
            Err(rejection) => {
                let (status, kind, message) = rejection_parts(&rejection);
                let log = build_log(LogContext {
                    endpoint,
                    // 提取失败时别名与虚拟密钥都还没有，留空由 `build_log` 归一为 NULL。
                    alias: String::new(),
                    is_stream: false,
                    route: None,
                    latency_ms: 0,
                    status: "error".to_string(),
                    http_status: Some(status.as_u16() as i64),
                    error: Some(LogError::new(kind, message.clone())),
                    request_id: None,
                    virtual_key_id: None,
                    usage: UsageTotals::missing(),
                    attempt_index: 0,
                    session_id: None,
                    trace_id: None,
                });
                let _ = record(state, log).await;
                Err(error_response(status, &message))
            }
        }
    }
}

/// 把提取拒绝拆成：HTTP 状态、归因类目、人话（超限补上限提示）。
fn rejection_parts(rejection: &JsonRejection) -> (StatusCode, ErrorKind, String) {
    let status = rejection.status();
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        let limit_mib = MAX_REQUEST_BODY / (1024 * 1024);
        let message = format!(
            "请求体超过网关上限（{limit_mib} MiB），已拒绝：{}",
            rejection.body_text()
        );
        (status, ErrorKind::PayloadTooLarge, message)
    } else {
        (status, ErrorKind::MalformedBody, rejection.body_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::extract::{DefaultBodyLimit, Request};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    use crate::db::logs::{list_logs, LogFilter};
    use crate::db::open_in_memory;
    use crate::state::{EventSink, GatewayStatus};

    #[derive(Default)]
    struct SilentSink;

    impl EventSink for SilentSink {
        fn status(&self, _status: &GatewayStatus) {}
        fn log(&self, _log: &crate::db::models::RequestLog) {}
    }

    fn state() -> Arc<AppState> {
        Arc::new(AppState::new(
            open_in_memory().unwrap(),
            reqwest::Client::new(),
            Arc::new(SilentSink),
            0,
        ))
    }

    async fn ok(JsonBody(_): JsonBody) -> Response {
        StatusCode::OK.into_response()
    }

    fn request_with(body: &str) -> Request {
        Request::builder()
            .method("POST")
            .uri("/responses")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn oversized_body_returns_structured_413_and_is_logged() {
        let state = state();
        let app = Router::new()
            .route("/responses", post(ok))
            .layer(DefaultBodyLimit::max(16))
            .with_state(state.clone());
        let response = app
            .oneshot(request_with(r#"{"model":"far larger than sixteen bytes"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("超过网关上限"),
            "{value}"
        );

        let logs = {
            let conn = state.db.lock().unwrap();
            list_logs(&conn, &LogFilter::default()).unwrap()
        };
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].error_domain.as_deref(), Some("client"));
        assert_eq!(logs[0].error_kind.as_deref(), Some("payload_too_large"));
        assert!(logs[0].route_alias.is_none(), "无别名应落 NULL");
    }

    #[tokio::test]
    async fn malformed_json_keeps_status_and_shape() {
        let state = state();
        let app = Router::new()
            .route("/responses", post(ok))
            .layer(DefaultBodyLimit::max(1024))
            .with_state(state.clone());
        let response = app.oneshot(request_with("{ not json")).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["error"]["type"], "lumen_error");

        let logs = {
            let conn = state.db.lock().unwrap();
            list_logs(&conn, &LogFilter::default()).unwrap()
        };
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].error_kind.as_deref(), Some("malformed_body"));
    }
}
