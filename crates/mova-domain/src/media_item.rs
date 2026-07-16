use serde::Serialize;
use time::OffsetDateTime;

pub const METADATA_STATUS_MATCHED: &str = "matched";
pub const METADATA_STATUS_PENDING: &str = "pending";
pub const METADATA_STATUS_UNMATCHED: &str = "unmatched";
pub const METADATA_STATUS_FAILED: &str = "failed";
pub const METADATA_STATUS_SKIPPED: &str = "skipped";

pub const METADATA_FAILURE_NO_REMOTE_MATCH: &str = "no_remote_match";
pub const METADATA_FAILURE_PROVIDER_DISABLED: &str = "metadata_provider_disabled";
pub const METADATA_FAILURE_PROVIDER_ERROR: &str = "metadata_provider_error";

pub const REMOTE_MEDIA_TYPE_MOVIE: &str = "movie";
pub const REMOTE_MEDIA_TYPE_SERIES: &str = "series";

/// 扫描目录后沉淀到系统里的媒体条目。
/// 当前最小实现里，一个视频文件会先对应一个 media item。
#[derive(Debug, Clone, Serialize)]
pub struct MediaItem {
    pub id: i64,
    pub library_id: i64,
    pub media_type: String,
    pub title: String,
    pub source_title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub metadata_provider: Option<String>,
    pub metadata_provider_item_id: Option<i64>,
    pub metadata_status: String,
    pub metadata_failure_reason: Option<String>,
    pub remote_media_type: Option<String>,
    pub year: Option<i32>,
    pub imdb_rating: Option<String>,
    pub country: Option<String>,
    pub genres: Option<String>,
    pub studio: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
