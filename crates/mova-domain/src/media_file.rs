use serde::Serialize;
use time::OffsetDateTime;

use crate::MediaSourceKind;

/// 归属于某个媒体条目的具体物理文件。
/// 一个媒体条目可以同时拥有多个本地文件或 STRM 载体作为播放版本。
#[derive(Debug, Clone, Serialize)]
pub struct MediaFile {
    pub id: i64,
    pub library_id: i64,
    pub media_item_id: i64,
    pub file_path: String,
    pub source_kind: MediaSourceKind,
    pub stream_reference_hash: Option<String>,
    pub container: Option<String>,
    pub file_size: i64,
    pub duration_seconds: Option<i32>,
    pub video_title: Option<String>,
    pub video_codec: Option<String>,
    pub video_profile: Option<String>,
    pub video_level: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bitrate: Option<i64>,
    pub video_bitrate: Option<i64>,
    pub video_frame_rate: Option<f64>,
    pub video_aspect_ratio: Option<String>,
    pub video_scan_type: Option<String>,
    pub video_color_primaries: Option<String>,
    pub video_color_space: Option<String>,
    pub video_color_transfer: Option<String>,
    pub video_bit_depth: Option<i32>,
    pub video_pixel_format: Option<String>,
    pub video_reference_frames: Option<i32>,
    pub technical_tags: Vec<String>,
    pub scan_hash: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
