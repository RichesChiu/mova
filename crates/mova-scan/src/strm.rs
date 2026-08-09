use sha2::{Digest, Sha256};
use std::{
    fmt,
    fs::File,
    io::{self, Read},
    path::Path,
};
use url::Url;

pub const MAX_STRM_FILE_BYTES: usize = 8 * 1024;
pub const MAX_STRM_URL_BYTES: usize = 4096;

/// A validated HTTP(S) reference held only for the lifetime of the caller.
///
/// The custom `Debug` implementation intentionally omits the URL so signed
/// query strings and credentials can never leak through routine diagnostics.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpStrmReference {
    pub url: Url,
    pub reference_hash: String,
}

impl fmt::Debug for HttpStrmReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpStrmReference")
            .field("scheme", &self.url.scheme())
            .field("host_present", &self.url.host().is_some())
            .field("port", &self.url.port_or_known_default())
            .field(
                "reference_hash_prefix",
                &self
                    .reference_hash
                    .get(..8)
                    .unwrap_or(self.reference_hash.as_str()),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrmReferenceError {
    Empty,
    TooLarge,
    InvalidUtf8,
    MultipleLines,
    InvalidUrl,
    UnsupportedScheme,
    CredentialsNotAllowed,
}

impl StrmReferenceError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Empty => "strm_reference_empty",
            Self::TooLarge => "strm_reference_too_large",
            Self::InvalidUtf8 => "strm_reference_invalid_utf8",
            Self::MultipleLines => "strm_reference_multiple_lines",
            Self::InvalidUrl => "strm_reference_invalid_url",
            Self::UnsupportedScheme => "strm_reference_unsupported_scheme",
            Self::CredentialsNotAllowed => "strm_reference_credentials_not_allowed",
        }
    }
}

impl fmt::Display for StrmReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for StrmReferenceError {}

#[derive(Debug)]
pub enum ReadHttpStrmReferenceError {
    Io(io::Error),
    Invalid(StrmReferenceError),
}

impl fmt::Display for ReadHttpStrmReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "failed to read STRM carrier: {error}"),
            Self::Invalid(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ReadHttpStrmReferenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Invalid(error) => Some(error),
        }
    }
}

impl From<io::Error> for ReadHttpStrmReferenceError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<StrmReferenceError> for ReadHttpStrmReferenceError {
    fn from(error: StrmReferenceError) -> Self {
        Self::Invalid(error)
    }
}

