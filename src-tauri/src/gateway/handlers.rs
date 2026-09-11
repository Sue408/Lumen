use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Local;
use serde_json::{json, Value};

use crate::db::keys::{find_enabled_virtual_key, virtual_key_spend};
use crate::db::models::{VirtualKey, PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI};
use crate::db::routes::list_enabled_aliases;
use crate::db::with_db;
use crate::error::AppError;
use crate::gateway::forward::{
    contains_cache_read, ensure_include_usage, error_response, request_id, send, stream_response,
    upstream_path_for,
};
use crate::gateway::quota::{is_over_quota, period_start, QuotaPeriod};
use crate::gateway::resolve::{resolve, ResolvedRoute};
use crate::gateway::usage::{build_log, extract_usage, record, LogContext, UsageTotals};
use crate::state::AppState;

const CHAT_ENDPOINT: &str = "/v1/chat/completions";
const MESSAGES_ENDPOINT: &str = "/v1/messages";
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(600);

pub async fn health() -> Response {
    Json(json!({ "status": "ok" })).into_response()
}

pub async fn list_models(State(state): State<Arc<AppState>>) -> Response {
    match with_db(&state.db, list_enabled_aliases).await {
        Ok(aliases) => {
            let data: Vec<Value> = aliases
                .into_iter()
                .map(|(id, display_name)| {
                    json!({
                        "id": id,
                        "object": "model",
                        "created": 0,
                        "owned_by": "lumen",
                        "display_name": display_name,
                    })
                })
                .collect();
            Json(json!({ "object": "list", "data": data })).into_response()
        }
        Err(error) => error.into_response(),
    }
}

pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    forward(state, headers, body, PROTOCOL_OPENAI, CHAT_ENDPOINT).await
}

/// Anthropic Messages 直通：交给协议为 anthropic 的上游处理，不做协议转换。
pub async fn messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    forward(state, headers, body, PROTOCOL_ANTHROPIC, MESSAGES_ENDPOINT).await
}

