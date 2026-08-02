use serde::Serialize;

/// A durable person credit imported from local metadata or another provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaItemCredit {
    pub id: i64,
    pub media_item_id: i64,
    pub credit_type: String,
    pub retrieved_via: String,
    pub sort_order: i32,
    pub person_id: Option<String>,
    pub name: String,
    pub role: Option<String>,
    pub profile_path: Option<String>,
}
