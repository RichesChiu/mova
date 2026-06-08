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
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}
