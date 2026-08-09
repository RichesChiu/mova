use super::{
    ratings::{list_media_item_ratings, replace_media_item_remote_data},
    reconcile_local_metadata_snapshots_tx, CreateAudioTrackParams, CreateSubtitleTrackParams,
    ExistingMediaMetadataSummary, GlobalSearchParams, GlobalSearchResult, LibraryMediaTypeCounts,
    ListMediaItemsForLibraryParams, ListMediaItemsForLibraryResult, MediaItemPlaybackHeader,
    RecentlyAddedLibraryMediaItems, SeriesEpisodeOutlineCacheEntry, UpdateMediaFileMetadataParams,
    UpdateMediaItemMetadataParams, UpdateSeriesEpisodeMetadataParams,
    UpdateSeriesSeasonMetadataParams, UpsertSeriesEpisodeOutlineCacheParams,
};
use crate::{
    tmdb_revalidation::{
        lock_library_tmdb_artwork_reference_write, record_authoritative_tmdb_snapshot_tx,
    },
    VisibilityResult,
};
use anyhow::{Context, Result};
use mova_domain::{
    AudioTrack, Episode, Library, MediaFile, MediaItem, MediaSourceKind, Season, SubtitleFile,
};
use sqlx::{
    postgres::{PgPool, PgRow},
    Row,
};
use std::collections::HashMap;
use time::OffsetDateTime;

/// 读取某个媒体库下当前已经入库的媒体条目。
pub async fn list_media_items_for_library(
    pool: &PgPool,
    params: ListMediaItemsForLibraryParams,
) -> Result<ListMediaItemsForLibraryResult> {
    let total_row = sqlx::query(
        r#"
        select count(*) as total
        from media_items
        where library_id = $1
          and media_type in ('movie', 'series')
          and (
                $2::text is null
                or title ilike '%' || $2 || '%'
                or source_title ilike '%' || $2 || '%'
                or coalesce(original_title, '') ilike '%' || $2 || '%'
              )
          and ($3::int is null or year = $3)
        "#,
    )
    .bind(params.library_id)
    .bind(params.query.as_deref())
    .bind(params.year)
    .fetch_one(pool)
    .await
    .context("failed to count media items for library listing")?;

    let rows = sqlx::query(
        r#"
        select
            id,
            library_id,
            media_type,
            title,
            source_title,
            original_title,
            sort_title,
            metadata_provider,
            metadata_provider_item_id,
            metadata_status,
            metadata_failure_reason,
            remote_media_type,
            year,
            tagline,
            premiere_date,
            content_rating,
            country,
            genres,
            studio,
            overview,
            poster_path,
            backdrop_path,
            logo_path,
            created_at,
            updated_at
        from media_items
        where library_id = $1
          and media_type in ('movie', 'series')
          and (
                $2::text is null
                or title ilike '%' || $2 || '%'
                or source_title ilike '%' || $2 || '%'
                or coalesce(original_title, '') ilike '%' || $2 || '%'
              )
          and ($3::int is null or year = $3)
        order by lower(coalesce(nullif(title, ''), source_title)) asc, id asc
        limit $4
        offset $5
        "#,
    )
    .bind(params.library_id)
    .bind(params.query.as_deref())
    .bind(params.year)
    .bind(params.limit)
    .bind(params.offset)
    .fetch_all(pool)
    .await
    .context("failed to list media items for library")?;

    let mut items = rows.into_iter().map(map_media_item_row).collect::<Vec<_>>();
    attach_media_item_ratings(pool, &mut items).await?;

    Ok(ListMediaItemsForLibraryResult {
        items,
        total: total_row.get("total"),
    })
}

/// 批量读取多个媒体库的有界首页预览，避免首页按媒体库产生 N 次列表查询。
pub async fn list_media_item_previews_by_library(
    pool: &PgPool,
    library_ids: &[i64],
    item_limit: i64,
) -> Result<HashMap<i64, Vec<MediaItem>>> {
    if library_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        with ranked_items as (
            select
                id,
                library_id,
                media_type,
                title,
                source_title,
                original_title,
                sort_title,
                metadata_provider,
                metadata_provider_item_id,
                metadata_status,
                metadata_failure_reason,
                remote_media_type,
                year,
                tagline,
                premiere_date,
                content_rating,
                country,
                genres,
                studio,
                overview,
                poster_path,
                backdrop_path,
                logo_path,
                created_at,
                updated_at,
                row_number() over (
                    partition by library_id
                    order by lower(coalesce(nullif(title, ''), source_title)) asc, id asc
                ) as item_rank
            from media_items
            where library_id = any($1)
              and media_type in ('movie', 'series')
        )
        select
            id,
            library_id,
            media_type,
            title,
            source_title,
            original_title,
            sort_title,
            metadata_provider,
            metadata_provider_item_id,
            metadata_status,
            metadata_failure_reason,
            remote_media_type,
            year,
            tagline,
            premiere_date,
            content_rating,
            country,
            genres,
            studio,
            overview,
            poster_path,
            backdrop_path,
            logo_path,
            created_at,
            updated_at
        from ranked_items
        where item_rank <= $2
        order by library_id asc, item_rank asc
        "#,
    )
    .bind(library_ids)
    .bind(item_limit.max(1))
    .fetch_all(pool)
    .await
    .context("failed to list media item previews by library")?;

    let mut items_by_library = HashMap::new();
    for row in rows {
        let item = map_media_item_row(row);
        items_by_library
            .entry(item.library_id)
            .or_insert_with(Vec::new)
            .push(item);
    }
    let media_item_ids = items_by_library
        .values()
        .flat_map(|items| items.iter().map(|item| item.id))
        .collect::<Vec<_>>();
    let mut ratings_by_media_item = list_media_item_ratings(pool, &media_item_ids).await?;
    for items in items_by_library.values_mut() {
        for item in items {
            item.ratings = ratings_by_media_item.remove(&item.id).unwrap_or_default();
        }
    }
    Ok(items_by_library)
}

/// 按媒体条目的入库时间，聚合每个可见媒体库最近新增的内容。
pub async fn list_recently_added_media_items_by_library(
    pool: &PgPool,
    library_ids: Option<&[i64]>,
    item_limit: i64,
    created_since: Option<OffsetDateTime>,
) -> Result<Vec<RecentlyAddedLibraryMediaItems>> {
    let library_ids = library_ids.map(|ids| ids.to_vec());
    let rows = sqlx::query(
        r#"
        with visible_libraries as (
            select
                id,
                name,
                description,
                metadata_language,
                root_path,
                created_at,
                updated_at
            from libraries
            where $1::bigint[] is null or id = any($1)
        ),
        ranked_items as (
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
                mi.tagline,
                mi.premiere_date,
                mi.content_rating,
                mi.country,
                mi.genres,
                mi.studio,
                mi.overview,
                mi.poster_path,
                mi.backdrop_path,
                mi.logo_path,
                mi.created_at,
                mi.updated_at,
                row_number() over (
                    partition by mi.library_id
                    order by mi.created_at desc, mi.id desc
                ) as item_rank,
                count(*) over (partition by mi.library_id) as library_total
            from media_items mi
            join visible_libraries vl on vl.id = mi.library_id
            where mi.media_type in ('movie', 'series')
                and ($3::timestamptz is null or mi.created_at >= $3)
        ),
        library_recency as (
            select
                library_id,
                max(created_at) as latest_created_at,
                max(library_total) as total
            from ranked_items
            group by library_id
            order by max(created_at) desc, library_id asc
        )
        select
            vl.id as library_id,
            vl.name as library_name,
            vl.description as library_description,
            vl.metadata_language as library_metadata_language,
            vl.root_path as library_root_path,
            vl.created_at as library_created_at,
            vl.updated_at as library_updated_at,
            lr.total as library_total,
            ri.id as media_item_id,
            ri.library_id as media_item_library_id,
            ri.media_type as media_item_media_type,
            ri.title as media_item_title,
            ri.source_title as media_item_source_title,
            ri.original_title as media_item_original_title,
            ri.sort_title as media_item_sort_title,
            ri.metadata_provider as media_item_metadata_provider,
            ri.metadata_provider_item_id as media_item_metadata_provider_item_id,
            ri.metadata_status as media_item_metadata_status,
            ri.metadata_failure_reason as media_item_metadata_failure_reason,
            ri.remote_media_type as media_item_remote_media_type,
            ri.year as media_item_year,
            ri.tagline as media_item_tagline,
            ri.premiere_date as media_item_premiere_date,
            ri.content_rating as media_item_content_rating,
            ri.country as media_item_country,
            ri.genres as media_item_genres,
            ri.studio as media_item_studio,
            ri.overview as media_item_overview,
            ri.poster_path as media_item_poster_path,
            ri.backdrop_path as media_item_backdrop_path,
            ri.logo_path as media_item_logo_path,
            ri.created_at as media_item_created_at,
            ri.updated_at as media_item_updated_at
        from library_recency lr
        join visible_libraries vl on vl.id = lr.library_id
        join ranked_items ri on ri.library_id = lr.library_id
            and ri.item_rank <= $2
        order by lr.latest_created_at desc, vl.id asc, ri.created_at desc, ri.id desc
        "#,
    )
    .bind(library_ids.as_deref())
    .bind(item_limit)
    .bind(created_since)
    .fetch_all(pool)
    .await
    .context("failed to list recently added media items by library")?;

    let mut groups: Vec<RecentlyAddedLibraryMediaItems> = Vec::new();

    for row in rows {
        let library_id: i64 = row.get("library_id");
        if !groups
            .last()
            .map(|group| group.library.id == library_id)
            .unwrap_or(false)
        {
            groups.push(RecentlyAddedLibraryMediaItems {
                library: map_recently_added_library_row(&row),
                items: Vec::new(),
                total: row.get("library_total"),
            });
        }

        if let Some(group) = groups.last_mut() {
            group.items.push(map_recently_added_media_item_row(&row));
        }
    }

    let media_item_ids = groups
        .iter()
        .flat_map(|group| group.items.iter().map(|item| item.id))
        .collect::<Vec<_>>();
    let mut ratings_by_media_item = list_media_item_ratings(pool, &media_item_ids).await?;
    for group in &mut groups {
        for item in &mut group.items {
            item.ratings = ratings_by_media_item.remove(&item.id).unwrap_or_default();
        }
    }

    Ok(groups)
}

