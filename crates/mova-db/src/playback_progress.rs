use anyhow::{Context, Result};
use mova_domain::{ContinueWatchingItem, MediaItem, PlaybackProgress};
use sqlx::{
    postgres::{PgPool, PgRow},
    Row,
};
use std::collections::HashMap;

use crate::media_items::list_media_item_ratings;

const CONTINUE_WATCHING_LIMIT: i64 = 20;

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

/// 读取有上限的“继续观看”活跃队列。电影按自身唯一，剧集按 series 唯一。
pub async fn list_continue_watching(
    pool: &PgPool,
    user_id: i64,
    limit: i64,
) -> Result<Vec<ContinueWatchingItem>> {
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
            mi.metadata_provider,
            mi.metadata_provider_item_id,
            mi.metadata_status,
            mi.metadata_failure_reason,
            mi.remote_media_type,
            mi.year,
            mi.country,
            mi.genres,
            mi.studio,
            mi.overview,
            mi.poster_path,
            mi.backdrop_path,
            mi.logo_path,
            mi.created_at,
            mi.updated_at,
            pp.id as progress_id,
            pp.media_item_id as progress_media_item_id,
            pp.media_file_id,
            pp.position_seconds,
            pp.duration_seconds,
            pp.last_watched_at,
            pp.is_finished,
            s.season_number,
            e.episode_number,
            case
                when watched_mi.media_type = 'episode' then coalesce(nullif(e.title, ''), nullif(watched_mi.title, ''))
                else null
            end as episode_title,
            case when watched_mi.media_type = 'episode' then watched_mi.overview else null end
                as episode_overview,
            case when watched_mi.media_type = 'episode' then watched_mi.poster_path else null end
                as episode_poster_path,
            case when watched_mi.media_type = 'episode' then watched_mi.backdrop_path else null end
                as episode_backdrop_path
        from continue_watching cw
        join media_items mi on mi.id = cw.media_item_id
        join media_items watched_mi on watched_mi.id = cw.last_played_media_item_id
        join playback_progress pp
          on pp.user_id = cw.user_id
         and pp.media_file_id = cw.media_file_id
        left join episodes e on e.media_item_id = cw.last_played_media_item_id
        left join seasons s on s.id = e.season_id
        where cw.user_id = $1
          and pp.is_finished = false
        order by cw.last_watched_at desc, cw.id desc
        limit $2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("failed to list continue watching items")?;

    let mut items = rows
        .into_iter()
        .map(map_continue_watching_row)
        .collect::<Vec<_>>();
    let media_item_ids = items
        .iter()
        .map(|item| item.media_item.id)
        .collect::<Vec<_>>();
    let mut ratings_by_media_item = list_media_item_ratings(pool, &media_item_ids).await?;
    for item in &mut items {
        item.media_item.ratings = ratings_by_media_item
            .remove(&item.media_item.id)
            .unwrap_or_default();
    }

    Ok(items)
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
    let mut transaction = pool
        .begin()
        .await
        .context("failed to begin playback progress transaction")?;
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
    .fetch_one(&mut *transaction)
    .await
    .context("failed to upsert playback progress")?;

    if params.is_finished {
        sqlx::query(
            r#"
            delete from continue_watching
            where user_id = $1
              and media_item_id = (
                  select coalesce(e.series_id, mi.id)
                  from media_items mi
                  left join episodes e on e.media_item_id = mi.id
                  where mi.id = $2
              )
            "#,
        )
        .bind(params.user_id)
        .bind(params.media_item_id)
        .execute(&mut *transaction)
        .await
        .context("failed to remove finished item from continue watching")?;
    } else {
        sqlx::query(
            r#"
            insert into continue_watching (
                user_id,
                media_item_id,
                last_played_media_item_id,
                media_file_id,
                last_watched_at
            )
            select
                $1,
                coalesce(e.series_id, mi.id),
                mi.id,
                $3,
                now()
            from media_items mi
            left join episodes e on e.media_item_id = mi.id
            where mi.id = $2
            on conflict (user_id, media_item_id) do update
            set last_played_media_item_id = excluded.last_played_media_item_id,
                media_file_id = excluded.media_file_id,
                last_watched_at = excluded.last_watched_at
            "#,
        )
        .bind(params.user_id)
        .bind(params.media_item_id)
        .bind(params.media_file_id)
        .execute(&mut *transaction)
        .await
        .context("failed to upsert continue watching item")?;

        sqlx::query(
            r#"
            delete from continue_watching
            where id in (
                select id
                from continue_watching
                where user_id = $1
                order by last_watched_at desc, id desc
                offset $2
            )
            "#,
        )
        .bind(params.user_id)
        .bind(CONTINUE_WATCHING_LIMIT)
        .execute(&mut *transaction)
        .await
        .context("failed to prune continue watching items")?;
    }

    transaction
        .commit()
        .await
        .context("failed to commit playback progress transaction")?;

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
            source_title: row.get("source_title"),
            original_title: row.get("original_title"),
            sort_title: row.get("sort_title"),
            metadata_provider: row.get("metadata_provider"),
            metadata_provider_item_id: row.get("metadata_provider_item_id"),
            metadata_status: row.get("metadata_status"),
            metadata_failure_reason: row.get("metadata_failure_reason"),
            remote_media_type: row.get("remote_media_type"),
            year: row.get("year"),
            ratings: Vec::new(),
            country: row.get("country"),
            genres: row.get("genres"),
            studio: row.get("studio"),
            overview: row.get("overview"),
            poster_path: row.get("poster_path"),
            backdrop_path: row.get("backdrop_path"),
            logo_path: row.get("logo_path"),
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
