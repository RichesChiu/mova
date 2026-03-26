use serde::Serialize;
use time::OffsetDateTime;

/// 归属于某个媒体文件的可切换字幕轨道。
/// 既支持同目录外挂字幕，也支持通过 ffprobe 发现的媒体内嵌字幕流。
#[derive(Debug, Clone, Serialize)]
pub struct SubtitleFile {
    pub id: i64,
    pub media_file_id: i64,
    pub source_kind: String,
    pub file_path: Option<String>,
    pub stream_index: Option<i32>,
    pub language: Option<String>,
    pub subtitle_format: String,
    pub label: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
