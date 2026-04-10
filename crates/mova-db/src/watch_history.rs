use anyhow::{Context, Result};
use mova_domain::{MediaItem, WatchHistory, WatchHistoryItem};
use sqlx::{
    postgres::{PgPool, PgRow},
    Row,
};
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct CreateWatchHistoryParams {
    pub user_id: i64,
    pub media_item_id: i64,
    pub media_file_id: i64,
    pub position_seconds: i32,
    pub duration_seconds: Option<i32>,
    pub started_at: OffsetDateTime,
    pub last_watched_at: OffsetDateTime,
    pub ended_at: Option<OffsetDateTime>,
    pub completed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct UpdateWatchHistoryParams {
    pub id: i64,
    pub position_seconds: i32,
    pub duration_seconds: Option<i32>,
    pub last_watched_at: OffsetDateTime,
    pub ended_at: Option<OffsetDateTime>,
    pub completed_at: Option<OffsetDateTime>,
}

pub async fn list_watch_history(
    pool: &PgPool,
    user_id: i64,
    limit: i64,
) -> Result<Vec<WatchHistoryItem>> {
    let rows = sqlx::query(
        r#"
        select
            mi.id,
            mi.library_id,
            mi.media_type,
            mi.title,
            mi.source_title,
            mi.original_title,
            mi.sort_title,
            mi.year,
            mi.imdb_rating,
            mi.overview,
            mi.poster_path,
            mi.backdrop_path,
            mi.created_at,
            mi.updated_at,
            wh.id as watch_history_id,
            wh.media_item_id as watch_history_media_item_id,
            wh.media_file_id,
            wh.position_seconds,
            wh.duration_seconds,
            wh.started_at,
            wh.last_watched_at,
            wh.ended_at,
            wh.completed_at
        from watch_history wh
        join media_items mi on mi.id = wh.media_item_id
        where wh.user_id = $1
        order by wh.last_watched_at desc, wh.id desc
        limit $2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("failed to list watch history")?;

    Ok(rows.into_iter().map(map_watch_history_item_row).collect())
}

pub async fn get_latest_open_watch_history(
    pool: &PgPool,
    user_id: i64,
    media_file_id: i64,
) -> Result<Option<WatchHistory>> {
    let row = sqlx::query(
        r#"
        select
            id,
            media_item_id,
            media_file_id,
            position_seconds,
            duration_seconds,
            started_at,
            last_watched_at,
            ended_at,
            completed_at
        from watch_history
        where user_id = $1
          and media_file_id = $2
          and ended_at is null
        order by last_watched_at desc, id desc
        limit 1
        "#,
    )
    .bind(user_id)
    .bind(media_file_id)
    .fetch_optional(pool)
    .await
    .context("failed to get latest open watch history")?;

    Ok(row.map(map_watch_history_row))
}

pub async fn create_watch_history(
    pool: &PgPool,
    params: CreateWatchHistoryParams,
) -> Result<WatchHistory> {
    let row = sqlx::query(
        r#"
        insert into watch_history (
            user_id,
            media_item_id,
            media_file_id,
            position_seconds,
            duration_seconds,
            started_at,
            last_watched_at,
            ended_at,
            completed_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        returning
            id,
            media_item_id,
            media_file_id,
            position_seconds,
            duration_seconds,
            started_at,
            last_watched_at,
            ended_at,
            completed_at
        "#,
    )
    .bind(params.user_id)
    .bind(params.media_item_id)
    .bind(params.media_file_id)
    .bind(params.position_seconds)
    .bind(params.duration_seconds)
    .bind(params.started_at)
    .bind(params.last_watched_at)
    .bind(params.ended_at)
    .bind(params.completed_at)
    .fetch_one(pool)
    .await
    .context("failed to create watch history")?;

    Ok(map_watch_history_row(row))
}

pub async fn update_watch_history(
    pool: &PgPool,
    params: UpdateWatchHistoryParams,
) -> Result<WatchHistory> {
    let row = sqlx::query(
        r#"
        update watch_history
        set position_seconds = $2,
            duration_seconds = $3,
            last_watched_at = $4,
            ended_at = $5,
            completed_at = $6,
            updated_at = now()
        where id = $1
        returning
            id,
            media_item_id,
            media_file_id,
            position_seconds,
            duration_seconds,
            started_at,
            last_watched_at,
            ended_at,
            completed_at
        "#,
    )
    .bind(params.id)
    .bind(params.position_seconds)
    .bind(params.duration_seconds)
    .bind(params.last_watched_at)
    .bind(params.ended_at)
    .bind(params.completed_at)
    .fetch_one(pool)
    .await
    .context("failed to update watch history")?;

    Ok(map_watch_history_row(row))
}

fn map_watch_history_row(row: PgRow) -> WatchHistory {
    WatchHistory {
        id: row.get("id"),
        media_item_id: row.get("media_item_id"),
        media_file_id: row.get("media_file_id"),
        position_seconds: row.get("position_seconds"),
        duration_seconds: row.get("duration_seconds"),
        started_at: row.get("started_at"),
        last_watched_at: row.get("last_watched_at"),
        ended_at: row.get("ended_at"),
        completed_at: row.get("completed_at"),
    }
}

fn map_watch_history_item_row(row: PgRow) -> WatchHistoryItem {
    WatchHistoryItem {
        media_item: MediaItem {
            id: row.get("id"),
            library_id: row.get("library_id"),
            media_type: row.get("media_type"),
            title: row.get("title"),
            source_title: row.get("source_title"),
            original_title: row.get("original_title"),
            sort_title: row.get("sort_title"),
            metadata_provider: None,
            metadata_provider_item_id: None,
            year: row.get("year"),
            imdb_rating: row.get("imdb_rating"),
            overview: row.get("overview"),
            poster_path: row.get("poster_path"),
            backdrop_path: row.get("backdrop_path"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        },
        watch_history: WatchHistory {
            id: row.get("watch_history_id"),
            media_item_id: row.get("watch_history_media_item_id"),
            media_file_id: row.get("media_file_id"),
            position_seconds: row.get("position_seconds"),
            duration_seconds: row.get("duration_seconds"),
            started_at: row.get("started_at"),
            last_watched_at: row.get("last_watched_at"),
            ended_at: row.get("ended_at"),
            completed_at: row.get("completed_at"),
        },
    }
}
