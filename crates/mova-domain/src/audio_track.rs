use serde::Serialize;
use time::OffsetDateTime;

/// 归属于某个媒体文件的可切换音轨。
/// 当前只表达内嵌音轨，不额外支持外挂音频文件。
#[derive(Debug, Clone, Serialize)]
pub struct AudioTrack {
    pub id: i64,
    pub media_file_id: i64,
    pub stream_index: i32,
    pub language: Option<String>,
    pub audio_codec: Option<String>,
    pub label: Option<String>,
    pub channel_layout: Option<String>,
    pub channels: Option<i32>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i32>,
    pub is_default: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
