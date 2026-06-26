use thiserror::Error;

pub type ApplicationResult<T> = Result<T, ApplicationError>;

/// 应用层统一使用的错误类型，后续再由 HTTP 层转换成响应。
#[derive(Debug, Error)]
pub enum ApplicationError {
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
            Self::TokenExpired => "TOKEN_EXPIRED",
            Self::InvalidToken => "INVALID_TOKEN",
            Self::InvalidRefreshToken => "INVALID_REFRESH_TOKEN",
            Self::RefreshTokenExpired => "REFRESH_TOKEN_EXPIRED",
            Self::SessionRevoked => "SESSION_REVOKED",
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
    pub fn auth_token(code: AuthTokenErrorCode) -> Self {
        Self::AuthToken {
            code,
            message: code.message().to_string(),
        }
    }
}
