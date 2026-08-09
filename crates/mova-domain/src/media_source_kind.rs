use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Describes how a media file obtains its playable bytes.
///
/// This is deliberately transport-agnostic: URL parsing and carrier I/O live
/// in `mova-scan`, while the domain only records the stable source category.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaSourceKind {
    #[default]
    LocalFile,
    Strm,
}

impl MediaSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalFile => "local_file",
            Self::Strm => "strm",
        }
    }
}

impl fmt::Display for MediaSourceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseMediaSourceKindError;

impl fmt::Display for ParseMediaSourceKindError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unsupported media source kind")
    }
}

impl std::error::Error for ParseMediaSourceKindError {}

impl FromStr for MediaSourceKind {
    type Err = ParseMediaSourceKindError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "local_file" => Ok(Self::LocalFile),
            "strm" => Ok(Self::Strm),
            _ => Err(ParseMediaSourceKindError),
        }
    }
}

impl TryFrom<&str> for MediaSourceKind {
    type Error = ParseMediaSourceKindError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::MediaSourceKind;

    #[test]
    fn source_kind_uses_stable_snake_case_values() {
        assert_eq!(MediaSourceKind::LocalFile.as_str(), "local_file");
        assert_eq!(MediaSourceKind::Strm.as_str(), "strm");
        assert_eq!("strm".parse(), Ok(MediaSourceKind::Strm));
        assert!("remote_url".parse::<MediaSourceKind>().is_err());
        assert_eq!(
            serde_json::to_string(&MediaSourceKind::LocalFile).unwrap(),
            "\"local_file\""
        );
    }
}
