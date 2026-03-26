use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// 把应用层错误统一映射成稳定的 HTTP 响应格式。
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Conflict(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    RangeNotSatisfiable { message: String, file_size: u64 },
    Internal,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    error: String,
}

impl From<mova_application::ApplicationError> for ApiError {
    fn from(error: mova_application::ApplicationError) -> Self {
        match error {
            mova_application::ApplicationError::Validation(message) => Self::BadRequest(message),
            mova_application::ApplicationError::Conflict(message) => Self::Conflict(message),
            mova_application::ApplicationError::Unauthorized(message) => {
                Self::Unauthorized(message)
            }
            mova_application::ApplicationError::Forbidden(message) => Self::Forbidden(message),
            mova_application::ApplicationError::NotFound(message) => Self::NotFound(message),
            mova_application::ApplicationError::Unexpected(source) => {
                // 详细错误打到日志里，接口只返回通用错误，避免把内部实现细节暴露给客户端。
                tracing::error!(error = ?source, "application request failed");
                Self::Internal
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Conflict(message) => (StatusCode::CONFLICT, message),
            Self::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message),
            Self::Forbidden(message) => (StatusCode::FORBIDDEN, message),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message),
            Self::RangeNotSatisfiable { message, file_size } => {
                let mut response = (
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    Json(ApiErrorBody { error: message }),
                )
                    .into_response();

                response.headers_mut().insert(
                    axum::http::header::CONTENT_RANGE,
                    axum::http::HeaderValue::from_str(&format!("bytes */{}", file_size))
                        .unwrap_or_else(|_| axum::http::HeaderValue::from_static("bytes */0")),
                );

                return response;
            }
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal server error".to_string(),
            ),
        };

        (status, Json(ApiErrorBody { error: message })).into_response()
    }
}
