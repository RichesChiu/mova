use crate::{
    error::{ApplicationError, ApplicationResult},
    intro_detection::ensure_intro_markers_for_playback,
};
use sqlx::postgres::PgPool;

#[derive(Debug, Clone)]
pub struct MediaItemPlaybackHeader {
    pub media_item_id: i64,
    pub library_id: i64,
    pub media_type: String,
    pub series_media_item_id: Option<i64>,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub season_id: Option<i64>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub episode_title: Option<String>,
    pub season_intro_start_seconds: Option<i32>,
    pub season_intro_end_seconds: Option<i32>,
    pub episode_intro_start_seconds: Option<i32>,
    pub episode_intro_end_seconds: Option<i32>,
}

pub async fn get_media_item_playback_header(
    pool: &PgPool,
    media_item_id: i64,
) -> ApplicationResult<MediaItemPlaybackHeader> {
    let header = mova_db::get_media_item_playback_header(pool, media_item_id)
        .await
        .map_err(ApplicationError::from)?;

    let header = header.ok_or_else(|| {
        ApplicationError::NotFound(format!("media item not found: {}", media_item_id))
    })?;

    if let Err(error) = ensure_intro_markers_for_playback(pool, &header).await {
        tracing::warn!(
            media_item_id,
            error = ?error,
            "on-demand intro detection failed; continuing without intro markers"
        );
    }

    Ok(MediaItemPlaybackHeader {
        media_item_id: header.media_item_id,
        library_id: header.library_id,
        media_type: header.media_type,
        series_media_item_id: header.series_media_item_id,
        title: header.title,
        original_title: header.original_title,
        year: header.year,
        season_id: header.season_id,
        season_number: header.season_number,
        episode_number: header.episode_number,
        episode_title: header.episode_title,
        season_intro_start_seconds: header.season_intro_start_seconds,
        season_intro_end_seconds: header.season_intro_end_seconds,
        episode_intro_start_seconds: header.episode_intro_start_seconds,
        episode_intro_end_seconds: header.episode_intro_end_seconds,
    })
}
