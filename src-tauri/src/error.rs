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