/// 全局搜索用户可见库下的电影、剧集和本地集条目。
pub async fn global_search(
    pool: &PgPool,
    params: GlobalSearchParams,
) -> Result<Vec<GlobalSearchResult>> {
    let visible_library_ids = params.visible_library_ids;
    let rows = sqlx::query(
        r#"
        with search_results as (
            select
                'media_item'::text as kind,
                mi.library_id,
                l.name as library_name,
                mi.id as media_item_id,
                null::bigint as series_media_item_id,
                mi.media_type,
                mi.title,
                null::text as subtitle,
                mi.year,
                mi.overview,
                mi.poster_path,
                mi.backdrop_path,
                null::integer as season_number,
                null::integer as episode_number,
                mi.updated_at,
                case
                    when lower(mi.title) = lower($1) then 0
                    when mi.title ilike $1 || '%' then 1
                    when mi.source_title ilike $1 || '%' then 2
                    else 3
                end as result_rank
            from media_items mi
            join libraries l on l.id = mi.library_id
            where mi.media_type in ('movie', 'series')
              and ($2::bigint[] is null or mi.library_id = any($2))
              and (
                    mi.title ilike '%' || $1 || '%'
                    or mi.source_title ilike '%' || $1 || '%'
                    or coalesce(mi.original_title, '') ilike '%' || $1 || '%'
                  )

            union all

            select
                'episode'::text as kind,
                episode_mi.library_id,
                l.name as library_name,
                episode_mi.id as media_item_id,
                s.series_id as series_media_item_id,
                episode_mi.media_type,
                episode_mi.title,
                series_mi.title as subtitle,
                series_mi.year,
                episode_mi.overview,
                episode_mi.poster_path,
                episode_mi.backdrop_path,
                s.season_number,
                e.episode_number,
                episode_mi.updated_at,
                case
                    when lower(episode_mi.title) = lower($1) then 0
                    when episode_mi.title ilike $1 || '%' then 1
                    when series_mi.title ilike $1 || '%' then 2
                    else 3
                end as result_rank
            from episodes e
            join media_items episode_mi on episode_mi.id = e.media_item_id
            join seasons s on s.id = e.season_id
            join media_items series_mi on series_mi.id = s.series_id
            join libraries l on l.id = episode_mi.library_id
            where ($2::bigint[] is null or episode_mi.library_id = any($2))
              and (
                    episode_mi.title ilike '%' || $1 || '%'
                    or episode_mi.source_title ilike '%' || $1 || '%'
                    or coalesce(episode_mi.original_title, '') ilike '%' || $1 || '%'
                    or series_mi.title ilike '%' || $1 || '%'
                    or series_mi.source_title ilike '%' || $1 || '%'
                    or coalesce(series_mi.original_title, '') ilike '%' || $1 || '%'
                  )
        )
        select
            kind,
            library_id,
            library_name,
            media_item_id,
            series_media_item_id,
            media_type,
            title,
            subtitle,
            year,
            overview,
            poster_path,
            backdrop_path,
            season_number,
            episode_number,
            updated_at
        from search_results
        order by result_rank asc, updated_at desc, media_item_id asc
        limit $3
        "#,
    )
    .bind(&params.query)
    .bind(visible_library_ids.as_deref())
    .bind(params.limit)
    .fetch_all(pool)
    .await
    .context("failed to search media library")?;

    let mut results = rows
        .into_iter()
        .map(map_global_search_result_row)
        .collect::<Vec<_>>();
    let media_item_ids = results
        .iter()
        .map(|result| result.media_item_id)
        .collect::<Vec<_>>();
    let mut ratings_by_media_item = list_media_item_ratings(pool, &media_item_ids).await?;

    for result in &mut results {
        result.ratings = ratings_by_media_item
            .remove(&result.media_item_id)
            .unwrap_or_default();
    }

    Ok(results)
}

/// 按主键读取单个媒体条目。
pub async fn get_media_item(pool: &PgPool, media_item_id: i64) -> Result<Option<MediaItem>> {
    let row = sqlx::query(
        r#"
        select
            id,
            library_id,
            media_type,
            title,
            source_title,
            original_title,
            sort_title,
            metadata_provider,
            metadata_provider_item_id,
            metadata_status,
            metadata_failure_reason,
            remote_media_type,
            year,
            tagline,
            premiere_date,
            content_rating,
            country,
            genres,
            studio,
            overview,
            poster_path,
            backdrop_path,
            logo_path,
            created_at,
            updated_at
        from media_items
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await
    .context("failed to get media item")?;

    let mut media_item = row.map(map_media_item_row);
    if let Some(media_item) = media_item.as_mut() {
        attach_media_item_ratings(pool, std::slice::from_mut(media_item)).await?;
    }

    Ok(media_item)
}

/// 一次查询读取媒体条目及所属媒体库，并在 SQL 中应用可见范围。
pub async fn get_media_item_with_library_visibility(
    pool: &PgPool,
    media_item_id: i64,
    visible_library_ids: Option<&[i64]>,
) -> Result<VisibilityResult<(MediaItem, Library)>> {
    let row = sqlx::query(
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
            mi.tagline,
            mi.premiere_date,
            mi.content_rating,
            mi.country,
            mi.genres,
            mi.studio,
            mi.overview,
            mi.poster_path,
            mi.backdrop_path,
            mi.logo_path,
            mi.created_at,
            mi.updated_at,
            l.name as access_library_name,
            l.description as access_library_description,
            l.metadata_language as access_library_metadata_language,
            l.root_path as access_library_root_path,
            l.created_at as access_library_created_at,
            l.updated_at as access_library_updated_at,
            ($2::bigint[] is null or mi.library_id = any($2)) as is_visible
        from media_items mi
        join libraries l on l.id = mi.library_id
        where mi.id = $1
        "#,
    )
    .bind(media_item_id)
    .bind(visible_library_ids)
    .fetch_optional(pool)
    .await
    .context("failed to get visible media item and library")?;

    let Some(row) = row else {
        return Ok(VisibilityResult::Missing);
    };
    if !row.get::<bool, _>("is_visible") {
        return Ok(VisibilityResult::Forbidden {
            library_id: row.get("library_id"),
        });
    }

    let library = map_access_library_row(&row);
    let mut media_item = map_media_item_row(row);
    attach_media_item_ratings(pool, std::slice::from_mut(&mut media_item)).await?;
    Ok(VisibilityResult::Visible((media_item, library)))
}

pub async fn get_media_item_playback_header(
    pool: &PgPool,
    media_item_id: i64,
) -> Result<Option<MediaItemPlaybackHeader>> {
    let row = sqlx::query(
        r#"
        select
            mi.id as media_item_id,
            mi.library_id,
            mi.media_type,
            s.series_id as series_media_item_id,
            case
                when mi.media_type = 'episode' then coalesce(nullif(series_mi.title, ''), mi.title)
                else mi.title
            end as title,
            case
                when mi.media_type = 'episode' then coalesce(nullif(series_mi.original_title, ''), mi.original_title)
                else mi.original_title
            end as original_title,
            case
                when mi.media_type = 'episode' then coalesce(series_mi.year, mi.year)
                else mi.year
            end as year,
            case
                when mi.media_type = 'episode' then series_mi.logo_path
                else mi.logo_path
            end as logo_path,
            case
                when mi.media_type = 'episode' then series_mi.updated_at
                else mi.updated_at
            end as logo_updated_at,
            e.season_id,
            s.season_number,
            e.episode_number,
            case
                when mi.media_type = 'episode' then nullif(mi.title, '')
                else null
            end as episode_title,
            s.intro_start_seconds as season_intro_start_seconds,
            s.intro_end_seconds as season_intro_end_seconds,
            e.intro_start_seconds as episode_intro_start_seconds,
            e.intro_end_seconds as episode_intro_end_seconds
        from media_items mi
        left join episodes e on e.media_item_id = mi.id
        left join seasons s on s.id = e.season_id
        left join media_items series_mi on series_mi.id = s.series_id
        where mi.id = $1
        "#,
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await
    .context("failed to get media item playback header")?;

    Ok(row.map(map_media_item_playback_header_row))
}

/// 更新单个媒体条目的 metadata 字段。
pub async fn update_media_item_metadata(
    pool: &PgPool,
    media_item_id: i64,
    params: UpdateMediaItemMetadataParams,
) -> Result<Option<MediaItem>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start media metadata update transaction")?;
    let previous_identity = sqlx::query(
        r#"
        select
            library_id,
            media_type,
            metadata_provider,
            metadata_provider_item_id
        from media_items
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to resolve media item identity before metadata update")?;
    let Some(previous_identity) = previous_identity else {
        tx.rollback()
            .await
            .context("failed to roll back missing media metadata update")?;
        return Ok(None);
    };
    let library_id = previous_identity.get::<i64, _>("library_id");
    let previous_media_type = previous_identity.get::<String, _>("media_type");
    let previous_metadata_provider =
        previous_identity.get::<Option<String>, _>("metadata_provider");
    let previous_metadata_provider_item_id =
        previous_identity.get::<Option<String>, _>("metadata_provider_item_id");
    let superseded_tmdb_series_id = previous_metadata_provider_item_id.filter(|previous_id| {
        previous_media_type.eq_ignore_ascii_case("series")
            && previous_metadata_provider
                .as_deref()
                .is_some_and(|provider| provider.eq_ignore_ascii_case("tmdb"))
            && params
                .metadata_provider
                .as_deref()
                .is_some_and(|provider| provider.eq_ignore_ascii_case("tmdb"))
            && params.metadata_status.eq_ignore_ascii_case("matched")
            && params
                .metadata_provider_item_id
                .as_deref()
                .is_some_and(|provider_item_id| provider_item_id != previous_id)
    });
    lock_library_tmdb_artwork_reference_write(&mut tx, library_id).await?;
    sqlx::query("select set_config('mova.defer_catalog_revision', 'on', true)")
        .fetch_one(&mut *tx)
        .await
        .context("failed to defer catalog revision for media metadata update")?;
    let row = sqlx::query(
        r#"
        update media_items
        set
            title = $2,
            source_title = $3,
            original_title = $4,
            sort_title = $5,
            metadata_provider = $6,
            metadata_provider_item_id = $7,
            metadata_status = $8,
            metadata_failure_reason = $9,
            remote_media_type = $10,
            year = $11,
            country = $12,
            genres = $13,
            studio = $14,
            overview = $15,
            poster_path = $16,
            backdrop_path = $17,
            logo_path = $18,
            tagline = $19,
            premiere_date = $20,
            content_rating = $21,
            updated_at = now()
        where id = $1
          and library_id = $22
          and updated_at = $23
        returning
            id,
            library_id,
            media_type,
            title,
            source_title,
            original_title,
            sort_title,
            metadata_provider,
            metadata_provider_item_id,
            metadata_status,
            metadata_failure_reason,
            remote_media_type,
            year,
            tagline,
            premiere_date,
            content_rating,
            country,
            genres,
            studio,
            overview,
            poster_path,
            backdrop_path,
            logo_path,
            created_at,
            updated_at
        "#,
    )
    .bind(media_item_id)
    .bind(&params.title)
    .bind(&params.source_title)
    .bind(&params.original_title)
    .bind(&params.sort_title)
    .bind(&params.metadata_provider)
    .bind(params.metadata_provider_item_id)
    .bind(&params.metadata_status)
    .bind(&params.metadata_failure_reason)
    .bind(&params.remote_media_type)
    .bind(params.year)
    .bind(&params.country)
    .bind(&params.genres)
    .bind(&params.studio)
    .bind(&params.overview)
    .bind(&params.poster_path)
    .bind(&params.backdrop_path)
    .bind(&params.logo_path)
    .bind(&params.tagline)
    .bind(params.premiere_date)
    .bind(&params.content_rating)
    .bind(library_id)
    .bind(params.expected_updated_at)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to update media item metadata")?;

    if row.is_some() && params.replace_remote_data {
        replace_media_item_remote_data(
            &mut tx,
            media_item_id,
            params.metadata_provider.as_deref(),
            &params.external_ids,
            &params.ratings,
        )
        .await?;
        record_authoritative_tmdb_snapshot_tx(
            &mut tx,
            media_item_id,
            params.metadata_provider.as_deref(),
            params.tmdb_remote_snapshot_json.as_deref(),
            params.tmdb_remote_snapshot_renews_retention,
        )
        .await?;
    }

    if row.is_some() {
        reconcile_local_metadata_snapshots_tx(
            &mut tx,
            library_id,
            crate::local_metadata::MediaLocalMetadataTarget::MediaItem(media_item_id),
            &params.removed_local_nfo_source_paths,
            &params.local_nfos,
        )
        .await?;
    }

    if row.is_some() && previous_media_type.eq_ignore_ascii_case("series") {
        for season in &params.seasons {
            sqlx::query(
                r#"
                update seasons
                set title = $3,
                    overview = $4,
                    poster_path = $5,
                    backdrop_path = $6,
                    updated_at = now()
                where series_id = $1
                  and season_number = $2
                "#,
            )
            .bind(media_item_id)
            .bind(season.season_number)
            .bind(&season.title)
            .bind(&season.overview)
            .bind(&season.poster_path)
            .bind(&season.backdrop_path)
            .execute(&mut *tx)
            .await
            .context("failed to update season metadata during parent refresh")?;
        }
    }

    if row.is_some() {
        if let Some(superseded_tmdb_series_id) = superseded_tmdb_series_id.as_deref() {
            clear_superseded_tmdb_series_episode_identity_tx(
                &mut tx,
                media_item_id,
                superseded_tmdb_series_id,
            )
            .await?;
        }
    }

    if let Some(row) = row.as_ref() {
        let library_id: i64 = row.get("library_id");
        sqlx::query("select mova_bump_realtime_revision($1)")
            .bind(format!("library:{library_id}:catalog"))
            .fetch_one(&mut *tx)
            .await
            .context("failed to bump media metadata catalog revision")?;
    }

    tx.commit()
        .await
        .context("failed to commit media metadata update transaction")?;

    let mut media_item = row.map(map_media_item_row);
    if let Some(media_item) = media_item.as_mut() {
        attach_media_item_ratings(pool, std::slice::from_mut(media_item)).await?;
    }
    Ok(media_item)
}

async fn clear_superseded_tmdb_series_episode_identity_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    series_id: i64,
    superseded_provider_item_id: &str,
) -> Result<()> {
    let episode_ids = sqlx::query_scalar::<_, i64>(
        r#"
        select item.id
        from media_items item
        join episodes episode on episode.media_item_id = item.id
        join seasons season on season.id = episode.season_id
        where season.series_id = $1
          and item.media_type = 'episode'
          and item.metadata_provider = 'tmdb'
          and item.metadata_provider_item_id = $2
        order by item.id
        for update of item
        "#,
    )
    .bind(series_id)
    .bind(superseded_provider_item_id)
    .fetch_all(&mut **tx)
    .await
    .context("failed to lock episodes owned by the superseded TMDB series binding")?;
    if episode_ids.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r#"
        delete from media_item_external_ids
        where media_item_id = any($1)
          and retrieved_via not in ('nfo', 'manual')
        "#,
    )
    .bind(&episode_ids)
    .execute(&mut **tx)
    .await
    .context("failed to clear external ids owned by the superseded TMDB series binding")?;
    sqlx::query(
        r#"
        delete from media_item_ratings
        where media_item_id = any($1)
          and retrieved_via not in ('nfo', 'manual')
        "#,
    )
    .bind(&episode_ids)
    .execute(&mut **tx)
    .await
    .context("failed to clear ratings owned by the superseded TMDB series binding")?;

    let cleared = sqlx::query(
        r#"
        update media_items
        set metadata_provider = null,
            metadata_provider_item_id = null,
            updated_at = now()
        where id = any($1)
          and metadata_provider = 'tmdb'
          and metadata_provider_item_id = $2
        "#,
    )
    .bind(&episode_ids)
    .bind(superseded_provider_item_id)
    .execute(&mut **tx)
    .await
    .context("failed to clear episode identities owned by the superseded TMDB series binding")?;
    if cleared.rows_affected() != episode_ids.len() as u64 {
        anyhow::bail!("superseded TMDB episode identity changed while replacing series metadata");
    }

    Ok(())
}

