use serde::Serialize;

/// A persisted external identity and the channel that supplied it.
///
/// Multiple channels may retain the same provider identity. This lets a local
/// NFO identity remain authoritative without preventing a remote provider from
/// keeping its independently refreshed snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaExternalIdRecord {
    pub media_item_id: i64,
    pub provider: String,
    pub external_id: String,
    pub retrieved_via: String,
}