/// 端点协议与路由协议绑定：不匹配则在网关处拒绝，绝不把错误形状的 body 盲发上游。
async fn forward(
    state: Arc<AppState>,
    headers: HeaderMap,
    mut body: Value,
    required_protocol: &str,
    endpoint: &str,
) -> Response {
    let alias = match body.get("model").and_then(Value::as_str) {
        Some(alias) if !alias.is_empty() => alias.to_string(),
        _ => return error_response(StatusCode::BAD_REQUEST, "请求缺少 model 字段"),
    };
    let is_stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);

    // 强制鉴权：无有效 key 直接拒绝，并落一条无归属的 error 日志。
    let virtual_key = match authenticate(&state, &headers).await {
        Ok(key) => key,
        Err(error) => {
            reject(&state, endpoint, &alias, is_stream, None, None, &error).await;
            return error.into_response();
        }
    };
    let key_id = virtual_key.id.clone();

    // 额度闸门：建连前看「已累计花费」是否已达上限；未达即放行，最后一次可能略微超出。
    let period = QuotaPeriod::parse(&virtual_key.quota_period);
    let spend_start = period_start(period, Local::now());
    let spend_key = key_id.clone();
    let spent =
        match with_db(&state.db, move |conn| virtual_key_spend(conn, &spend_key, spend_start)).await
        {
            Ok(spent) => spent,
            Err(error) => {
                reject(
                    &state,
                    endpoint,
                    &alias,
                    is_stream,
                    None,
                    Some(key_id.clone()),
                    &error,
                )
                .await;
                return error.into_response();
            }
        };
    if let Some(limit) = virtual_key.quota_limit {
        if is_over_quota(spent, virtual_key.quota_limit) {
            let error = AppError::QuotaExceeded {
                name: virtual_key.name.clone(),
                spent,
                limit,
                period: period_label(period).to_string(),
            };
            reject(
                &state,
                endpoint,
                &alias,
                is_stream,
                None,
                Some(key_id.clone()),
                &error,
            )
            .await;
            return error.into_response();
        }
    }

    let route = match resolve(&state, &alias).await {
        Ok(Some(route)) => route,
        Ok(None) => {
            let error = AppError::ModelNotFound(alias.clone());
            reject(
                &state,
                endpoint,
                &alias,
                is_stream,
                None,
                Some(key_id.clone()),
                &error,
            )
            .await;
            return error.into_response();
        }
        Err(error) => return error.into_response(),
    };

    if route.route_protocol != required_protocol {
        let error = AppError::ProtocolMismatch {
            alias: alias.clone(),
            expected: required_protocol.to_string(),
            actual: route.route_protocol.clone(),
        };
        reject(
            &state,
            endpoint,
            &alias,
            is_stream,
            Some(route),
            Some(key_id.clone()),
            &error,
        )
        .await;
        return error.into_response();
    }
    if route.route_protocol != route.upstream_protocol {
        let error = AppError::message(format!(
            "配置不一致：路由 {alias} 声明为 {} 协议，但上游提供商为 {} 协议",
            route.route_protocol, route.upstream_protocol
        ));
        reject(
            &state,
            endpoint,
            &alias,
            is_stream,
            Some(route),
            Some(key_id.clone()),
            &error,
        )
        .await;
        return error.into_response();
    }

    let upstream_path = upstream_path_for(&route.upstream_protocol);
    body["model"] = Value::String(route.model_id.clone());
    ensure_include_usage(&mut body, &route.upstream_protocol, is_stream);
    let cache_in_input = contains_cache_read(&route.upstream_protocol);
    let started = Instant::now();
    let timeout = if is_stream { None } else { Some(UPSTREAM_TIMEOUT) };
    let response = match send(&state, &route, &body, upstream_path, timeout).await {
        Ok(response) => response,
        Err(error) => {
            let log = build_log(LogContext {
                endpoint: endpoint.to_string(),
                method: "POST".to_string(),
                alias: alias.clone(),
                kind: "chat".to_string(),
                is_stream,
                route: Some(route.clone()),
                latency_ms: started.elapsed().as_millis() as i64,
                status: "error".to_string(),
                http_status: None,
                error_message: Some(error.to_string()),
                request_id: None,
                virtual_key_id: Some(key_id.clone()),
                usage: UsageTotals::missing(),
            });
            let _ = record(&state, log).await;
            return error.into_response();
        }
    };
    let request_id = request_id(response.headers());

    if is_stream {
        return stream_response(state, route, alias, endpoint.to_string(), Some(key_id), response);
    }

    let status = response.status();
    let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
    let text = match response.text().await {
        Ok(text) => text,
        Err(error) => return crate::error::AppError::from(error).into_response(),
    };
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let usage = extract_usage(&value, cache_in_input).unwrap_or_else(UsageTotals::missing);

    let log = build_log(LogContext {
        endpoint: endpoint.to_string(),
        method: "POST".to_string(),
        alias,
        kind: "chat".to_string(),
        is_stream: false,
        route: Some(route),
        latency_ms: started.elapsed().as_millis() as i64,
        status: if status.is_success() {
            "success".to_string()
        } else {
            "error".to_string()
        },
        http_status: Some(status.as_u16() as i64),
        error_message: if status.is_success() {
            None
        } else {
            Some(error_message(&value, &text))
        },
        request_id,
        virtual_key_id: Some(key_id),
        usage,
    });
    let _ = record(&state, log).await;

    let mut output = (status, text).into_response();
    if let Some(content_type) = content_type {
        output.headers_mut().insert(header::CONTENT_TYPE, content_type);
    }
    output
}

fn error_message(value: &Value, fallback: &str) -> String {
    value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            if fallback.is_empty() {
                "上游返回错误".to_string()
            } else {
                fallback.to_string()
            }
        })
}

fn reject_status(error: &AppError) -> i64 {
    match error {
        AppError::ModelNotFound(_) => 404,
        AppError::ProtocolMismatch { .. } => 400,
        AppError::Unauthorized => 401,
        AppError::QuotaExceeded { .. } => 429,
        _ => 500,
    }
}

