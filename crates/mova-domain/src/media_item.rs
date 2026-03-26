use serde::Serialize;
use time::OffsetDateTime;

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
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
