use serde::Serialize;
use time::OffsetDateTime;

/// 某个用户在某个媒体文件上的观看进度。
/// 当前阶段服务默认只维护一个本地用户，后续接入鉴权后可以自然扩展到多用户。
#[derive(Debug, Clone, Serialize)]
pub struct PlaybackProgress {
    pub id: i64,
    pub media_item_id: i64,
    pub media_file_id: i64,
    pub position_seconds: i32,
    pub duration_seconds: Option<i32>,
    pub last_watched_at: OffsetDateTime,
    pub is_finished: bool,
}
