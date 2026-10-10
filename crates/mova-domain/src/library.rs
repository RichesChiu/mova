use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const LIBRARY_STORAGE_AVAILABLE: &str = "available";
pub const LIBRARY_STORAGE_UNAVAILABLE: &str = "unavailable";

/// 面向上层暴露的媒体库领域对象。
/// 这里的 root_path 表示这个库后续扫描时要读取的根目录。
#[derive(Debug, Clone, Serialize)]
pub struct Library {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub metadata_language: String,
    pub root_path: String,
    /// `available`, or `unavailable` while the network storage holding the
    /// library root is not connected or cannot be read.
    pub storage_status: String,
    pub storage_issue: Option<LibraryStorageIssue>,
    pub storage_unavailable_since: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl Library {
    pub fn is_storage_available(&self) -> bool {
        self.storage_status == LIBRARY_STORAGE_AVAILABLE
    }
}

/// Why a library's storage is unavailable. `reason_code` and the mount facts
/// are the stable contract; `diagnostic_message` is operator text only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryStorageIssue {
    pub reason_code: String,
    pub mount_point: String,
    pub expected_fs_type: Option<String>,
    pub expected_source: Option<String>,
    pub actual_fs_type: Option<String>,
    pub actual_source: Option<String>,
    pub diagnostic_message: Option<String>,
}
