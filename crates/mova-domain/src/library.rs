use serde::Serialize;
use time::OffsetDateTime;

/// 面向上层暴露的媒体库领域对象。
/// 这里的 root_path 表示这个库后续扫描时要读取的根目录。
#[derive(Debug, Clone, Serialize)]
pub struct Library {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub library_type: String,
    pub metadata_language: String,
    pub root_path: String,
    pub is_enabled: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
