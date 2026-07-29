use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;

pub type ApplicationResult<T> = Result<T, ApplicationError>;
pub type BusinessErrorParams = BTreeMap<String, Value>;

/// 应用层统一使用的错误类型，后续再由 HTTP 层转换成响应。
#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("{0}")]
    Business(#[from] BusinessError),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{message}")]
    AuthToken {
        code: AuthTokenErrorCode,
        message: String,
    },
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// 与传输协议无关的业务错误分类。
///
/// `code` 和 `params` 是客户端本地化所需的稳定契约；`diagnostic_message`
/// 只用于服务端日志和未知错误码兜底，不应作为客户端主文案。
#[derive(Debug, Error)]
#[error("{diagnostic_message}")]
pub struct BusinessError {
    kind: BusinessErrorKind,
    code: &'static str,
    params: BusinessErrorParams,
    diagnostic_message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusinessErrorKind {
    Validation,
    Conflict,
    Unauthorized,
    Forbidden,
    NotFound,
}

impl BusinessError {
    pub fn new(
        kind: BusinessErrorKind,
        code: &'static str,
        params: BusinessErrorParams,
        diagnostic_message: impl Into<String>,
    ) -> Self {
        debug_assert!(
            !code.trim().is_empty(),
            "business error code cannot be empty"
        );
        Self {
            kind,
            code,
            params,
            diagnostic_message: diagnostic_message.into(),
        }
    }

    pub fn kind(&self) -> BusinessErrorKind {
        self.kind
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn params(&self) -> &BusinessErrorParams {
        &self.params
    }

    pub fn diagnostic_message(&self) -> &str {
        &self.diagnostic_message
    }

    pub fn into_parts(self) -> (BusinessErrorKind, &'static str, BusinessErrorParams, String) {
        (self.kind, self.code, self.params, self.diagnostic_message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthTokenErrorCode {
    TokenExpired,
    InvalidToken,
    InvalidRefreshToken,
    RefreshTokenExpired,
    SessionRevoked,
}

impl AuthTokenErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TokenExpired => "token_expired",
            Self::InvalidToken => "invalid_token",
            Self::InvalidRefreshToken => "invalid_refresh_token",
            Self::RefreshTokenExpired => "refresh_token_expired",
            Self::SessionRevoked => "session_revoked",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::TokenExpired => "Access token expired",
            Self::InvalidToken => "Invalid access token",
            Self::InvalidRefreshToken => "Invalid refresh token",
            Self::RefreshTokenExpired => "Refresh token expired",
            Self::SessionRevoked => "Session revoked",
        }
    }
}

impl ApplicationError {
    pub fn business(
        kind: BusinessErrorKind,
        code: &'static str,
        params: BusinessErrorParams,
        diagnostic_message: impl Into<String>,
    ) -> Self {
        BusinessError::new(kind, code, params, diagnostic_message).into()
    }

    pub fn validation(
        code: &'static str,
        params: BusinessErrorParams,
        diagnostic_message: impl Into<String>,
    ) -> Self {
        Self::business(
            BusinessErrorKind::Validation,
            code,
            params,
            diagnostic_message,
        )
    }

    pub fn conflict(
        code: &'static str,
        params: BusinessErrorParams,
        diagnostic_message: impl Into<String>,
    ) -> Self {
        Self::business(
            BusinessErrorKind::Conflict,
            code,
            params,
            diagnostic_message,
        )
    }

    pub fn unauthorized(
        code: &'static str,
        params: BusinessErrorParams,
        diagnostic_message: impl Into<String>,
    ) -> Self {
        Self::business(
            BusinessErrorKind::Unauthorized,
            code,
            params,
            diagnostic_message,
        )
    }

    pub fn forbidden(
        code: &'static str,
        params: BusinessErrorParams,
        diagnostic_message: impl Into<String>,
    ) -> Self {
        Self::business(
            BusinessErrorKind::Forbidden,
            code,
            params,
            diagnostic_message,
        )
    }

    pub fn not_found(
        code: &'static str,
        params: BusinessErrorParams,
        diagnostic_message: impl Into<String>,
    ) -> Self {
        Self::business(
            BusinessErrorKind::NotFound,
            code,
            params,
            diagnostic_message,
        )
    }

    pub fn auth_token(code: AuthTokenErrorCode) -> Self {
        Self::AuthToken {
            code,
            message: code.message().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ApplicationError, BusinessErrorKind};
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn business_error_keeps_localization_contract_separate_from_diagnostics() {
        let error = ApplicationError::validation(
            "field_too_long",
            BTreeMap::from([
                ("field".to_string(), json!("account")),
                ("max".to_string(), json!(254)),
            ]),
            "username must be at most 254 characters long",
        );

        let ApplicationError::Business(error) = error else {
            panic!("expected structured business error");
        };
        assert_eq!(error.kind(), BusinessErrorKind::Validation);
        assert_eq!(error.code(), "field_too_long");
        assert_eq!(error.params()["field"], "account");
        assert_eq!(error.params()["max"], 254);
        assert_eq!(
            error.diagnostic_message(),
            "username must be at most 254 characters long"
        );
    }
}
