use anyhow::{Context, Result};
use mova_domain::{ContinueWatchingItem, MediaItem, PlaybackProgress};
use sqlx::{
    postgres::{PgPool, PgRow},
    Row,
};
use std::collections::HashMap;

/// 写入或更新播放进度时需要的参数。
#[derive(Debug, Clone)]
pub struct UpsertPlaybackProgressParams {
    pub user_id: i64,
    pub media_item_id: i64,
    pub media_file_id: i64,
    pub position_seconds: i32,
    pub duration_seconds: Option<i32>,
    pub is_finished: bool,
}

/// 读取“继续观看”列表，电影按媒体条目聚合，剧集按 series 聚合，
/// 每个聚合键只保留最近一次未看完的观看进度。
pub async fn list_continue_watching(
    pool: &PgPool,
    user_id: i64,
    limit: i64,
) -> Result<Vec<ContinueWatchingItem>> {
    let rows = sqlx::query(
        r#"
        with ranked_progress as (
            select
                pp.id as progress_id,
                pp.media_item_id as progress_media_item_id,
                pp.media_file_id,
                pp.position_seconds,
                pp.duration_seconds,
                pp.last_watched_at,
                pp.is_finished,
                coalesce(e.series_id, pp.media_item_id) as presentation_media_item_id,
                s.season_number,
                e.episode_number,
                case
                    when mi.media_type = 'episode' then coalesce(nullif(e.title, ''), nullif(mi.title, ''))
                    else null
                end as episode_title,
                case
                    when mi.media_type = 'episode' then mi.overview
                    else null
                end as episode_overview,
                case
                    when mi.media_type = 'episode' then mi.poster_path
                    else null
                end as episode_poster_path,
                case
                    when mi.media_type = 'episode' then mi.backdrop_path
                    else null
                end as episode_backdrop_path
            from playback_progress pp
            join media_items mi on mi.id = pp.media_item_id
            left join episodes e on e.media_item_id = pp.media_item_id
            left join seasons s on s.id = e.season_id
            where pp.user_id = $1
              and pp.is_finished = false
        ),
        -- 剧集在首页只保留一个入口，所以按 series 或电影自身做 presentation 聚合，
        -- 然后从每组里取最近一次未看完的观看进度。
        latest_progress as (
            select distinct on (presentation_media_item_id)
                progress_id,
                progress_media_item_id,
                media_file_id,
                position_seconds,
                duration_seconds,
                last_watched_at,
                is_finished,
                presentation_media_item_id,
                season_number,
                episode_number,
                episode_title,
                episode_overview,
                episode_poster_path,
                episode_backdrop_path
            from ranked_progress
            order by presentation_media_item_id, last_watched_at desc, progress_id desc
        )
        select
            mi.id,
            mi.library_id,
            mi.media_type,
            mi.title,
            mi.original_title,
            mi.sort_title,
            mi.year,
            mi.overview,
            mi.poster_path,
            mi.backdrop_path,
            mi.created_at,
            mi.updated_at,
            lp.progress_id,
            lp.progress_media_item_id,
            lp.media_file_id,
            lp.position_seconds,
            lp.duration_seconds,
            lp.last_watched_at,
            lp.is_finished,
            lp.season_number,
            lp.episode_number,
            lp.episode_title,
            lp.episode_overview,
            lp.episode_poster_path,
            lp.episode_backdrop_path
        from latest_progress lp
        join media_items mi on mi.id = lp.presentation_media_item_id
        order by lp.last_watched_at desc, lp.progress_id desc
        limit $2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("failed to list continue watching items")?;

    Ok(rows.into_iter().map(map_continue_watching_row).collect())
}

/// 读取某个媒体条目最近一次观看进度。
pub async fn get_playback_progress_for_media_item(
    pool: &PgPool,
    user_id: i64,
    media_item_id: i64,
) -> Result<Option<PlaybackProgress>> {
    let row = sqlx::query(
        r#"
        select
            id,
            media_item_id,
            media_file_id,
            position_seconds,
            duration_seconds,
            last_watched_at,
            is_finished
        from playback_progress
        where user_id = $1
          and media_item_id = $2
        order by last_watched_at desc, id desc
        limit 1
        "#,
    )
    .bind(user_id)
    .bind(media_item_id)
    .fetch_optional(pool)
    .await
    .context("failed to get playback progress for media item")?;

    Ok(row.map(map_playback_progress_row))
}

/// 批量读取一组媒体条目的最近播放进度。
pub async fn list_playback_progress_for_media_items(
    pool: &PgPool,
    user_id: i64,
    media_item_ids: &[i64],
) -> Result<HashMap<i64, PlaybackProgress>> {
    if media_item_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        select distinct on (media_item_id)
            id,
            media_item_id,
            media_file_id,
            position_seconds,
            duration_seconds,
            last_watched_at,
            is_finished
        from playback_progress
        where user_id = $1
          and media_item_id = any($2)
        order by media_item_id, last_watched_at desc, id desc
        "#,
    )
    .bind(user_id)
    .bind(media_item_ids)
    .fetch_all(pool)
    .await
    .context("failed to list playback progress for media items")?;

    Ok(rows
        .into_iter()
        .map(map_playback_progress_row)
        .map(|progress| (progress.media_item_id, progress))
        .collect())
}

