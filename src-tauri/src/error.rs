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
    #[error("{0}")]
    Message(String),
}

impl AppError {
    pub fn message(message: impl Into<String>) -> Self {
        AppError::Message(message.into())
    }
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
        let status = match &self {
            AppError::ModelNotFound(_) => StatusCode::NOT_FOUND,
            AppError::NotRunning | AppError::AlreadyRunning => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
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
        AppError::AlreadyRunning => "already_running",
        AppError::NotRunning => "not_running",
        _ => "internal_error",
    }
}
