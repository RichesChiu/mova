use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MediaCastMember {
    pub media_item_id: i64,
    pub person_id: Option<i64>,
    pub sort_order: i32,
    pub name: String,
    pub character_name: Option<String>,
    pub profile_path: Option<String>,
}
