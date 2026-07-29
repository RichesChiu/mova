use crate::response::ApiEnvelope;
use axum::{
    body::{to_bytes, Body},
    http::{
        header::{CONTENT_LENGTH, CONTENT_TYPE},
        Request, StatusCode,
    },
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::collections::BTreeMap;

/// 把应用层错误统一映射成稳定的 HTTP 响应格式。
#[derive(Debug)]
pub enum ApiError {
    Business {
        status: StatusCode,
        error_code: &'static str,
        params: BTreeMap<String, serde_json::Value>,
        diagnostic_message: String,
    },
    BadRequest(String),
    Conflict(String),
    Unauthorized(String),
    UnauthorizedCode {
        code: mova_application::AuthTokenErrorCode,
        message: String,
    },
    Forbidden(String),
    NotFound(String),
    TooManyRequests {
        message: String,
        retry_after_seconds: u64,
    },
    ServiceUnavailable(String),
    RangeNotSatisfiable {
        message: String,
        file_size: u64,
    },
    Internal,
}

impl From<mova_application::ApplicationError> for ApiError {
    fn from(error: mova_application::ApplicationError) -> Self {
        match error {
            mova_application::ApplicationError::Business(error) => {
                let (kind, error_code, params, diagnostic_message) = error.into_parts();
                let status = match kind {
                    mova_application::BusinessErrorKind::Validation => StatusCode::BAD_REQUEST,
                    mova_application::BusinessErrorKind::Conflict => StatusCode::CONFLICT,
                    mova_application::BusinessErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
                    mova_application::BusinessErrorKind::Forbidden => StatusCode::FORBIDDEN,
                    mova_application::BusinessErrorKind::NotFound => StatusCode::NOT_FOUND,
                };
                Self::Business {
                    status,
                    error_code,
                    params,
                    diagnostic_message,
                }
            }
            mova_application::ApplicationError::Validation(message) => Self::BadRequest(message),
            mova_application::ApplicationError::Conflict(message) => Self::Conflict(message),
            mova_application::ApplicationError::Unauthorized(message) => {
                Self::Unauthorized(message)
            }
            mova_application::ApplicationError::AuthToken { code, message } => {
                Self::UnauthorizedCode { code, message }
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

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!(error = ?error, "database request failed");
        Self::Internal
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_code, message, params, retry_after_seconds, content_range) = match self {
            Self::Business {
                status,
                error_code,
                params,
                diagnostic_message,
            } => (status, error_code, diagnostic_message, params, None, None),
            Self::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
                "invalid_request",
                message,
                BTreeMap::new(),
                None,
                None,
            ),
            Self::Conflict(message) => (
                StatusCode::CONFLICT,
                "resource_conflict",
                message,
                BTreeMap::new(),
                None,
                None,
            ),
            Self::Unauthorized(message) => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                message,
                BTreeMap::new(),
                None,
                None,
            ),
            Self::UnauthorizedCode { code, message } => (
                StatusCode::UNAUTHORIZED,
                code.as_str(),
                message,
                BTreeMap::new(),
                None,
                None,
            ),
            Self::Forbidden(message) => (
                StatusCode::FORBIDDEN,
                "forbidden",
                message,
                BTreeMap::new(),
                None,
                None,
            ),
            Self::NotFound(message) => (
                StatusCode::NOT_FOUND,
                "resource_not_found",
                message,
                BTreeMap::new(),
                None,
                None,
            ),
            Self::TooManyRequests {
                message,
                retry_after_seconds,
            } => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                message,
                BTreeMap::from([(
                    "retry_after_seconds".to_string(),
                    json!(retry_after_seconds),
                )]),
                Some(retry_after_seconds),
                None,
            ),
            Self::ServiceUnavailable(message) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                message,
                BTreeMap::new(),
                None,
                None,
            ),
            Self::RangeNotSatisfiable { message, file_size } => (
                StatusCode::RANGE_NOT_SATISFIABLE,
                "range_not_satisfiable",
                message,
                BTreeMap::from([("file_size".to_string(), json!(file_size))]),
                None,
                Some(file_size),
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "internal server error".to_string(),
                BTreeMap::new(),
                None,
                None,
            ),
        };

        let mut response = (
            status,
            Json(ApiEnvelope {
                code: status.as_u16(),
                error_code: Some(error_code),
                params: Some(params),
                message,
                data: (),
            }),
        )
            .into_response();

        if let Some(retry_after_seconds) = retry_after_seconds {
            response.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_str(&retry_after_seconds.to_string())
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("1")),
            );
        }
        if let Some(file_size) = content_range {
            response.headers_mut().insert(
                axum::http::header::CONTENT_RANGE,
                axum::http::HeaderValue::from_str(&format!("bytes */{}", file_size))
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("bytes */0")),
            );
        }

        response
    }
}