/// 按主键读取单个媒体文件。
pub async fn get_media_file(pool: &PgPool, media_file_id: i64) -> Result<Option<MediaFile>> {
    let row = sqlx::query(
        r#"
        select
            id,
            media_item_id,
            library_id,
            file_path,
            source_kind,
            stream_reference_hash,
            container,
            file_size,
            duration_seconds,
            video_title,
            video_codec,
            video_profile,
            video_level,
            audio_codec,
            width,
            height,
            bitrate,
            video_bitrate,
            video_frame_rate,
            video_aspect_ratio,
            video_scan_type,
            video_color_primaries,
            video_color_space,
            video_color_transfer,
            video_bit_depth,
            video_pixel_format,
            video_reference_frames,
            technical_tags,
            scan_hash,
            created_at,
            updated_at
        from media_files
        where id = $1
        "#,
    )
    .bind(media_file_id)
    .fetch_optional(pool)
    .await
    .context("failed to get media file")?;

    Ok(row.map(map_media_file_row))
}

/// 一次查询读取媒体文件及所属媒体库，并在 SQL 中应用可见范围。
pub async fn get_media_file_with_library_visibility(
    pool: &PgPool,
    media_file_id: i64,
    visible_library_ids: Option<&[i64]>,
) -> Result<VisibilityResult<(MediaFile, Library)>> {
    let row = sqlx::query(
        r#"
        select
            mf.id,
            mf.media_item_id,
            mf.library_id,
            mf.file_path,
            mf.source_kind,
            mf.stream_reference_hash,
            mf.container,
            mf.file_size,
            mf.duration_seconds,
            mf.video_title,
            mf.video_codec,
            mf.video_profile,
            mf.video_level,
            mf.audio_codec,
            mf.width,
            mf.height,
            mf.bitrate,
            mf.video_bitrate,
            mf.video_frame_rate,
            mf.video_aspect_ratio,
            mf.video_scan_type,
            mf.video_color_primaries,
            mf.video_color_space,
            mf.video_color_transfer,
            mf.video_bit_depth,
            mf.video_pixel_format,
            mf.video_reference_frames,
            mf.technical_tags,
            mf.scan_hash,
            mf.created_at,
            mf.updated_at,
            l.name as access_library_name,
            l.description as access_library_description,
            l.metadata_language as access_library_metadata_language,
            l.root_path as access_library_root_path,
            l.created_at as access_library_created_at,
            l.updated_at as access_library_updated_at,
            ($2::bigint[] is null or mf.library_id = any($2)) as is_visible
        from media_files mf
        join libraries l on l.id = mf.library_id
        where mf.id = $1
        "#,
    )
    .bind(media_file_id)
    .bind(visible_library_ids)
    .fetch_optional(pool)
    .await
    .context("failed to get visible media file and library")?;

    Ok(match row {
        None => VisibilityResult::Missing,
        Some(row) if !row.get::<bool, _>("is_visible") => VisibilityResult::Forbidden {
            library_id: row.get("library_id"),
        },
        Some(row) => {
            let library = map_access_library_row(&row);
            VisibilityResult::Visible((map_media_file_row(row), library))
        }
    })
}

/// 更新单个媒体文件的路径和探测字段。
pub async fn update_media_file_metadata(
    pool: &PgPool,
    media_file_id: i64,
    params: UpdateMediaFileMetadataParams,
) -> Result<Option<MediaFile>> {
    let row = sqlx::query(
        r#"
        update media_files
        set
            file_path = $2,
            source_kind = $3,
            stream_reference_hash = $4,
            container = $5,
            file_size = $6,
            duration_seconds = $7,
            video_title = $8,
            video_codec = $9,
            video_profile = $10,
            video_level = $11,
            audio_codec = $12,
            width = $13,
            height = $14,
            bitrate = $15,
            video_bitrate = $16,
            video_frame_rate = $17,
            video_aspect_ratio = $18,
            video_scan_type = $19,
            video_color_primaries = $20,
            video_color_space = $21,
            video_color_transfer = $22,
            video_bit_depth = $23,
            video_pixel_format = $24,
            video_reference_frames = $25,
            technical_tags = $26,
            updated_at = now()
        where id = $1
        returning
            id,
            media_item_id,
            library_id,
            file_path,
            source_kind,
            stream_reference_hash,
            container,
            file_size,
            duration_seconds,
            video_title,
            video_codec,
            video_profile,
            video_level,
            audio_codec,
            width,
            height,
            bitrate,
            video_bitrate,
            video_frame_rate,
            video_aspect_ratio,
            video_scan_type,
            video_color_primaries,
            video_color_space,
            video_color_transfer,
            video_bit_depth,
            video_pixel_format,
            video_reference_frames,
            technical_tags,
            scan_hash,
            created_at,
            updated_at
        "#,
    )
    .bind(media_file_id)
    .bind(&params.file_path)
    .bind(params.source_kind.as_str())
    .bind(&params.stream_reference_hash)
    .bind(&params.container)
    .bind(params.file_size)
    .bind(params.duration_seconds)
    .bind(&params.video_title)
    .bind(&params.video_codec)
    .bind(&params.video_profile)
    .bind(&params.video_level)
    .bind(&params.audio_codec)
    .bind(params.width)
    .bind(params.height)
    .bind(params.bitrate)
    .bind(params.video_bitrate)
    .bind(params.video_frame_rate)
    .bind(&params.video_aspect_ratio)
    .bind(&params.video_scan_type)
    .bind(&params.video_color_primaries)
    .bind(&params.video_color_space)
    .bind(&params.video_color_transfer)
    .bind(params.video_bit_depth)
    .bind(&params.video_pixel_format)
    .bind(params.video_reference_frames)
    .bind(&params.technical_tags)
    .fetch_optional(pool)
    .await
    .context("failed to update media file metadata")?;

    Ok(row.map(map_media_file_row))
}

