use std::fmt;

/// Stable, transport-independent failure categories for HTTP(S) STRM playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteStreamErrorKind {
    InvalidRequest,
    ReferenceTooLarge,
    ReferenceInvalid,
    CarrierNotFound,
    CarrierReadFailed,
    TargetForbidden,
    UserLimitExceeded,
    CapacityExhausted,
    SourceUnavailable,
    ResponseInvalid,
    SourceTimeout,
    RangeNotSupported,
}

impl RemoteStreamErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::ReferenceTooLarge => "strm_reference_too_large",
            Self::ReferenceInvalid => "strm_reference_invalid",
            Self::CarrierNotFound => "resource_not_found",
            Self::CarrierReadFailed => "internal_error",
            Self::TargetForbidden => "strm_target_forbidden",
            Self::UserLimitExceeded => "strm_user_stream_limit_exceeded",
            Self::CapacityExhausted => "strm_stream_capacity_exhausted",
            Self::SourceUnavailable => "remote_source_unavailable",
            Self::ResponseInvalid => "remote_response_invalid",
            Self::SourceTimeout => "remote_source_timeout",
            Self::RangeNotSupported => "remote_range_not_supported",
        }
    }
}

/// An error that deliberately stores no URL, hostname, request, or upstream
/// error value. This makes routine `Debug` and `Display` formatting safe.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RemoteStreamError {
    kind: RemoteStreamErrorKind,
    diagnostic_message: &'static str,
}

impl RemoteStreamError {
    pub(crate) const fn new(kind: RemoteStreamErrorKind, diagnostic_message: &'static str) -> Self {
        Self {
            kind,
            diagnostic_message,
        }
    }

    pub const fn kind(self) -> RemoteStreamErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }

    pub const fn diagnostic_message(self) -> &'static str {
        self.diagnostic_message
    }
}

impl fmt::Debug for RemoteStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteStreamError")
            .field("kind", &self.kind)
            .field("code", &self.code())
            .finish()
    }
}

impl fmt::Display for RemoteStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.diagnostic_message)
    }
}

impl std::error::Error for RemoteStreamError {}

pub(crate) const fn invalid_request(message: &'static str) -> RemoteStreamError {
    RemoteStreamError::new(RemoteStreamErrorKind::InvalidRequest, message)
}

pub(crate) const fn target_forbidden() -> RemoteStreamError {
    RemoteStreamError::new(
        RemoteStreamErrorKind::TargetForbidden,
        "the STRM target is forbidden by the remote streaming policy",
    )
}

pub(crate) const fn source_unavailable() -> RemoteStreamError {
    RemoteStreamError::new(
        RemoteStreamErrorKind::SourceUnavailable,
        "the remote media source is unavailable",
    )
}

pub(crate) const fn response_invalid() -> RemoteStreamError {
    RemoteStreamError::new(
        RemoteStreamErrorKind::ResponseInvalid,
        "the remote media response is not a valid direct media response",
    )
}

pub(crate) const fn source_timeout() -> RemoteStreamError {
    RemoteStreamError::new(
        RemoteStreamErrorKind::SourceTimeout,
        "the remote media source timed out",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_do_not_have_a_slot_for_remote_urls() {
        let error = response_invalid();
        let rendered = format!("{error:?} {error}");

        assert_eq!(error.code(), "remote_response_invalid");
        assert!(!rendered.contains("http://"));
        assert!(!rendered.contains("https://"));
        assert!(!rendered.contains('?'));
    }

    #[test]
    fn public_failure_kinds_keep_the_stable_error_codes() {
        let cases = [
            (
                RemoteStreamErrorKind::ReferenceTooLarge,
                "strm_reference_too_large",
            ),
            (
                RemoteStreamErrorKind::ReferenceInvalid,
                "strm_reference_invalid",
            ),
            (
                RemoteStreamErrorKind::TargetForbidden,
                "strm_target_forbidden",
            ),
            (
                RemoteStreamErrorKind::UserLimitExceeded,
                "strm_user_stream_limit_exceeded",
            ),
            (
                RemoteStreamErrorKind::CapacityExhausted,
                "strm_stream_capacity_exhausted",
            ),
            (
                RemoteStreamErrorKind::SourceUnavailable,
                "remote_source_unavailable",
            ),
            (
                RemoteStreamErrorKind::ResponseInvalid,
                "remote_response_invalid",
            ),
            (
                RemoteStreamErrorKind::SourceTimeout,
                "remote_source_timeout",
            ),
            (
                RemoteStreamErrorKind::RangeNotSupported,
                "remote_range_not_supported",
            ),
        ];

        for (kind, code) in cases {
            assert_eq!(kind.code(), code);
        }
    }
}
