pub mod forward;
pub mod handlers;
pub mod quota;
pub mod resolve;
pub mod usage;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::oneshot;

use crate::error::AppError;
use crate::state::{AppState, GatewayHandle, GatewayStatus};

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/v1/models", get(handlers::list_models))
        .route("/v1/chat/completions", post(handlers::chat_completions))
        .route("/v1/messages", post(handlers::messages))
        .route("/v1/responses", post(handlers::responses))
        .route("/v1beta/models/{*model_action}", post(handlers::gemini_generate))
        .with_state(state)
}

/// 绑定回环端口并启动 axum 服务；已在运行时返回错误。
pub async fn start(state: Arc<AppState>) -> Result<(), AppError> {
    {
        let guard = state
            .gateway
            .lock()
            .map_err(|_| AppError::message("网关状态锁已中毒"))?;
        if guard.is_some() {
            return Err(AppError::AlreadyRunning);
        }
    }

    let port = state.port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let router = build_router(state.clone());
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    {
        let mut guard = state
            .gateway
            .lock()
            .map_err(|_| AppError::message("网关状态锁已中毒"))?;
        *guard = Some(GatewayHandle {
            port,
            shutdown: Some(shutdown_tx),
            task,
        });
    }

    state.events.status(&GatewayStatus::running(port));
    Ok(())
}

/// 触发优雅关闭并等待服务任务结束。
pub async fn stop(state: &Arc<AppState>) -> Result<(), AppError> {
    let handle = {
        let mut guard = state
            .gateway
            .lock()
            .map_err(|_| AppError::message("网关状态锁已中毒"))?;
        guard.take()
    };
    let Some(mut handle) = handle else {
        return Err(AppError::NotRunning);
    };
    if let Some(shutdown) = handle.shutdown.take() {
        let _ = shutdown.send(());
    }
    let _ = handle.task.await;
    state.events.status(&GatewayStatus::stopped(handle.port));
    Ok(())
}
