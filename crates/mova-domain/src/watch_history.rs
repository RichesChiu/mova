use serde::Serialize;
use time::OffsetDateTime;

/// 某个用户的一次观看会话记录。
#[derive(Debug, Clone, Serialize)]
pub struct WatchHistory {
    pub id: i64,
    pub media_item_id: i64,
    pub media_file_id: i64,
    pub position_seconds: i32,
    pub duration_seconds: Option<i32>,
    pub started_at: OffsetDateTime,
    pub last_watched_at: OffsetDateTime,
    pub ended_at: Option<OffsetDateTime>,
    pub completed_at: Option<OffsetDateTime>,
}