fn default_error_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "resource_not_found",
        StatusCode::CONFLICT => "resource_conflict",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        StatusCode::SERVICE_UNAVAILABLE => "service_unavailable",
        StatusCode::RANGE_NOT_SATISFIABLE => "range_not_satisfiable",
        status if status.is_client_error() => "invalid_request",
        _ => "internal_error",
    }
}

/// 把 Axum 产生的非 JSON 错误（例如 JSON 解析失败）归一化为 API 错误协议。
///
/// 业务错误已经通过 [`ApiError`] 返回 JSON，这里只处理仍会绕过业务错误映射的
/// 框架级响应。诊断文本保留在 `message`，客户端只根据 `error_code + params`
/// 选择本地化文案。
pub async fn normalize_api_error_response(request: Request<Body>, next: Next) -> Response {
    normalize_error_response(next.run(request).await).await
}

async fn normalize_error_response(response: Response) -> Response {
    let status = response.status();
    if !status.is_client_error() && !status.is_server_error() {
        return response;
    }

    let is_json = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if is_json {
        return response;
    }

    let (parts, body) = response.into_parts();
    let diagnostic_message = to_bytes(body, 64 * 1024)
        .await
        .ok()
        .and_then(|body| String::from_utf8(body.to_vec()).ok())
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| {
            status
                .canonical_reason()
                .unwrap_or("request failed")
                .to_string()
        });

    let mut normalized = (
        status,
        Json(ApiEnvelope {
            code: status.as_u16(),
            error_code: Some(default_error_code(status)),
            params: Some(BTreeMap::new()),
            message: diagnostic_message,
            data: (),
        }),
    )
        .into_response();

    for (name, value) in &parts.headers {
        if name != CONTENT_TYPE && name != CONTENT_LENGTH {
            normalized.headers_mut().append(name, value.clone());
        }
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::{default_error_code, normalize_error_response, ApiError};
    use axum::{
        body::to_bytes,
        http::{header, StatusCode},
        response::IntoResponse,
    };

    #[test]
    fn rate_limit_response_includes_retry_after() {
        let response = ApiError::TooManyRequests {
            message: "try again later".to_string(),
            retry_after_seconds: 90,
        }
        .into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers().get(header::RETRY_AFTER).unwrap(), "90");
    }

    #[test]
    fn framework_statuses_map_to_stable_error_codes() {
        assert_eq!(
            default_error_code(StatusCode::UNPROCESSABLE_ENTITY),
            "invalid_request"
        );
        assert_eq!(
            default_error_code(StatusCode::METHOD_NOT_ALLOWED),
            "invalid_request"
        );
        assert_eq!(
            default_error_code(StatusCode::NOT_FOUND),
            "resource_not_found"
        );
        assert_eq!(
            default_error_code(StatusCode::PAYLOAD_TOO_LARGE),
            "invalid_request"
        );
        assert_eq!(
            default_error_code(StatusCode::UNSUPPORTED_MEDIA_TYPE),
            "invalid_request"
        );
    }

    #[tokio::test]
    async fn structured_application_error_preserves_code_params_and_diagnostics() {
        let application_error = mova_application::ApplicationError::forbidden(
            "insufficient_privilege",
            std::collections::BTreeMap::from([(
                "required_role".to_string(),
                serde_json::json!("owner"),
            )]),
            "only the system owner can perform this operation",
        );
        let response = ApiError::from(application_error).into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error_code"], "insufficient_privilege");
        assert_eq!(payload["params"]["required_role"], "owner");
        assert_eq!(
            payload["message"],
            "only the system owner can perform this operation"
        );
    }

    #[tokio::test]
    async fn error_response_exposes_stable_localization_contract() {
        let response = ApiError::TooManyRequests {
            message: "try again later".to_string(),
            retry_after_seconds: 90,
        }
        .into_response();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(payload["code"], 429);
        assert_eq!(payload["error_code"], "rate_limited");
        assert_eq!(payload["params"]["retry_after_seconds"], 90);
        assert_eq!(payload["message"], "try again later");
        assert!(payload["data"].is_null());
    }

    #[tokio::test]
    async fn framework_plain_text_error_is_normalized() {
        let response = (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Failed to deserialize the JSON body",
        )
            .into_response();
        let response = normalize_error_response(response).await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(payload["code"], 422);
        assert_eq!(payload["error_code"], "invalid_request");
        assert_eq!(payload["params"], serde_json::json!({}));
        assert_eq!(payload["message"], "Failed to deserialize the JSON body");
    }
}
