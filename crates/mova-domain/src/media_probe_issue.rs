use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Why ffprobe could not analyze a media file.
///
/// A file with an issue is still part of the catalog and is still scraped;
/// the issue tells the user that the file itself could not be analyzed and
/// may not play. Missing individual fields (resolution, duration, codec) are
/// not an issue: they are stored as unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaProbeIssue {
    /// Opening or reading the file failed (I/O error, permission denied).
    Unreadable,
    /// ffprobe ran but rejected the file: corrupt, unsupported, incomplete.
    Unrecognized,
    /// The file parsed but carries no video stream (audio only, cover art only).
    NoVideoStream,
    /// ffprobe did not finish within the time or output limits.
    Timeout,
}

impl MediaProbeIssue {
    pub const ALL: [Self; 4] = [
        Self::Unreadable,
        Self::Unrecognized,
        Self::NoVideoStream,
        Self::Timeout,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::Unrecognized => "unrecognized",
            Self::NoVideoStream => "no_video_stream",
            Self::Timeout => "timeout",
        }
    }

    /// Stable reason code used by scan notifications and client copy.
    pub fn reason_code(self) -> &'static str {
        match self {
            Self::Unreadable => "probe_unreadable",
            Self::Unrecognized => "probe_unrecognized",
            Self::NoVideoStream => "probe_no_video_stream",
            Self::Timeout => "probe_timeout",
        }
    }

    /// Transient issues are probed again on the next scan even when the file
    /// is unchanged; permanent ones only when the file changes.
    pub fn is_transient(self) -> bool {
        matches!(self, Self::Unreadable | Self::Timeout)
    }
}

impl fmt::Display for MediaProbeIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseMediaProbeIssueError(pub String);

impl fmt::Display for ParseMediaProbeIssueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown media probe issue: {}", self.0)
    }
}

impl std::error::Error for ParseMediaProbeIssueError {}

impl FromStr for MediaProbeIssue {
    type Err = ParseMediaProbeIssueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|issue| issue.as_str() == value)
            .ok_or_else(|| ParseMediaProbeIssueError(value.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::MediaProbeIssue;

    #[test]
    fn codes_round_trip_and_split_by_retry_policy() {
        for issue in MediaProbeIssue::ALL {
            assert_eq!(issue.as_str().parse::<MediaProbeIssue>(), Ok(issue));
            assert!(issue.reason_code().starts_with("probe_"));
        }
        assert!("broken".parse::<MediaProbeIssue>().is_err());
        assert!(MediaProbeIssue::Unreadable.is_transient());
        assert!(MediaProbeIssue::Timeout.is_transient());
        assert!(!MediaProbeIssue::Unrecognized.is_transient());
        assert!(!MediaProbeIssue::NoVideoStream.is_transient());
    }
}
