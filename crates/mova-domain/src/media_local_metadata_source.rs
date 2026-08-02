use serde::Serialize;
use serde_json::Value;
use time::OffsetDateTime;

/// A normalized snapshot of a local sidecar metadata document.
///
/// `payload` contains parsed, provider-neutral values. Raw XML is deliberately
/// not persisted as an application or API contract.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MediaLocalMetadataSource {
    pub id: i64,
    pub library_id: i64,
    pub media_item_id: i64,
    pub source_path: String,
    pub document_type: String,
    pub schema_version: i32,
    pub is_locked: bool,
    pub is_selected: bool,
    pub payload: Value,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Lightweight index entry for a persisted local metadata document.
///
/// The normalized payload is intentionally absent so callers can list source
/// records without loading potentially large JSON snapshots from Postgres.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaLocalMetadataSourceSummary {
    pub id: i64,
    pub library_id: i64,
    pub media_item_id: i64,
    pub source_path: String,
    pub document_type: String,
    pub schema_version: i32,
    pub is_locked: bool,
    pub is_selected: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