/// 对指定文件写入播放进度；如果此前已有记录则直接更新。
pub async fn upsert_playback_progress(
    pool: &PgPool,
    params: UpsertPlaybackProgressParams,
) -> Result<PlaybackProgress> {
    let row = sqlx::query(
        r#"
        insert into playback_progress (
            user_id,
            media_item_id,
            media_file_id,
            position_seconds,
            duration_seconds,
            last_watched_at,
            is_finished
        )
        values ($1, $2, $3, $4, $5, now(), $6)
        on conflict (user_id, media_file_id) do update
        set media_item_id = excluded.media_item_id,
            position_seconds = excluded.position_seconds,
            duration_seconds = excluded.duration_seconds,
            last_watched_at = now(),
            is_finished = excluded.is_finished
        returning
            id,
            media_item_id,
            media_file_id,
            position_seconds,
            duration_seconds,
            last_watched_at,
            is_finished
        "#,
    )
    .bind(params.user_id)
    .bind(params.media_item_id)
    .bind(params.media_file_id)
    .bind(params.position_seconds)
    .bind(params.duration_seconds)
    .bind(params.is_finished)
    .fetch_one(pool)
    .await
    .context("failed to upsert playback progress")?;

    Ok(map_playback_progress_row(row))
}

fn map_playback_progress_row(row: PgRow) -> PlaybackProgress {
    PlaybackProgress {
        id: row.get("id"),
        media_item_id: row.get("media_item_id"),
        media_file_id: row.get("media_file_id"),
        position_seconds: row.get("position_seconds"),
        duration_seconds: row.get("duration_seconds"),
        last_watched_at: row.get("last_watched_at"),
        is_finished: row.get("is_finished"),
    }
}

fn map_continue_watching_row(row: PgRow) -> ContinueWatchingItem {
    ContinueWatchingItem {
        media_item: MediaItem {
            id: row.get("id"),
            library_id: row.get("library_id"),
            media_type: row.get("media_type"),
            title: row.get("title"),
            original_title: row.get("original_title"),
            sort_title: row.get("sort_title"),
            metadata_provider: None,
            metadata_provider_item_id: None,
            year: row.get("year"),
            overview: row.get("overview"),
            poster_path: row.get("poster_path"),
            backdrop_path: row.get("backdrop_path"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        },
        playback_progress: PlaybackProgress {
            id: row.get("progress_id"),
            media_item_id: row.get("progress_media_item_id"),
            media_file_id: row.get("media_file_id"),
            position_seconds: row.get("position_seconds"),
            duration_seconds: row.get("duration_seconds"),
            last_watched_at: row.get("last_watched_at"),
            is_finished: row.get("is_finished"),
        },
        // 这些 episode_* 字段只在剧集聚合场景下有值；电影会保持为空。
        season_number: row.get("season_number"),
        episode_number: row.get("episode_number"),
        episode_title: row.get("episode_title"),
        episode_overview: row.get("episode_overview"),
        episode_poster_path: row.get("episode_poster_path"),
        episode_backdrop_path: row.get("episode_backdrop_path"),
    }
}
