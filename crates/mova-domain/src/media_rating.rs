use serde::Serialize;
use serde_json::Value;
use time::OffsetDateTime;

pub const RATING_SOURCE_TMDB: &str = "tmdb";
pub const RATING_KIND_AUDIENCE: &str = "audience";

/// A stable identifier assigned to a media item by an external provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaExternalId {
    pub provider: String,
    pub external_id: String,
}

/// A source-native aggregate rating snapshot.
///
/// Scores retain the provider's original scale. Clients must display the
/// source alongside the value instead of blending ratings into one score.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MediaRating {
    pub source: String,
    pub kind: String,
    pub score: f64,
    pub scale: f64,
    pub rating_count: Option<i64>,
    pub retrieved_via: String,
    pub attributes: Value,
    pub fetched_at: OffsetDateTime,
}