/// 读取某个媒体条目关联的文件列表。
pub async fn list_media_files_for_media_item(
    pool: &PgPool,
    media_item_id: i64,
) -> Result<Vec<MediaFile>> {
    let rows = sqlx::query(
        r#"
        select
            id,
            media_item_id,
            library_id,
            file_path,
            source_kind,
            stream_reference_hash,
            container,
            file_size,
            duration_seconds,
            video_title,
            video_codec,
            video_profile,
            video_level,
            audio_codec,
            width,
            height,
            bitrate,
            video_bitrate,
            video_frame_rate,
            video_aspect_ratio,
            video_scan_type,
            video_color_primaries,
            video_color_space,
            video_color_transfer,
            video_bit_depth,
            video_pixel_format,
            video_reference_frames,
            technical_tags,
            scan_hash,
            created_at,
            updated_at
        from media_files
        where media_item_id = $1
        order by created_at asc, id asc
        "#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await
    .context("failed to list media files for media item")?;

    Ok(rows.into_iter().map(map_media_file_row).collect())
}

/// List every filesystem carrier that can contribute local metadata to one
/// logical item. A series parent owns no file directly, so all files below its
/// seasons and episodes are returned as `tvshow.nfo` lookup anchors.
pub async fn list_media_item_metadata_refresh_source_files(
    pool: &PgPool,
    media_item_id: i64,
) -> Result<Vec<MediaFile>> {
    let rows = sqlx::query(
        r#"
        select
            mf.id,
            mf.media_item_id,
            mf.library_id,
            mf.file_path,
            mf.source_kind,
            mf.stream_reference_hash,
            mf.container,
            mf.file_size,
            mf.duration_seconds,
            mf.video_title,
            mf.video_codec,
            mf.video_profile,
            mf.video_level,
            mf.audio_codec,
            mf.width,
            mf.height,
            mf.bitrate,
            mf.video_bitrate,
            mf.video_frame_rate,
            mf.video_aspect_ratio,
            mf.video_scan_type,
            mf.video_color_primaries,
            mf.video_color_space,
            mf.video_color_transfer,
            mf.video_bit_depth,
            mf.video_pixel_format,
            mf.video_reference_frames,
            mf.technical_tags,
            mf.scan_hash,
            mf.created_at,
            mf.updated_at
        from media_files mf
        left join episodes e on e.media_item_id = mf.media_item_id
        left join seasons s on s.id = e.season_id
        where mf.media_item_id = $1
           or s.series_id = $1
        order by
            case when mf.media_item_id = $1 then 0 else 1 end,
            s.season_number asc nulls first,
            e.episode_number asc nulls first,
            mf.file_path asc,
            mf.id asc
        "#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await
    .context("failed to list media item metadata refresh source files")?;

    Ok(rows.into_iter().map(map_media_file_row).collect())
}

/// 读取某个媒体文件可切换的字幕轨道列表。
pub async fn list_subtitle_files_for_media_file(
    pool: &PgPool,
    media_file_id: i64,
) -> Result<Vec<SubtitleFile>> {
    let rows = sqlx::query(
        r#"
        select
            id,
            media_file_id,
            source_kind,
            file_path,
            stream_index,
            language,
            subtitle_format,
            label,
            is_default,
            is_forced,
            is_hearing_impaired,
            created_at,
            updated_at
        from subtitle_files
        where media_file_id = $1
        order by
            is_default desc,
            is_forced desc,
            coalesce(language, '') asc,
            id asc
        "#,
    )
    .bind(media_file_id)
    .fetch_all(pool)
    .await
    .context("failed to list subtitle files for media file")?;

    Ok(rows.into_iter().map(map_subtitle_file_row).collect())
}

/// 读取某个媒体文件可切换的音轨列表。
pub async fn list_audio_tracks_for_media_file(
    pool: &PgPool,
    media_file_id: i64,
) -> Result<Vec<AudioTrack>> {
    let rows = sqlx::query(
        r#"
        select
            id,
            media_file_id,
            stream_index,
            language,
            audio_codec,
            label,
            channel_layout,
            channels,
            bitrate,
            sample_rate,
            is_default,
            created_at,
            updated_at
        from audio_tracks
        where media_file_id = $1
        order by
            is_default desc,
            coalesce(language, '') asc,
            id asc
        "#,
    )
    .bind(media_file_id)
    .fetch_all(pool)
    .await
    .context("failed to list audio tracks for media file")?;

    Ok(rows.into_iter().map(map_audio_track_row).collect())
}

/// 批量读取多个媒体文件的音轨，供扫描复用本地分析时避免逐文件查询。
pub async fn list_audio_tracks_for_media_files(
    pool: &PgPool,
    media_file_ids: &[i64],
) -> Result<Vec<AudioTrack>> {
    if media_file_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        select
            id,
            media_file_id,
            stream_index,
            language,
            audio_codec,
            label,
            channel_layout,
            channels,
            bitrate,
            sample_rate,
            is_default,
            created_at,
            updated_at
        from audio_tracks
        where media_file_id = any($1)
        order by
            media_file_id asc,
            is_default desc,
            coalesce(language, '') asc,
            id asc
        "#,
    )
    .bind(media_file_ids)
    .fetch_all(pool)
    .await
    .context("failed to list audio tracks for media files")?;

    Ok(rows.into_iter().map(map_audio_track_row).collect())
}

/// 批量读取多个媒体文件的字幕，供扫描复用本地分析时避免逐文件查询。
pub async fn list_subtitle_files_for_media_files(
    pool: &PgPool,
    media_file_ids: &[i64],
) -> Result<Vec<SubtitleFile>> {
    if media_file_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        select
            id,
            media_file_id,
            source_kind,
            file_path,
            stream_index,
            language,
            subtitle_format,
            label,
            is_default,
            is_forced,
            is_hearing_impaired,
            created_at,
            updated_at
        from subtitle_files
        where media_file_id = any($1)
        order by
            media_file_id asc,
            is_default desc,
            is_forced desc,
            coalesce(language, '') asc,
            id asc
        "#,
    )
    .bind(media_file_ids)
    .fetch_all(pool)
    .await
    .context("failed to list subtitle files for media files")?;

    Ok(rows.into_iter().map(map_subtitle_file_row).collect())
}

/// 通过主键读取单条字幕轨道。
pub async fn get_subtitle_file(
    pool: &PgPool,
    subtitle_file_id: i64,
) -> Result<Option<SubtitleFile>> {
    let row = sqlx::query(
        r#"
        select
            id,
            media_file_id,
            source_kind,
            file_path,
            stream_index,
            language,
            subtitle_format,
            label,
            is_default,
            is_forced,
            is_hearing_impaired,
            created_at,
            updated_at
        from subtitle_files
        where id = $1
        "#,
    )
    .bind(subtitle_file_id)
    .fetch_optional(pool)
    .await
    .context("failed to get subtitle file")?;

    Ok(row.map(map_subtitle_file_row))
}

/// 通过主键读取单条音轨。
pub async fn get_audio_track(pool: &PgPool, audio_track_id: i64) -> Result<Option<AudioTrack>> {
    let row = sqlx::query(
        r#"
        select
            id,
            media_file_id,
            stream_index,
            language,
            audio_codec,
            label,
            channel_layout,
            channels,
            bitrate,
            sample_rate,
            is_default,
            created_at,
            updated_at
        from audio_tracks
        where id = $1
        "#,
    )
    .bind(audio_track_id)
    .fetch_optional(pool)
    .await
    .context("failed to get audio track")?;

    Ok(row.map(map_audio_track_row))
}