/// Reads a STRM carrier with a hard upper bound and validates its reference.
pub fn read_http_strm_reference(
    path: &Path,
) -> Result<HttpStrmReference, ReadHttpStrmReferenceError> {
    let file = File::open(path)?;
    let mut bytes = Vec::with_capacity(MAX_STRM_FILE_BYTES.min(1024) + 1);
    file.take((MAX_STRM_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    parse_http_strm_reference(&bytes).map_err(Into::into)
}

/// Parses one HTTP(S) reference without ever including input text in errors.
pub fn parse_http_strm_reference(bytes: &[u8]) -> Result<HttpStrmReference, StrmReferenceError> {
    if bytes.len() > MAX_STRM_FILE_BYTES {
        return Err(StrmReferenceError::TooLarge);
    }

    let text = std::str::from_utf8(bytes).map_err(|_| StrmReferenceError::InvalidUtf8)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).trim();
    if text.is_empty() {
        return Err(StrmReferenceError::Empty);
    }

    let mut non_empty_lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let reference_text = non_empty_lines.next().ok_or(StrmReferenceError::Empty)?;
    if non_empty_lines.next().is_some() {
        return Err(StrmReferenceError::MultipleLines);
    }
    if reference_text.len() > MAX_STRM_URL_BYTES {
        return Err(StrmReferenceError::InvalidUrl);
    }

    let url = Url::parse(reference_text).map_err(|_| StrmReferenceError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(StrmReferenceError::UnsupportedScheme);
    }
    let authority = reference_text
        .split_once("://")
        .map(|(_, remainder)| remainder.split(['/', '?', '#']).next().unwrap_or_default())
        .ok_or(StrmReferenceError::InvalidUrl)?;
    if authority.is_empty() || url.host().is_none() {
        return Err(StrmReferenceError::InvalidUrl);
    }
    if authority.contains('@') || !url.username().is_empty() || url.password().is_some() {
        return Err(StrmReferenceError::CredentialsNotAllowed);
    }
    if url.fragment().is_some() {
        return Err(StrmReferenceError::InvalidUrl);
    }

    let reference_hash = format!("{:x}", Sha256::digest(reference_text.as_bytes()));
    Ok(HttpStrmReference {
        url,
        reference_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_https_queries_bom_crlf_and_outer_whitespace() {
        let http = parse_http_strm_reference(b"http://media.example/video.mp4").unwrap();
        assert_eq!(http.url.scheme(), "http");

        let signed = parse_http_strm_reference(
            "\u{feff}  https://media.example/video.mp4?token=secret&part=2\r\n".as_bytes(),
        )
        .unwrap();
        assert_eq!(signed.url.scheme(), "https");
        assert_eq!(signed.url.query(), Some("token=secret&part=2"));

        let expected = format!(
            "{:x}",
            Sha256::digest(b"https://media.example/video.mp4?token=secret&part=2")
        );
        assert_eq!(signed.reference_hash, expected);
    }

    #[test]
    fn rejects_empty_oversized_non_utf8_and_multiple_references() {
        assert_eq!(
            parse_http_strm_reference(b" \r\n\t"),
            Err(StrmReferenceError::Empty)
        );
        assert_eq!(
            parse_http_strm_reference(&vec![b'a'; MAX_STRM_FILE_BYTES + 1]),
            Err(StrmReferenceError::TooLarge)
        );
        assert_eq!(
            parse_http_strm_reference(&[0xff, 0xfe]),
            Err(StrmReferenceError::InvalidUtf8)
        );
        assert_eq!(
            parse_http_strm_reference(b"https://one.example/a\nhttps://two.example/b"),
            Err(StrmReferenceError::MultipleLines)
        );
    }

    #[test]
    fn rejects_missing_hosts_unsupported_schemes_credentials_and_fragments() {
        for value in [b"http:///video.mp4".as_slice(), b"https:/video.mp4"] {
            assert_eq!(
                parse_http_strm_reference(value),
                Err(StrmReferenceError::InvalidUrl)
            );
        }
        for value in [
            b"ftp://media.example/video".as_slice(),
            b"rtsp://media.example/video",
            b"mms://media.example/video",
            b"file:///video.mp4",
        ] {
            assert_eq!(
                parse_http_strm_reference(value),
                Err(StrmReferenceError::UnsupportedScheme)
            );
        }
        assert_eq!(
            parse_http_strm_reference(b"https://user:password@media.example/video"),
            Err(StrmReferenceError::CredentialsNotAllowed)
        );
        assert_eq!(
            parse_http_strm_reference(b"https://@media.example/video"),
            Err(StrmReferenceError::CredentialsNotAllowed)
        );
        assert_eq!(
            parse_http_strm_reference(b"https://media.example/video#fragment"),
            Err(StrmReferenceError::InvalidUrl)
        );
    }

    #[test]
    fn same_length_reference_change_updates_hash() {
        let first = parse_http_strm_reference(b"https://media.example/a.mp4").unwrap();
        let second = parse_http_strm_reference(b"https://media.example/b.mp4").unwrap();
        assert_ne!(first.reference_hash, second.reference_hash);
    }

    #[test]
    fn diagnostics_never_include_the_reference_or_query() {
        let reference = parse_http_strm_reference(
            b"https://media.example/video.mp4?private-signature=do-not-log",
        )
        .unwrap();
        let debug = format!("{reference:?}");
        assert!(!debug.contains("media.example"));
        assert!(!debug.contains("private-signature"));
        assert!(!debug.contains("do-not-log"));

        let error = parse_http_strm_reference(
            b"https://media.example/video.mp4?private-signature=do-not-log#invalid",
        )
        .unwrap_err();
        let diagnostics = format!("{error:?} {error}");
        assert!(!diagnostics.contains("media.example"));
        assert!(!diagnostics.contains("private-signature"));
        assert!(!diagnostics.contains("do-not-log"));
    }
}
