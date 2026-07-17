use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use time::OffsetDateTime;

pub const MAX_SCAN_NOTIFICATION_ISSUES: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    pub id: i64,
    pub category: String,
    pub notification_type: String,
    pub severity: String,
    pub library_id: Option<i64>,
    pub payload: Value,
    pub is_read: bool,
    pub read_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationFeed {
    pub items: Vec<Notification>,
    pub total_unread: i64,
    pub unread_by_category: BTreeMap<String, i64>,
}

/// 扫描任务终态通知中保留的单个问题摘要。
#[derive(Debug, Clone, Serialize)]
pub struct ScanNotificationIssue {
    pub item_key: String,
    pub media_type: String,
    pub title: String,
    pub year: Option<i32>,
    pub file_count: i32,
    pub metadata_status: String,
    pub metadata_failure_reason: Option<String>,
    pub failure_detail: Option<String>,
    pub probe_warning_count: i32,
    pub probe_warning_file_path: Option<String>,
    pub probe_warning_detail: Option<String>,
}

/// 扫描 worker 在内存中累计、并随任务终态一次性写入通知 payload 的摘要。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanNotificationSummary {
    pub matched_files: i32,
    pub unmatched_files: i32,
    pub failed_files: i32,
    pub skipped_files: i32,
    pub probe_warning_count: i32,
    pub issue_count: i32,
    pub issues: Vec<ScanNotificationIssue>,
}