/// 从 `Authorization: Bearer` 或 `x-api-key` 中取出密钥原文。
fn extract_key(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        let value = value.trim();
        if let Some(token) = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
        {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// 解析并校验虚拟密钥；缺失、未知或已停用一律视为未授权。
async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<VirtualKey, AppError> {
    let key = extract_key(headers).ok_or(AppError::Unauthorized)?;
    with_db(&state.db, move |conn| find_enabled_virtual_key(conn, &key))
        .await?
        .ok_or(AppError::Unauthorized)
}

fn period_label(period: QuotaPeriod) -> &'static str {
    match period {
        QuotaPeriod::Daily => "每日",
        QuotaPeriod::Weekly => "每周",
        QuotaPeriod::Monthly => "每月",
        QuotaPeriod::Total => "一次性总额",
    }
}

/// 在进入转发前拒绝请求（未授权 / 超限 / 未找到模型 / 协议不匹配 / 配置不一致），并落一条 error 日志。
async fn reject(
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
        http_status: Some(reject_status(error)),
        error_message: Some(error.to_string()),
        request_id: None,
        virtual_key_id,
        usage: UsageTotals::missing(),
    });
    let _ = record(state, log).await;
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::db::keys::save_virtual_key;
    use crate::db::logs::{list_logs, LogFilter};
    use crate::db::models::{
        ProviderInput, RequestLog, RouteInput, RouteTargetInput, UpstreamModelInput,
        VirtualKeyInput,
    };
    use crate::db::{open_in_memory, providers, routes, Db};
    use crate::state::{EventSink, GatewayStatus};

    #[derive(Default)]
    struct MockSink {
        logs: Mutex<Vec<RequestLog>>,
    }

    impl EventSink for MockSink {
        fn status(&self, _status: &GatewayStatus) {}
        fn log(&self, log: &RequestLog) {
            self.logs.lock().unwrap().push(log.clone());
        }
    }

    async fn serve(app: axum::Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}")
    }

    async fn start_mock_upstream() -> String {
        let app = axum::Router::new().route(
            "/chat/completions",
            axum::routing::post(|| async {
                Json(json!({
                    "id": "cmpl-1",
                    "choices": [{ "message": { "role": "assistant", "content": "hi" } }],
                    "usage": { "prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150 }
                }))
            }),
        );
        serve(app).await
    }

    async fn anthropic_messages(headers: axum::http::HeaderMap) -> Json<Value> {
        let version = headers
            .get("anthropic-version")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let key = headers
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        Json(json!({
            "id": "msg_1",
            "type": "message",
            "content": [{ "type": "text", "text": "hi" }],
            "usage": { "input_tokens": 20, "output_tokens": 10 },
            "_receivedVersion": version,
            "_receivedKey": key
        }))
    }

    fn seed_upstream(db: &Db, base_url: &str, protocol: &str, alias: &str) {
        let conn = db.lock().unwrap();
        let provider = providers::save_provider(
            &conn,
            &ProviderInput {
                id: None,
                name: format!("mock-{protocol}"),
                base_url: base_url.to_string(),
                api_key: "secret".into(),
                auth_scheme: if protocol == "anthropic" {
                    "x-api-key".into()
                } else {
                    "bearer".into()
                },
                protocol: protocol.into(),
                extra_headers: std::collections::BTreeMap::new(),
                icon: None,
                icon_tint: "ink".into(),
                enabled: true,
            },
        )
        .unwrap();
        let model = providers::save_upstream_model(
            &conn,
            &UpstreamModelInput {
                id: None,
                provider_id: provider.id,
                model_id: "mock-model".into(),
                display_name: "Mock".into(),
                input_price: 1.0,
                output_price: 2.0,
                cache_read_price: 0.0,
                cache_creation_price: 0.0,
                context_window: 0,
                capabilities: Vec::new(),
                icon: None,
                icon_tint: "ink".into(),
                enabled: true,
            },
        )
        .unwrap();
        routes::save_route(
            &conn,
            &RouteInput {
                id: None,
                alias: alias.into(),
                display_name: "Mock".into(),
                protocol: protocol.into(),
                enabled: true,
                targets: vec![RouteTargetInput {
                    upstream_model_id: model.id,
                    priority: 0,
                    enabled: true,
                }],
            },
        )
        .unwrap();
    }

    const TEST_KEY: &str = "sk-lumen-test";

    fn seed_virtual_key(
        db: &Db,
        key: &str,
        enabled: bool,
        quota_limit: Option<f64>,
        quota_period: &str,
    ) -> VirtualKey {
        let conn = db.lock().unwrap();
        save_virtual_key(
            &conn,
            &VirtualKeyInput {
                id: None,
                key: Some(key.to_string()),
                name: "测试密钥".to_string(),
                enabled,
                quota_limit,
                quota_period: quota_period.to_string(),
            },
        )
        .unwrap()
    }

    fn post(uri: &str, body: Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_KEY}"))
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn post_without_key(uri: &str, body: Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn forwards_chat_and_records_usage_and_cost() {
        let base_url = start_mock_upstream().await;
        let db = open_in_memory().unwrap();
        seed_upstream(&db, &base_url, "openai", "lumen/mock");
        seed_virtual_key(&db, TEST_KEY, true, None, "monthly");
        let sink = Arc::new(MockSink::default());
        let state = Arc::new(AppState::new(
            db.clone(),
            reqwest::Client::new(),
            sink.clone(),
            0,
        ));
        let router = crate::gateway::build_router(state);

        let response = router
            .oneshot(post(
                "/v1/chat/completions",
                json!({ "model": "lumen/mock", "messages": [] }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["usage"]["total_tokens"], 150);

        let logs = {
            let conn = db.lock().unwrap();
            list_logs(&conn, &LogFilter::default()).unwrap()
        };
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].route_alias.as_deref(), Some("lumen/mock"));
        assert_eq!(logs[0].total_tokens, 150);
        assert!((logs[0].cost - 0.0002).abs() < 1e-9, "cost was {}", logs[0].cost);
        assert_eq!(sink.logs.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn unknown_model_returns_404_and_records_failure() {
        let db = open_in_memory().unwrap();
        seed_virtual_key(&db, TEST_KEY, true, None, "monthly");
        let sink = Arc::new(MockSink::default());
        let state = Arc::new(AppState::new(db.clone(), reqwest::Client::new(), sink, 0));
        let router = crate::gateway::build_router(state);

        let response = router
            .oneshot(post("/v1/chat/completions", json!({ "model": "missing" })))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let logs = {
            let conn = db.lock().unwrap();
            list_logs(&conn, &LogFilter::default()).unwrap()
        };
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, "error");
        assert_eq!(logs[0].http_status, Some(404));
    }

    #[tokio::test]
    async fn anthropic_messages_passthrough_sets_headers_and_records_usage() {
        let app = axum::Router::new().route(
            "/messages",
            axum::routing::post(anthropic_messages),
        );
        let base_url = serve(app).await;
        let db = open_in_memory().unwrap();
        seed_upstream(&db, &base_url, "anthropic", "lumen/claude");
        seed_virtual_key(&db, TEST_KEY, true, None, "monthly");
        let sink = Arc::new(MockSink::default());
        let state = Arc::new(AppState::new(db.clone(), reqwest::Client::new(), sink, 0));
        let router = crate::gateway::build_router(state);

        let response = router
            .oneshot(post(
                "/v1/messages",
                json!({ "model": "lumen/claude", "max_tokens": 64, "messages": [] }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["_receivedVersion"], "2023-06-01");
        assert_eq!(value["_receivedKey"], "secret");

        let logs = {
            let conn = db.lock().unwrap();
            list_logs(&conn, &LogFilter::default()).unwrap()
        };
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].route_alias.as_deref(), Some("lumen/claude"));
        assert_eq!(logs[0].total_tokens, 30);
        assert_eq!(logs[0].endpoint, "/v1/messages");
        assert!((logs[0].cost - 0.00004).abs() < 1e-9, "cost was {}", logs[0].cost);
    }

    #[tokio::test]
    async fn chat_endpoint_rejects_anthropic_route() {
        let db = open_in_memory().unwrap();
        seed_upstream(&db, "http://127.0.0.1:1", "anthropic", "lumen/claude");
        seed_virtual_key(&db, TEST_KEY, true, None, "monthly");
        let sink = Arc::new(MockSink::default());
        let state = Arc::new(AppState::new(db.clone(), reqwest::Client::new(), sink, 0));
        let router = crate::gateway::build_router(state);

        let response = router
            .oneshot(post(
                "/v1/chat/completions",
                json!({ "model": "lumen/claude", "messages": [] }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["error"]["code"], "protocol_mismatch");

        let logs = {
            let conn = db.lock().unwrap();
            list_logs(&conn, &LogFilter::default()).unwrap()
        };
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, "error");
        assert_eq!(logs[0].http_status, Some(400));
    }

    #[tokio::test]
    async fn messages_endpoint_rejects_openai_route() {
        let db = open_in_memory().unwrap();
        seed_upstream(&db, "http://127.0.0.1:1", "openai", "lumen/gpt");
        seed_virtual_key(&db, TEST_KEY, true, None, "monthly");
        let sink = Arc::new(MockSink::default());
        let state = Arc::new(AppState::new(db.clone(), reqwest::Client::new(), sink, 0));
        let router = crate::gateway::build_router(state);

        let response = router
            .oneshot(post(
                "/v1/messages",
                json!({ "model": "lumen/gpt", "max_tokens": 8, "messages": [] }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let logs = {
            let conn = db.lock().unwrap();
            list_logs(&conn, &LogFilter::default()).unwrap()
        };
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].http_status, Some(400));
    }

    #[tokio::test]
    async fn missing_key_is_rejected_with_401() {
        let db = open_in_memory().unwrap();
        seed_virtual_key(&db, TEST_KEY, true, None, "monthly");
        let sink = Arc::new(MockSink::default());
        let state = Arc::new(AppState::new(db.clone(), reqwest::Client::new(), sink, 0));
        let router = crate::gateway::build_router(state);

        let response = router
            .oneshot(post_without_key(
                "/v1/chat/completions",
                json!({ "model": "lumen/mock" }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let logs = {
            let conn = db.lock().unwrap();
            list_logs(&conn, &LogFilter::default()).unwrap()
        };
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, "error");
        assert_eq!(logs[0].http_status, Some(401));
        assert!(logs[0].virtual_key_id.is_none());
    }

    #[tokio::test]
    async fn disabled_key_is_rejected_with_401() {
        let db = open_in_memory().unwrap();
        seed_virtual_key(&db, TEST_KEY, false, None, "monthly");
        let sink = Arc::new(MockSink::default());
        let state = Arc::new(AppState::new(db.clone(), reqwest::Client::new(), sink, 0));
        let router = crate::gateway::build_router(state);

        let response = router
            .oneshot(post(
                "/v1/chat/completions",
                json!({ "model": "lumen/mock" }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn over_quota_key_is_rejected_with_429() {
        let db = open_in_memory().unwrap();
        seed_virtual_key(&db, TEST_KEY, true, Some(0.0), "monthly");
        let sink = Arc::new(MockSink::default());
        let state = Arc::new(AppState::new(db.clone(), reqwest::Client::new(), sink, 0));
        let router = crate::gateway::build_router(state);

        let response = router
            .oneshot(post(
                "/v1/chat/completions",
                json!({ "model": "lumen/mock" }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

        let logs = {
            let conn = db.lock().unwrap();
            list_logs(&conn, &LogFilter::default()).unwrap()
        };
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, "error");
        assert_eq!(logs[0].http_status, Some(429));
    }

    #[tokio::test]
    async fn forwards_chat_and_records_virtual_key() {
        let base_url = start_mock_upstream().await;
        let db = open_in_memory().unwrap();
        seed_upstream(&db, &base_url, "openai", "lumen/mock");
        let key = seed_virtual_key(&db, TEST_KEY, true, None, "monthly");
        let sink = Arc::new(MockSink::default());
        let state = Arc::new(AppState::new(db.clone(), reqwest::Client::new(), sink, 0));
        let router = crate::gateway::build_router(state);

        let response = router
            .oneshot(post(
                "/v1/chat/completions",
                json!({ "model": "lumen/mock", "messages": [] }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let logs = {
            let conn = db.lock().unwrap();
            list_logs(&conn, &LogFilter::default()).unwrap()
        };
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].virtual_key_id.as_deref(), Some(key.id.as_str()));
    }
}
