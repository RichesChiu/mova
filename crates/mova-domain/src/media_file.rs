use serde::Serialize;
use time::OffsetDateTime;

/// 归属于某个媒体条目的具体物理文件。
/// 当前最小实现里，一个 media item 只会有一个 media file，但后续可以扩展成多版本文件。
#[derive(Debug, Clone, Serialize)]
pub struct MediaFile {
    pub id: i64,
    pub media_item_id: i64,
    pub file_path: String,
    pub container: Option<String>,
    pub file_size: i64,
    pub duration_seconds: Option<i32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bitrate: Option<i64>,
    pub scan_hash: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
