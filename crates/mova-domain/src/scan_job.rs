use serde::Serialize;
use time::OffsetDateTime;

/// 一次媒体库扫描任务的执行记录。
#[derive(Debug, Clone, Serialize)]
pub struct ScanJob {
    pub id: i64,
    pub library_id: i64,
    pub status: String,
    pub total_files: i32,
    pub scanned_files: i32,
    pub created_at: OffsetDateTime,
    pub started_at: Option<OffsetDateTime>,
    pub finished_at: Option<OffsetDateTime>,
    pub error_message: Option<String>,
}