/// 每次扫描后直接整体替换某个媒体文件的音轨清单，避免做复杂 diff。
pub async fn replace_audio_tracks_for_media_file(
    pool: &PgPool,
    media_file_id: i64,
    audio_tracks: &[CreateAudioTrackParams],
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start audio track replacement transaction")?;

    sqlx::query(
        r#"
        delete from audio_tracks
        where media_file_id = $1
        "#,
    )
    .bind(media_file_id)
    .execute(&mut *tx)
    .await
    .context("failed to delete existing audio tracks")?;

    for audio_track in audio_tracks {
        sqlx::query(
            r#"
            insert into audio_tracks (
                media_file_id,
                stream_index,
                language,
                audio_codec,
                label,
                channel_layout,
                channels,
                bitrate,
                sample_rate,
                is_default
            ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(media_file_id)
        .bind(audio_track.stream_index)
        .bind(&audio_track.language)
        .bind(&audio_track.audio_codec)
        .bind(&audio_track.label)
        .bind(&audio_track.channel_layout)
        .bind(audio_track.channels)
        .bind(audio_track.bitrate)
        .bind(audio_track.sample_rate)
        .bind(audio_track.is_default)
        .execute(&mut *tx)
        .await
        .context("failed to insert audio track")?;
    }

    tx.commit()
        .await
        .context("failed to commit audio track replacement transaction")?;

    Ok(())
}

/// 每次扫描后直接整体替换某个媒体文件的字幕清单，避免做复杂 diff。
pub async fn replace_subtitle_files_for_media_file(
    pool: &PgPool,
    media_file_id: i64,
    subtitles: &[CreateSubtitleTrackParams],
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start subtitle replacement transaction")?;

    sqlx::query(
        r#"
        delete from subtitle_files
        where media_file_id = $1
        "#,
    )
    .bind(media_file_id)
    .execute(&mut *tx)
    .await
    .context("failed to delete existing subtitle files")?;

    for subtitle in subtitles {
        sqlx::query(
            r#"
            insert into subtitle_files (
                media_file_id,
                source_kind,
                file_path,
                stream_index,
                language,
                subtitle_format,
                label,
                is_default,
                is_forced,
                is_hearing_impaired
            ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(media_file_id)
        .bind(&subtitle.source_kind)
        .bind(&subtitle.file_path)
        .bind(subtitle.stream_index)
        .bind(&subtitle.language)
        .bind(&subtitle.subtitle_format)
        .bind(&subtitle.label)
        .bind(subtitle.is_default)
        .bind(subtitle.is_forced)
        .bind(subtitle.is_hearing_impaired)
        .execute(&mut *tx)
        .await
        .context("failed to insert subtitle file")?;
    }

    tx.commit()
        .await
        .context("failed to commit subtitle replacement transaction")?;

    Ok(())
}

pub async fn list_seasons_for_series(pool: &PgPool, series_id: i64) -> Result<Vec<Season>> {
    let rows = sqlx::query(
        r#"
        select
            s.id,
            s.series_id,
            s.season_number,
            s.title,
            s.overview,
            s.poster_path,
            s.backdrop_path,
            s.intro_start_seconds,
            s.intro_end_seconds,
            count(e.media_item_id) as episode_count,
            s.created_at,
            s.updated_at
        from seasons s
        left join episodes e on e.season_id = s.id
        where s.series_id = $1
        group by
            s.id,
            s.series_id,
            s.season_number,
            s.title,
            s.overview,
            s.poster_path,
            s.backdrop_path,
            s.intro_start_seconds,
            s.intro_end_seconds,
            s.created_at,
            s.updated_at
        order by s.season_number asc, s.id asc
        "#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .context("failed to list seasons for series")?;

    Ok(rows.into_iter().map(map_season_row).collect())
}

pub async fn list_episodes_for_season(pool: &PgPool, season_id: i64) -> Result<Vec<Episode>> {
    let rows = sqlx::query(
        r#"
        select
            e.media_item_id,
            s.series_id,
            e.season_id,
            e.episode_number,
            mi.title,
            mi.overview,
            mi.poster_path,
            mi.backdrop_path,
            e.intro_start_seconds,
            e.intro_end_seconds
        from episodes e
        join media_items mi on mi.id = e.media_item_id
        join seasons s on s.id = e.season_id
        where e.season_id = $1
        order by e.episode_number asc, e.media_item_id asc
        "#,
    )
    .bind(season_id)
    .fetch_all(pool)
    .await
    .context("failed to list episodes for season")?;

    Ok(rows.into_iter().map(map_episode_row).collect())
}

pub async fn list_series_media_item_ids_for_library(
    pool: &PgPool,
    library_id: i64,
) -> Result<Vec<i64>> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select id
        from media_items
        where library_id = $1
          and media_type = 'series'
        order by id asc
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .context("failed to list series media items for library")
}

pub async fn update_series_season_metadata(
    pool: &PgPool,
    params: UpdateSeriesSeasonMetadataParams,
) -> Result<Option<u64>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start series season metadata update transaction")?;
    let library_id = sqlx::query_scalar::<_, i64>(
        "select library_id from media_items where id = $1 and media_type = 'series'",
    )
    .bind(params.series_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to resolve series library before season metadata update")?;
    let Some(library_id) = library_id else {
        tx.rollback()
            .await
            .context("failed to roll back missing series season metadata update")?;
        return Ok(None);
    };
    lock_library_tmdb_artwork_reference_write(&mut tx, library_id).await?;
    let current = sqlx::query_scalar::<_, i64>(
        r#"
        select id
        from media_items
        where id = $1
          and media_type = 'series'
          and metadata_provider = 'tmdb'
          and metadata_provider_item_id = $2
          and metadata_status = 'matched'
          and updated_at = $3
        for update
        "#,
    )
    .bind(params.series_id)
    .bind(&params.expected_provider_item_id)
    .bind(params.expected_media_item_updated_at)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to lock the current TMDB series binding for season metadata")?;
    if current.is_none() {
        tx.rollback()
            .await
            .context("failed to roll back stale series season metadata")?;
        return Ok(None);
    }
    let result = sqlx::query(
        r#"
        update seasons
        set
            title = coalesce($3, title),
            overview = coalesce($4, overview),
            poster_path = $5,
            backdrop_path = $6,
            updated_at = now()
        where series_id = $1
          and season_number = $2
        "#,
    )
    .bind(params.series_id)
    .bind(params.season_number)
    .bind(&params.title)
    .bind(&params.overview)
    .bind(&params.poster_path)
    .bind(&params.backdrop_path)
    .execute(&mut *tx)
    .await
    .context("failed to update series season metadata")?;
    tx.commit()
        .await
        .context("failed to commit series season metadata update")?;

    Ok(Some(result.rows_affected()))
}

pub async fn update_series_episode_metadata(
    pool: &PgPool,
    params: UpdateSeriesEpisodeMetadataParams,
) -> Result<Option<u64>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start series episode metadata update transaction")?;
    let library_id = sqlx::query_scalar::<_, i64>(
        "select library_id from media_items where id = $1 and media_type = 'series'",
    )
    .bind(params.series_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to resolve series library before episode metadata update")?;
    let Some(library_id) = library_id else {
        tx.rollback()
            .await
            .context("failed to roll back missing series episode metadata update")?;
        return Ok(None);
    };
    lock_library_tmdb_artwork_reference_write(&mut tx, library_id).await?;
    let current = sqlx::query_scalar::<_, i64>(
        r#"
        select id
        from media_items
        where id = $1
          and media_type = 'series'
          and metadata_provider = 'tmdb'
          and metadata_provider_item_id = $2
          and metadata_status = 'matched'
          and updated_at = $3
        for update
        "#,
    )
    .bind(params.series_id)
    .bind(&params.expected_provider_item_id)
    .bind(params.expected_media_item_updated_at)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to lock the current TMDB series binding for episode metadata")?;
    if current.is_none() {
        tx.rollback()
            .await
            .context("failed to roll back stale series episode metadata")?;
        return Ok(None);
    }

    let media_item_result = sqlx::query(
        r#"
        with target as (
            select e.media_item_id
            from episodes e
            join seasons s on s.id = e.season_id
            where s.series_id = $1
              and s.season_number = $2
              and e.episode_number = $3
        )
        update media_items mi
        set
            title = coalesce($4, mi.title),
            overview = coalesce($5, mi.overview),
            poster_path = $6,
            backdrop_path = $7,
            updated_at = now()
        from target
        where mi.id = target.media_item_id
        "#,
    )
    .bind(params.series_id)
    .bind(params.season_number)
    .bind(params.episode_number)
    .bind(&params.title)
    .bind(&params.overview)
    .bind(&params.poster_path)
    .bind(&params.backdrop_path)
    .execute(&mut *tx)
    .await
    .context("failed to update episode media item metadata")?;

    tx.commit()
        .await
        .context("failed to commit series episode metadata update transaction")?;

    Ok(Some(media_item_result.rows_affected()))
}

pub async fn get_season(pool: &PgPool, season_id: i64) -> Result<Option<Season>> {
    let row = sqlx::query(
        r#"
        select
            s.id,
            s.series_id,
            s.season_number,
            s.title,
            s.overview,
            s.poster_path,
            s.backdrop_path,
            s.intro_start_seconds,
            s.intro_end_seconds,
            count(e.media_item_id) as episode_count,
            s.created_at,
            s.updated_at
        from seasons s
        left join episodes e on e.season_id = s.id
        where s.id = $1
        group by
            s.id,
            s.series_id,
            s.season_number,
            s.title,
            s.overview,
            s.poster_path,
            s.backdrop_path,
            s.intro_start_seconds,
            s.intro_end_seconds,
            s.created_at,
            s.updated_at
        "#,
    )
    .bind(season_id)
    .fetch_optional(pool)
    .await
    .context("failed to get season")?;

    Ok(row.map(map_season_row))
}

/// 一次查询读取季信息及所属媒体库，并在 SQL 中应用可见范围。
pub async fn get_season_with_library_visibility(
    pool: &PgPool,
    season_id: i64,
    visible_library_ids: Option<&[i64]>,
) -> Result<VisibilityResult<(Season, Library)>> {
    let row = sqlx::query(
        r#"
        select
            s.id,
            s.series_id,
            s.season_number,
            s.title,
            s.overview,
            s.poster_path,
            s.backdrop_path,
            s.intro_start_seconds,
            s.intro_end_seconds,
            (
                select count(*)
                from episodes e
                where e.season_id = s.id
            ) as episode_count,
            s.created_at,
            s.updated_at,
            l.id as library_id,
            l.name as access_library_name,
            l.description as access_library_description,
            l.metadata_language as access_library_metadata_language,
            l.root_path as access_library_root_path,
            l.created_at as access_library_created_at,
            l.updated_at as access_library_updated_at,
            ($2::bigint[] is null or s.library_id = any($2)) as is_visible
        from seasons s
        join libraries l on l.id = s.library_id
        where s.id = $1
        "#,
    )
    .bind(season_id)
    .bind(visible_library_ids)
    .fetch_optional(pool)
    .await
    .context("failed to get visible season and library")?;

    Ok(match row {
        None => VisibilityResult::Missing,
        Some(row) if !row.get::<bool, _>("is_visible") => VisibilityResult::Forbidden {
            library_id: row.get("library_id"),
        },
        Some(row) => {
            let library = map_access_library_row(&row);
            VisibilityResult::Visible((map_season_row(row), library))
        }
    })
}

pub async fn get_series_episode_outline_cache(
    pool: &PgPool,
    series_media_item_id: i64,
) -> Result<Option<SeriesEpisodeOutlineCacheEntry>> {
    let row = sqlx::query(
        r#"
        select
            cache.series_media_item_id,
            cache.outline_json,
            cache.fetched_at,
            cache.expires_at,
            cache.updated_at
        from series_episode_outline_cache cache
        join media_items item on item.id = cache.series_media_item_id
        where cache.series_media_item_id = $1
          and cache.metadata_provider = 'tmdb'
          and cache.provider_item_id = item.metadata_provider_item_id
          and cache.source_media_item_updated_at = item.updated_at
          and item.media_type = 'series'
          and item.metadata_provider = 'tmdb'
          and item.metadata_status = 'matched'
        "#,
    )
    .bind(series_media_item_id)
    .fetch_optional(pool)
    .await
    .context("failed to get series episode outline cache")?;

    Ok(row.map(map_series_episode_outline_cache_entry_row))
}

pub async fn upsert_series_episode_outline_cache(
    pool: &PgPool,
    params: UpsertSeriesEpisodeOutlineCacheParams,
) -> Result<Option<SeriesEpisodeOutlineCacheEntry>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start series episode outline cache transaction")?;
    let library_id = sqlx::query_scalar::<_, i64>(
        "select library_id from media_items where id = $1 and media_type = 'series'",
    )
    .bind(params.series_media_item_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to resolve series library before outline cache update")?
    .context("series outline cache target is missing")?;
    lock_library_tmdb_artwork_reference_write(&mut tx, library_id).await?;
    let current = sqlx::query_scalar::<_, i64>(
        r#"
        select id
        from media_items
        where id = $1
          and library_id = $2
          and media_type = 'series'
          and metadata_provider = 'tmdb'
          and metadata_provider_item_id = $3
          and metadata_status = 'matched'
          and updated_at = $4
        for update
        "#,
    )
    .bind(params.series_media_item_id)
    .bind(library_id)
    .bind(&params.expected_provider_item_id)
    .bind(params.expected_media_item_updated_at)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to lock the current TMDB series binding for outline cache")?;
    if current.is_none() {
        tx.rollback()
            .await
            .context("failed to roll back stale series outline cache update")?;
        return Ok(None);
    }
    let row = sqlx::query(
        r#"
        insert into series_episode_outline_cache (
            series_media_item_id,
            library_id,
            metadata_provider,
            provider_item_id,
            source_media_item_updated_at,
            outline_json,
            fetched_at,
            expires_at
        )
        select
            media_item.id,
            media_item.library_id,
            'tmdb',
            $2,
            $3,
            $4,
            $5,
            $6
        from media_items media_item
        where media_item.id = $1
          and media_item.media_type = 'series'
        on conflict (series_media_item_id)
        do update set
            library_id = excluded.library_id,
            metadata_provider = excluded.metadata_provider,
            provider_item_id = excluded.provider_item_id,
            source_media_item_updated_at = excluded.source_media_item_updated_at,
            outline_json = excluded.outline_json,
            fetched_at = excluded.fetched_at,
            expires_at = excluded.expires_at,
            updated_at = now()
        returning
            series_media_item_id,
            outline_json,
            fetched_at,
            expires_at,
            updated_at
        "#,
    )
    .bind(params.series_media_item_id)
    .bind(&params.expected_provider_item_id)
    .bind(params.expected_media_item_updated_at)
    .bind(params.outline_json)
    .bind(params.fetched_at)
    .bind(params.expires_at)
    .fetch_one(&mut *tx)
    .await
    .context("failed to upsert series episode outline cache")?;
    tx.commit()
        .await
        .context("failed to commit series episode outline cache")?;

    Ok(Some(map_series_episode_outline_cache_entry_row(row)))
}

pub async fn delete_series_episode_outline_cache(
    pool: &PgPool,
    series_media_item_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        delete from series_episode_outline_cache
        where series_media_item_id = $1
        "#,
    )
    .bind(series_media_item_id)
    .execute(pool)
    .await
    .context("failed to delete series episode outline cache")?;

    Ok(())
}

/// 统计某个媒体库下当前已有多少条媒体内容。
pub async fn count_media_items_for_library(pool: &PgPool, library_id: i64) -> Result<i64> {
    let row = sqlx::query(
        r#"
        select count(*) as media_count
        from media_items
        where library_id = $1
          and media_type in ('movie', 'series')
        "#,
    )
    .bind(library_id)
    .fetch_one(pool)
    .await
    .context("failed to count media items for library")?;

    Ok(row.get("media_count"))
}

pub async fn get_library_media_type_counts(
    pool: &PgPool,
    library_id: i64,
) -> Result<LibraryMediaTypeCounts> {
    let row = sqlx::query(
        r#"
        select
            count(*) filter (where media_type = 'movie') as movie_count,
            count(*) filter (where media_type = 'series') as series_count
        from media_items
        where library_id = $1
          and media_type in ('movie', 'series')
        "#,
    )
    .bind(library_id)
    .fetch_one(pool)
    .await
    .context("failed to count media items by type for library")?;

    Ok(LibraryMediaTypeCounts {
        movie_count: row.get("movie_count"),
        series_count: row.get("series_count"),
    })
}

/// 列出某个媒体库下当前所有已入库的视频文件路径。
pub async fn list_library_media_file_paths(pool: &PgPool, library_id: i64) -> Result<Vec<String>> {
    let rows = sqlx::query(
        r#"
        select mf.file_path
        from media_files mf
        where mf.library_id = $1
        order by mf.file_path
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .context("failed to list library media file paths")?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("file_path"))
        .collect())
}

pub async fn list_library_media_file_memberships(
    pool: &PgPool,
    library_id: i64,
) -> Result<Vec<super::LibraryMediaFileMembership>> {
    let rows = sqlx::query(
        r#"
        select
            mf.media_item_id,
            coalesce(season.series_id, mf.media_item_id) as logical_metadata_owner_id,
            mf.file_path
        from media_files mf
        left join episodes episode on episode.media_item_id = mf.media_item_id
        left join seasons season on season.id = episode.season_id
        where mf.library_id = $1
        order by mf.file_path
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .context("failed to list library media file memberships")?;

    Ok(rows
        .into_iter()
        .map(|row| super::LibraryMediaFileMembership {
            media_item_id: row.get("media_item_id"),
            logical_metadata_owner_id: row.get("logical_metadata_owner_id"),
            file_path: row.get("file_path"),
        })
        .collect())
}

pub async fn list_existing_media_metadata_for_file_paths(
    pool: &PgPool,
    library_id: i64,
    file_paths: &[String],
) -> Result<Vec<ExistingMediaMetadataSummary>> {
    if file_paths.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        select
            mi.id as media_item_id,
            coalesce(s.series_id, mi.id) as logical_metadata_owner_id,
            mf.id as media_file_id,
            mf.file_path,
            mi.media_type,
            mi.metadata_provider,
            mi.metadata_provider_item_id,
            mi.metadata_status,
            mi.metadata_failure_reason,
            mi.remote_media_type,
            exists (
                select 1
                from media_local_metadata_sources local_source
                where local_source.media_item_id = mi.id
            ) as has_local_nfo,
            (
                select selected_source.source_path
                from media_local_metadata_sources selected_source
                where selected_source.media_item_id = mi.id
                  and selected_source.is_selected
                order by selected_source.id
                limit 1
            ) as local_nfo_source_path,
            (
                select selected_source.payload
                from media_local_metadata_sources selected_source
                where selected_source.media_item_id = mi.id
                  and selected_source.is_selected
                order by selected_source.id
                limit 1
            ) as local_nfo_payload,
            (
                select revalidation.remote_snapshot
                from tmdb_metadata_revalidations revalidation
                where revalidation.media_item_id = mi.id
                limit 1
            ) as tmdb_remote_snapshot,
            mi.title,
            mi.source_title,
            mi.original_title,
            mi.sort_title,
            mi.year,
            mi.tagline,
            mi.premiere_date,
            mi.content_rating,
            mi.country,
            mi.genres,
            mi.studio,
            mi.overview,
            mi.poster_path,
            mi.backdrop_path,
            mi.logo_path,
            mf.scan_hash,
            mf.source_kind,
            mf.stream_reference_hash,
            mf.container,
            mf.file_size,
            mf.duration_seconds,
            mf.video_title,
            mf.video_codec,
            mf.video_profile,
            mf.video_level,
            mf.audio_codec,
            mf.width,
            mf.height,
            mf.bitrate,
            mf.video_bitrate,
            mf.video_frame_rate,
            mf.video_aspect_ratio,
            mf.video_scan_type,
            mf.video_color_primaries,
            mf.video_color_space,
            mf.video_color_transfer,
            mf.video_bit_depth,
            mf.video_pixel_format,
            mf.video_reference_frames,
            mf.technical_tags,
            mf.local_analysis_version,
            series_mi.title as series_title,
            series_mi.metadata_provider as series_metadata_provider,
            series_mi.metadata_provider_item_id as series_metadata_provider_item_id,
            exists (
                select 1
                from media_local_metadata_sources series_local_source
                where series_local_source.media_item_id = series_mi.id
            ) as series_has_local_nfo,
            (
                select selected_series_source.source_path
                from media_local_metadata_sources selected_series_source
                where selected_series_source.media_item_id = series_mi.id
                  and selected_series_source.is_selected
                order by selected_series_source.id
                limit 1
            ) as series_local_nfo_source_path,
            (
                select selected_series_source.payload
                from media_local_metadata_sources selected_series_source
                where selected_series_source.media_item_id = series_mi.id
                  and selected_series_source.is_selected
                order by selected_series_source.id
                limit 1
            ) as series_local_nfo_payload,
            (
                select series_revalidation.remote_snapshot
                from tmdb_metadata_revalidations series_revalidation
                where series_revalidation.media_item_id = series_mi.id
                limit 1
            ) as series_tmdb_remote_snapshot,
            series_mi.source_title as series_source_title,
            series_mi.original_title as series_original_title,
            series_mi.sort_title as series_sort_title,
            series_mi.year as series_year,
            series_mi.tagline as series_tagline,
            series_mi.premiere_date as series_premiere_date,
            series_mi.content_rating as series_content_rating,
            series_mi.country as series_country,
            series_mi.genres as series_genres,
            series_mi.studio as series_studio,
            series_mi.overview as series_overview,
            series_mi.poster_path as series_poster_path,
            series_mi.backdrop_path as series_backdrop_path,
            series_mi.logo_path as series_logo_path,
            s.title as season_title,
            s.season_number,
            s.overview as season_overview,
            s.poster_path as season_poster_path,
            s.backdrop_path as season_backdrop_path,
            mi.title as episode_title,
            e.episode_number
        from media_files mf
        join media_items mi on mi.id = mf.media_item_id
        left join episodes e on e.media_item_id = mi.id
        left join seasons s on s.id = e.season_id
        left join media_items series_mi on series_mi.id = s.series_id
        where mf.library_id = $1
          and mf.file_path = any($2)
        order by mf.file_path asc
        "#,
    )
    .bind(library_id)
    .bind(file_paths)
    .fetch_all(pool)
    .await
    .context("failed to list existing media metadata for file paths")?;

    Ok(rows
        .into_iter()
        .map(map_existing_media_metadata_summary_row)
        .collect())
}

fn map_media_item_row(row: PgRow) -> MediaItem {
    MediaItem {
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
        tagline: row.get("tagline"),
        premiere_date: row.get("premiere_date"),
        content_rating: row.get("content_rating"),
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
    }
}

fn map_recently_added_library_row(row: &PgRow) -> Library {
    Library {
        id: row.get("library_id"),
        name: row.get("library_name"),
        description: row.get("library_description"),
        metadata_language: row.get("library_metadata_language"),
        root_path: row.get("library_root_path"),
        created_at: row.get("library_created_at"),
        updated_at: row.get("library_updated_at"),
    }
}

fn map_access_library_row(row: &PgRow) -> Library {
    Library {
        id: row.get("library_id"),
        name: row.get("access_library_name"),
        description: row.get("access_library_description"),
        metadata_language: row.get("access_library_metadata_language"),
        root_path: row.get("access_library_root_path"),
        created_at: row.get("access_library_created_at"),
        updated_at: row.get("access_library_updated_at"),
    }
}

fn map_recently_added_media_item_row(row: &PgRow) -> MediaItem {
    MediaItem {
        id: row.get("media_item_id"),
        library_id: row.get("media_item_library_id"),
        media_type: row.get("media_item_media_type"),
        title: row.get("media_item_title"),
        source_title: row.get("media_item_source_title"),
        original_title: row.get("media_item_original_title"),
        sort_title: row.get("media_item_sort_title"),
        metadata_provider: row.get("media_item_metadata_provider"),
        metadata_provider_item_id: row.get("media_item_metadata_provider_item_id"),
        metadata_status: row.get("media_item_metadata_status"),
        metadata_failure_reason: row.get("media_item_metadata_failure_reason"),
        remote_media_type: row.get("media_item_remote_media_type"),
        year: row.get("media_item_year"),
        tagline: row.get("media_item_tagline"),
        premiere_date: row.get("media_item_premiere_date"),
        content_rating: row.get("media_item_content_rating"),
        ratings: Vec::new(),
        country: row.get("media_item_country"),
        genres: row.get("media_item_genres"),
        studio: row.get("media_item_studio"),
        overview: row.get("media_item_overview"),
        poster_path: row.get("media_item_poster_path"),
        backdrop_path: row.get("media_item_backdrop_path"),
        logo_path: row.get("media_item_logo_path"),
        created_at: row.get("media_item_created_at"),
        updated_at: row.get("media_item_updated_at"),
    }
}

async fn attach_media_item_ratings(pool: &PgPool, items: &mut [MediaItem]) -> Result<()> {
    let media_item_ids = items.iter().map(|item| item.id).collect::<Vec<_>>();
    let mut ratings_by_media_item = list_media_item_ratings(pool, &media_item_ids).await?;
    for item in items {
        item.ratings = ratings_by_media_item.remove(&item.id).unwrap_or_default();
    }
    Ok(())
}

fn map_media_item_playback_header_row(row: PgRow) -> MediaItemPlaybackHeader {
    MediaItemPlaybackHeader {
        media_item_id: row.get("media_item_id"),
        library_id: row.get("library_id"),
        media_type: row.get("media_type"),
        series_media_item_id: row.get("series_media_item_id"),
        title: row.get("title"),
        original_title: row.get("original_title"),
        year: row.get("year"),
        logo_path: row.get("logo_path"),
        logo_updated_at: row.get("logo_updated_at"),
        season_id: row.get("season_id"),
        season_number: row.get("season_number"),
        episode_number: row.get("episode_number"),
        episode_title: row.get("episode_title"),
        season_intro_start_seconds: row.get("season_intro_start_seconds"),
        season_intro_end_seconds: row.get("season_intro_end_seconds"),
        episode_intro_start_seconds: row.get("episode_intro_start_seconds"),
        episode_intro_end_seconds: row.get("episode_intro_end_seconds"),
    }
}

fn map_season_row(row: PgRow) -> Season {
    Season {
        id: row.get("id"),
        series_id: row.get("series_id"),
        season_number: row.get("season_number"),
        title: row.get("title"),
        overview: row.get("overview"),
        poster_path: row.get("poster_path"),
        backdrop_path: row.get("backdrop_path"),
        intro_start_seconds: row.get("intro_start_seconds"),
        intro_end_seconds: row.get("intro_end_seconds"),
        episode_count: row.get("episode_count"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_episode_row(row: PgRow) -> Episode {
    Episode {
        media_item_id: row.get("media_item_id"),
        series_id: row.get("series_id"),
        season_id: row.get("season_id"),
        episode_number: row.get("episode_number"),
        title: row.get("title"),
        overview: row.get("overview"),
        poster_path: row.get("poster_path"),
        backdrop_path: row.get("backdrop_path"),
        intro_start_seconds: row.get("intro_start_seconds"),
        intro_end_seconds: row.get("intro_end_seconds"),
    }
}

fn map_global_search_result_row(row: PgRow) -> GlobalSearchResult {
    GlobalSearchResult {
        kind: row.get("kind"),
        library_id: row.get("library_id"),
        library_name: row.get("library_name"),
        media_item_id: row.get("media_item_id"),
        series_media_item_id: row.get("series_media_item_id"),
        media_type: row.get("media_type"),
        title: row.get("title"),
        subtitle: row.get("subtitle"),
        year: row.get("year"),
        overview: row.get("overview"),
        poster_path: row.get("poster_path"),
        backdrop_path: row.get("backdrop_path"),
        season_number: row.get("season_number"),
        episode_number: row.get("episode_number"),
        ratings: Vec::new(),
        updated_at: row.get("updated_at"),
    }
}

fn map_media_file_row(row: PgRow) -> MediaFile {
    let source_kind = row
        .get::<String, _>("source_kind")
        .parse::<MediaSourceKind>()
        .expect("media_files.source_kind must satisfy its database constraint");
    MediaFile {
        id: row.get("id"),
        library_id: row.get("library_id"),
        media_item_id: row.get("media_item_id"),
        file_path: row.get("file_path"),
        source_kind,
        stream_reference_hash: row.get("stream_reference_hash"),
        container: row.get("container"),
        file_size: row.get("file_size"),
        duration_seconds: row.get("duration_seconds"),
        video_title: row.get("video_title"),
        video_codec: row.get("video_codec"),
        video_profile: row.get("video_profile"),
        video_level: row.get("video_level"),
        audio_codec: row.get("audio_codec"),
        width: row.get("width"),
        height: row.get("height"),
        bitrate: row.get("bitrate"),
        video_bitrate: row.get("video_bitrate"),
        video_frame_rate: row.get("video_frame_rate"),
        video_aspect_ratio: row.get("video_aspect_ratio"),
        video_scan_type: row.get("video_scan_type"),
        video_color_primaries: row.get("video_color_primaries"),
        video_color_space: row.get("video_color_space"),
        video_color_transfer: row.get("video_color_transfer"),
        video_bit_depth: row.get("video_bit_depth"),
        video_pixel_format: row.get("video_pixel_format"),
        video_reference_frames: row.get("video_reference_frames"),
        technical_tags: row.get("technical_tags"),
        scan_hash: row.get("scan_hash"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_existing_media_metadata_summary_row(row: PgRow) -> ExistingMediaMetadataSummary {
    let source_kind = row
        .get::<String, _>("source_kind")
        .parse::<MediaSourceKind>()
        .expect("media_files.source_kind must satisfy its database constraint");
    ExistingMediaMetadataSummary {
        media_item_id: row.get("media_item_id"),
        logical_metadata_owner_id: row.get("logical_metadata_owner_id"),
        media_file_id: row.get("media_file_id"),
        file_path: row.get("file_path"),
        media_type: row.get("media_type"),
        metadata_provider: row.get("metadata_provider"),
        metadata_provider_item_id: row.get("metadata_provider_item_id"),
        metadata_status: row.get("metadata_status"),
        metadata_failure_reason: row.get("metadata_failure_reason"),
        remote_media_type: row.get("remote_media_type"),
        has_local_nfo: row.get("has_local_nfo"),
        local_nfo_source_path: row.get("local_nfo_source_path"),
        local_nfo_payload: row.get("local_nfo_payload"),
        tmdb_remote_snapshot: row.get("tmdb_remote_snapshot"),
        title: row.get("title"),
        source_title: row.get("source_title"),
        original_title: row.get("original_title"),
        sort_title: row.get("sort_title"),
        year: row.get("year"),
        tagline: row.get("tagline"),
        premiere_date: row.get("premiere_date"),
        content_rating: row.get("content_rating"),
        country: row.get("country"),
        genres: row.get("genres"),
        studio: row.get("studio"),
        overview: row.get("overview"),
        poster_path: row.get("poster_path"),
        backdrop_path: row.get("backdrop_path"),
        logo_path: row.get("logo_path"),
        scan_hash: row.get("scan_hash"),
        source_kind,
        stream_reference_hash: row.get("stream_reference_hash"),
        container: row.get("container"),
        file_size: row.get("file_size"),
        duration_seconds: row.get("duration_seconds"),
        video_title: row.get("video_title"),
        video_codec: row.get("video_codec"),
        video_profile: row.get("video_profile"),
        video_level: row.get("video_level"),
        audio_codec: row.get("audio_codec"),
        width: row.get("width"),
        height: row.get("height"),
        bitrate: row.get("bitrate"),
        video_bitrate: row.get("video_bitrate"),
        video_frame_rate: row.get("video_frame_rate"),
        video_aspect_ratio: row.get("video_aspect_ratio"),
        video_scan_type: row.get("video_scan_type"),
        video_color_primaries: row.get("video_color_primaries"),
        video_color_space: row.get("video_color_space"),
        video_color_transfer: row.get("video_color_transfer"),
        video_bit_depth: row.get("video_bit_depth"),
        video_pixel_format: row.get("video_pixel_format"),
        video_reference_frames: row.get("video_reference_frames"),
        technical_tags: row.get("technical_tags"),
        local_analysis_version: row.get("local_analysis_version"),
        audio_tracks: Vec::new(),
        subtitle_tracks: Vec::new(),
        series_title: row.get("series_title"),
        series_metadata_provider: row.get("series_metadata_provider"),
        series_metadata_provider_item_id: row.get("series_metadata_provider_item_id"),
        series_has_local_nfo: row.get("series_has_local_nfo"),
        series_local_nfo_source_path: row.get("series_local_nfo_source_path"),
        series_local_nfo_payload: row.get("series_local_nfo_payload"),
        series_tmdb_remote_snapshot: row.get("series_tmdb_remote_snapshot"),
        series_source_title: row.get("series_source_title"),
        series_original_title: row.get("series_original_title"),
        series_sort_title: row.get("series_sort_title"),
        series_year: row.get("series_year"),
        series_tagline: row.get("series_tagline"),
        series_premiere_date: row.get("series_premiere_date"),
        series_content_rating: row.get("series_content_rating"),
        series_country: row.get("series_country"),
        series_genres: row.get("series_genres"),
        series_studio: row.get("series_studio"),
        series_overview: row.get("series_overview"),
        series_poster_path: row.get("series_poster_path"),
        series_backdrop_path: row.get("series_backdrop_path"),
        series_logo_path: row.get("series_logo_path"),
        season_title: row.get("season_title"),
        season_number: row.get("season_number"),
        season_overview: row.get("season_overview"),
        season_poster_path: row.get("season_poster_path"),
        season_backdrop_path: row.get("season_backdrop_path"),
        episode_title: row.get("episode_title"),
        episode_number: row.get("episode_number"),
    }
}

fn map_subtitle_file_row(row: PgRow) -> SubtitleFile {
    SubtitleFile {
        id: row.get("id"),
        media_file_id: row.get("media_file_id"),
        source_kind: row.get("source_kind"),
        file_path: row.get("file_path"),
        stream_index: row.get("stream_index"),
        language: row.get("language"),
        subtitle_format: row.get("subtitle_format"),
        label: row.get("label"),
        is_default: row.get("is_default"),
        is_forced: row.get("is_forced"),
        is_hearing_impaired: row.get("is_hearing_impaired"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_audio_track_row(row: PgRow) -> AudioTrack {
    AudioTrack {
        id: row.get("id"),
        media_file_id: row.get("media_file_id"),
        stream_index: row.get("stream_index"),
        language: row.get("language"),
        audio_codec: row.get("audio_codec"),
        label: row.get("label"),
        channel_layout: row.get("channel_layout"),
        channels: row.get("channels"),
        bitrate: row.get("bitrate"),
        sample_rate: row.get("sample_rate"),
        is_default: row.get("is_default"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_series_episode_outline_cache_entry_row(row: PgRow) -> SeriesEpisodeOutlineCacheEntry {
    SeriesEpisodeOutlineCacheEntry {
        series_media_item_id: row.get("series_media_item_id"),
        outline_json: row.get("outline_json"),
        fetched_at: row.get::<OffsetDateTime, _>("fetched_at"),
        expires_at: row.get::<OffsetDateTime, _>("expires_at"),
        updated_at: row.get::<OffsetDateTime, _>("updated_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        get_series_episode_outline_cache, list_media_item_metadata_refresh_source_files,
        update_media_item_metadata, upsert_series_episode_outline_cache,
    };
    use crate::{UpdateMediaItemMetadataParams, UpsertSeriesEpisodeOutlineCacheParams};
    use time::{Duration, OffsetDateTime};

    async fn seed_bound_series_episode(
        pool: &sqlx::PgPool,
        episode_provider: &str,
        episode_provider_item_id: &str,
    ) -> (i64, i64, OffsetDateTime) {
        let library_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into libraries (name, root_path)
            values ('Manual Match', '/manual-match')
            returning id
            "#,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let (series_id, series_updated_at) = sqlx::query_as::<_, (i64, OffsetDateTime)>(
            r#"
                insert into media_items (
                    library_id,
                    media_type,
                    title,
                    source_title,
                    metadata_provider,
                    metadata_provider_item_id,
                    metadata_status,
                    remote_media_type
                )
                values (
                    $1,
                    'series',
                    'Old Remote Series',
                    'Local Series',
                    'tmdb',
                    'old-series',
                    'matched',
                    'series'
                )
                returning id, updated_at
                "#,
        )
        .bind(library_id)
        .fetch_one(pool)
        .await
        .unwrap();
        let season_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into seasons (library_id, series_id, season_number, title)
            values ($1, $2, 1, 'Local Season')
            returning id
            "#,
        )
        .bind(library_id)
        .bind(series_id)
        .fetch_one(pool)
        .await
        .unwrap();
        let episode_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (
                library_id,
                media_type,
                title,
                source_title,
                metadata_provider,
                metadata_provider_item_id,
                metadata_status,
                remote_media_type,
                overview
            )
            values (
                $1,
                'episode',
                'Local Episode',
                'Local Episode Source',
                $2,
                $3,
                'matched',
                'series',
                'Local episode overview'
            )
            returning id
            "#,
        )
        .bind(library_id)
        .bind(episode_provider)
        .bind(episode_provider_item_id)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into episodes (media_item_id, library_id, season_id, episode_number)
            values ($1, $2, $3, 1)
            "#,
        )
        .bind(episode_id)
        .bind(library_id)
        .bind(season_id)
        .execute(pool)
        .await
        .unwrap();
        let local_metadata_source_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_local_metadata_sources (
                library_id, media_item_id, source_path, document_type, is_selected, payload
            )
            values ($1, $2, '/manual-match/episode.nfo', 'episodedetails', true, '{}')
            returning id
            "#,
        )
        .bind(library_id)
        .bind(episode_id)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_item_external_ids (media_item_id, provider, external_id)
            values
                ($1, 'tmdb', 'old-episode'),
                ($1, 'imdb', 'tt-old-episode')
            "#,
        )
        .bind(episode_id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_item_ratings (
                media_item_id,
                source,
                kind,
                score,
                scale,
                retrieved_via,
                local_metadata_source_id,
                fetched_at
            )
            values
                ($1, 'tmdb', 'audience', 8, 10, 'tmdb', null, now()),
                ($1, 'nfo', 'audience', 4, 5, 'nfo', $2, now())
            "#,
        )
        .bind(episode_id)
        .bind(local_metadata_source_id)
        .execute(pool)
        .await
        .unwrap();

        (series_id, episode_id, series_updated_at)
    }

    fn replacement_params(expected_updated_at: OffsetDateTime) -> UpdateMediaItemMetadataParams {
        UpdateMediaItemMetadataParams {
            expected_updated_at,
            title: "New Remote Series".to_string(),
            source_title: "Local Series".to_string(),
            original_title: None,
            sort_title: None,
            metadata_provider: Some("tmdb".to_string()),
            metadata_provider_item_id: Some("new-series".to_string()),
            metadata_status: "matched".to_string(),
            metadata_failure_reason: None,
            replace_remote_data: true,
            tmdb_remote_snapshot_json: None,
            tmdb_remote_snapshot_renews_retention: false,
            remote_media_type: Some("series".to_string()),
            year: None,
            tagline: None,
            premiere_date: None,
            content_rating: None,
            seasons: vec![super::super::UpdateSeasonMetadataParams {
                season_number: 1,
                title: "Remote Season 1".to_string(),
                overview: Some("Remote season overview".to_string()),
                poster_path: Some("/cache/season-1-poster.jpg".to_string()),
                backdrop_path: None,
            }],
            local_nfos: Vec::new(),
            removed_local_nfo_source_paths: Vec::new(),
            external_ids: Vec::new(),
            ratings: Vec::new(),
            country: None,
            genres: None,
            studio: None,
            overview: Some("New series overview".to_string()),
            poster_path: None,
            backdrop_path: None,
            logo_path: None,
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn metadata_refresh_source_files_for_series_include_only_descendant_carriers(
        pool: sqlx::PgPool,
    ) {
        let library_id = sqlx::query_scalar::<_, i64>(
            "insert into libraries (name, root_path) values ('Series carriers', '/series-carriers') returning id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let series_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, media_type, title, source_title)
            values ($1, 'series', 'Target Series', 'Target Series')
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let season_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into seasons (library_id, series_id, season_number, title)
            values ($1, $2, 1, 'Season 1')
            returning id
            "#,
        )
        .bind(library_id)
        .bind(series_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let episode_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, media_type, title, source_title)
            values ($1, 'episode', 'Episode 1', 'Target.Series.S01E01')
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into episodes (media_item_id, library_id, season_id, episode_number)
            values ($1, $2, $3, 1)
            "#,
        )
        .bind(episode_id)
        .bind(library_id)
        .bind(season_id)
        .execute(&pool)
        .await
        .unwrap();
        for path in [
            "/series-carriers/Target/Target.S01E01.1080p.mkv",
            "/series-carriers/Target/Target.S01E01.2160p.mkv",
        ] {
            sqlx::query(
                r#"
                insert into media_files (library_id, media_item_id, file_path, file_size)
                values ($1, $2, $3, 1)
                "#,
            )
            .bind(library_id)
            .bind(episode_id)
            .bind(path)
            .execute(&pool)
            .await
            .unwrap();
        }

        let unrelated_movie_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, media_type, title, source_title)
            values ($1, 'movie', 'Unrelated', 'Unrelated')
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_files (library_id, media_item_id, file_path, file_size)
            values ($1, $2, '/series-carriers/Unrelated.mkv', 1)
            "#,
        )
        .bind(library_id)
        .bind(unrelated_movie_id)
        .execute(&pool)
        .await
        .unwrap();

        let source_files = list_media_item_metadata_refresh_source_files(&pool, series_id)
            .await
            .unwrap();
        assert_eq!(
            source_files
                .iter()
                .map(|file| file.file_path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "/series-carriers/Target/Target.S01E01.1080p.mkv",
                "/series-carriers/Target/Target.S01E01.2160p.mkv",
            ]
        );
        assert!(source_files
            .iter()
            .all(|file| file.media_item_id == episode_id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn manual_series_binding_change_clears_old_tmdb_owned_episode_identity(
        pool: sqlx::PgPool,
    ) {
        let (series_id, episode_id, expected_updated_at) =
            seed_bound_series_episode(&pool, "tmdb", "old-series").await;

        let updated =
            update_media_item_metadata(&pool, series_id, replacement_params(expected_updated_at))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            updated.metadata_provider_item_id.as_deref(),
            Some("new-series")
        );

        let episode = sqlx::query_as::<
            _,
            (
                Option<String>,
                Option<String>,
                String,
                String,
                Option<String>,
                Option<String>,
            ),
        >(
            r#"
            select
                metadata_provider,
                metadata_provider_item_id,
                metadata_status,
                title,
                overview,
                remote_media_type
            from media_items
            where id = $1
            "#,
        )
        .bind(episode_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(episode.0, None);
        assert_eq!(episode.1, None);
        assert_ne!(episode.1.as_deref(), Some("new-series"));
        assert_eq!(episode.2, "matched");
        assert_eq!(episode.3, "Local Episode");
        assert_eq!(episode.4.as_deref(), Some("Local episode overview"));
        assert_eq!(episode.5.as_deref(), Some("series"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from media_item_external_ids where media_item_id = $1",
            )
            .bind(episode_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, Vec<String>>(
                r#"
                select coalesce(array_agg(source order by source), array[]::varchar[])
                from media_item_ratings
                where media_item_id = $1
                "#,
            )
            .bind(episode_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            vec!["nfo".to_string()]
        );
        let season = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
            "select title, overview, poster_path from seasons where series_id = $1 and season_number = 1",
        )
        .bind(series_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(season.0, "Remote Season 1");
        assert_eq!(season.1.as_deref(), Some("Remote season overview"));
        assert_eq!(season.2.as_deref(), Some("/cache/season-1-poster.jpg"));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn manual_series_binding_change_preserves_nfo_owned_episode(pool: sqlx::PgPool) {
        let (series_id, episode_id, expected_updated_at) =
            seed_bound_series_episode(&pool, "nfo", "nfo-episode").await;

        update_media_item_metadata(&pool, series_id, replacement_params(expected_updated_at))
            .await
            .unwrap()
            .unwrap();

        let episode =
            sqlx::query_as::<_, (Option<String>, Option<String>, String, Option<String>)>(
                r#"
            select metadata_provider, metadata_provider_item_id, title, overview
            from media_items
            where id = $1
            "#,
            )
            .bind(episode_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(episode.0.as_deref(), Some("nfo"));
        assert_eq!(episode.1.as_deref(), Some("nfo-episode"));
        assert_ne!(episode.1.as_deref(), Some("new-series"));
        assert_eq!(episode.2, "Local Episode");
        assert_eq!(episode.3.as_deref(), Some("Local episode overview"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from media_item_external_ids where media_item_id = $1",
            )
            .bind(episode_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from media_item_ratings where media_item_id = $1",
            )
            .bind(episode_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            2
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn outline_cache_is_hidden_and_stale_write_is_rejected_after_binding_change(
        pool: sqlx::PgPool,
    ) {
        let library_id = sqlx::query_scalar::<_, i64>(
            "insert into libraries (name, root_path) values ('Outline', '/outline') returning id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let (series_id, observed_updated_at) = sqlx::query_as::<_, (i64, OffsetDateTime)>(
            r#"
            insert into media_items (
                library_id,
                media_type,
                title,
                source_title,
                metadata_provider,
                metadata_provider_item_id,
                metadata_status
            )
            values ($1, 'series', 'Series', 'Series', 'tmdb', '42', 'matched')
            returning id, updated_at
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let fetched_at = OffsetDateTime::now_utc();
        let inserted = upsert_series_episode_outline_cache(
            &pool,
            UpsertSeriesEpisodeOutlineCacheParams {
                series_media_item_id: series_id,
                expected_provider_item_id: "42".to_string(),
                expected_media_item_updated_at: observed_updated_at,
                outline_json: r#"{"seasons":[]}"#.to_string(),
                fetched_at,
                expires_at: fetched_at + Duration::days(1),
            },
        )
        .await
        .unwrap();
        assert!(inserted.is_some());
        assert!(get_series_episode_outline_cache(&pool, series_id)
            .await
            .unwrap()
            .is_some());

        sqlx::query(
            r#"
            update media_items
            set metadata_provider_item_id = '84',
                updated_at = clock_timestamp()
            where id = $1
            "#,
        )
        .bind(series_id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(get_series_episode_outline_cache(&pool, series_id)
            .await
            .unwrap()
            .is_none());

        let stale_write = upsert_series_episode_outline_cache(
            &pool,
            UpsertSeriesEpisodeOutlineCacheParams {
                series_media_item_id: series_id,
                expected_provider_item_id: "42".to_string(),
                expected_media_item_updated_at: observed_updated_at,
                outline_json: r#"{"seasons":[{"season_number":1}]}"#.to_string(),
                fetched_at,
                expires_at: fetched_at + Duration::days(1),
            },
        )
        .await
        .unwrap();
        assert!(stale_write.is_none());
    }
}
