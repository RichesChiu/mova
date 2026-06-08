use serde::Serialize;
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize)]
pub struct Season {
    pub id: i64,
    pub series_id: i64,
    pub season_number: i32,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub intro_start_seconds: Option<i32>,
    pub intro_end_seconds: Option<i32>,
    pub episode_count: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
