use super::{
    CreateAudioTrackParams, CreateSubtitleTrackParams, ExistingMediaMetadataSummary,
    LibraryMediaTypeCounts, ListMediaItemsForLibraryParams, ListMediaItemsForLibraryResult,
    MediaItemPlaybackHeader, SeriesEpisodeOutlineCacheEntry, UpdateMediaFileMetadataParams,
    UpdateMediaItemMetadataParams, UpsertSeriesEpisodeOutlineCacheParams,
};
use anyhow::{Context, Result};
use mova_domain::{AudioTrack, Episode, MediaFile, MediaItem, Season, SubtitleFile};
use sqlx::{
    postgres::{PgPool, PgRow},
    Row,
};
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
            year,
            imdb_rating,
            country,
            genres,
            studio,
            overview,
            poster_path,
            backdrop_path,
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

    Ok(ListMediaItemsForLibraryResult {
        items: rows.into_iter().map(map_media_item_row).collect(),
        total: total_row.get("total"),
    })
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
            year,
            imdb_rating,
            country,
            genres,
            studio,
            overview,
            poster_path,
            backdrop_path,
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

    Ok(row.map(map_media_item_row))
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
            e.series_id as series_media_item_id,
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
            e.season_id,
            s.season_number,
            e.episode_number,
            case
                when mi.media_type = 'episode' then coalesce(nullif(e.title, ''), nullif(mi.title, ''))
                else null
            end as episode_title,
            s.intro_start_seconds as season_intro_start_seconds,
            s.intro_end_seconds as season_intro_end_seconds,
            e.intro_start_seconds as episode_intro_start_seconds,
            e.intro_end_seconds as episode_intro_end_seconds
        from media_items mi
        left join episodes e on e.media_item_id = mi.id
        left join seasons s on s.id = e.season_id
        left join media_items series_mi on series_mi.id = e.series_id
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
            year = $8,
            imdb_rating = $9,
            country = $10,
            genres = $11,
            studio = $12,
            overview = $13,
            poster_path = $14,
            backdrop_path = $15,
            updated_at = now()
        where id = $1
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
            year,
            imdb_rating,
            country,
            genres,
            studio,
            overview,
            poster_path,
            backdrop_path,
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
    .bind(params.year)
    .bind(&params.imdb_rating)
    .bind(&params.country)
    .bind(&params.genres)
    .bind(&params.studio)
    .bind(&params.overview)
    .bind(&params.poster_path)
    .bind(&params.backdrop_path)
    .fetch_optional(pool)
    .await
    .context("failed to update media item metadata")?;

    Ok(row.map(map_media_item_row))
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
            container = $3,
            file_size = $4,
            duration_seconds = $5,
            video_title = $6,
            video_codec = $7,
            video_profile = $8,
            video_level = $9,
            audio_codec = $10,
            width = $11,
            height = $12,
            bitrate = $13,
            video_bitrate = $14,
            video_frame_rate = $15,
            video_aspect_ratio = $16,
            video_scan_type = $17,
            video_color_primaries = $18,
            video_color_space = $19,
            video_color_transfer = $20,
            video_bit_depth = $21,
            video_pixel_format = $22,
            video_reference_frames = $23,
            updated_at = now()
        where id = $1
        returning
            id,
            media_item_id,
            library_id,
            file_path,
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
            scan_hash,
            created_at,
            updated_at
        "#,
    )
    .bind(media_file_id)
    .bind(&params.file_path)
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
            count(e.id) as episode_count,
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
            e.id,
            e.media_item_id,
            e.series_id,
            e.season_id,
            e.episode_number,
            e.title,
            mi.overview,
            mi.poster_path,
            mi.backdrop_path,
            e.intro_start_seconds,
            e.intro_end_seconds,
            e.created_at,
            e.updated_at
        from episodes e
        join media_items mi on mi.id = e.media_item_id
        where e.season_id = $1
        order by e.episode_number asc, e.id asc
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

pub async fn update_season_intro_markers(
    pool: &PgPool,
    season_id: i64,
    intro_start_seconds: Option<i32>,
    intro_end_seconds: Option<i32>,
) -> Result<Option<Season>> {
    let row = sqlx::query(
        r#"
        update seasons
        set
            intro_start_seconds = $2,
            intro_end_seconds = $3,
            updated_at = now()
        where id = $1
        returning
            id,
            series_id,
            season_number,
            title,
            overview,
            poster_path,
            backdrop_path,
            intro_start_seconds,
            intro_end_seconds,
            0::bigint as episode_count,
            created_at,
            updated_at
        "#,
    )
    .bind(season_id)
    .bind(intro_start_seconds)
    .bind(intro_end_seconds)
    .fetch_optional(pool)
    .await
    .context("failed to update season intro markers")?;

    Ok(row.map(map_season_row))
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
            count(e.id) as episode_count,
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

pub async fn get_series_episode_outline_cache(
    pool: &PgPool,
    series_media_item_id: i64,
) -> Result<Option<SeriesEpisodeOutlineCacheEntry>> {
    let row = sqlx::query(
        r#"
        select
            series_media_item_id,
            outline_json,
            fetched_at,
            expires_at,
            updated_at
        from series_episode_outline_cache
        where series_media_item_id = $1
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
) -> Result<SeriesEpisodeOutlineCacheEntry> {
    let row = sqlx::query(
        r#"
        insert into series_episode_outline_cache (
            series_media_item_id,
            outline_json,
            fetched_at,
            expires_at
        ) values ($1, $2, $3, $4)
        on conflict (series_media_item_id)
        do update set
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
    .bind(params.outline_json)
    .bind(params.fetched_at)
    .bind(params.expires_at)
    .fetch_one(pool)
    .await
    .context("failed to upsert series episode outline cache")?;

    Ok(map_series_episode_outline_cache_entry_row(row))
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
            mf.file_path,
            mi.media_type,
            mi.metadata_provider,
            mi.metadata_provider_item_id,
            mi.title,
            mi.source_title,
            mi.original_title,
            mi.sort_title,
            mi.year,
            mi.imdb_rating,
            mi.country,
            mi.genres,
            mi.studio,
            mi.overview,
            mi.poster_path,
            mi.backdrop_path,
            series_mi.title as series_title,
            series_mi.source_title as series_source_title,
            series_mi.original_title as series_original_title,
            series_mi.sort_title as series_sort_title,
            series_mi.year as series_year,
            series_mi.imdb_rating as series_imdb_rating,
            series_mi.country as series_country,
            series_mi.genres as series_genres,
            series_mi.studio as series_studio,
            series_mi.overview as series_overview,
            series_mi.poster_path as series_poster_path,
            series_mi.backdrop_path as series_backdrop_path,
            s.title as season_title,
            s.overview as season_overview,
            s.poster_path as season_poster_path,
            s.backdrop_path as season_backdrop_path,
            e.title as episode_title
        from media_files mf
        join media_items mi on mi.id = mf.media_item_id
        left join episodes e on e.media_item_id = mi.id
        left join seasons s on s.id = e.season_id
        left join media_items series_mi on series_mi.id = e.series_id
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
        year: row.get("year"),
        imdb_rating: row.get("imdb_rating"),
        country: row.get("country"),
        genres: row.get("genres"),
        studio: row.get("studio"),
        overview: row.get("overview"),
        poster_path: row.get("poster_path"),
        backdrop_path: row.get("backdrop_path"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
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
        id: row.get("id"),
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
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_media_file_row(row: PgRow) -> MediaFile {
    MediaFile {
        id: row.get("id"),
        media_item_id: row.get("media_item_id"),
        file_path: row.get("file_path"),
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
        scan_hash: row.get("scan_hash"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_existing_media_metadata_summary_row(row: PgRow) -> ExistingMediaMetadataSummary {
    ExistingMediaMetadataSummary {
        file_path: row.get("file_path"),
        media_type: row.get("media_type"),
        metadata_provider: row.get("metadata_provider"),
        metadata_provider_item_id: row.get("metadata_provider_item_id"),
        title: row.get("title"),
        source_title: row.get("source_title"),
        original_title: row.get("original_title"),
        sort_title: row.get("sort_title"),
        year: row.get("year"),
        imdb_rating: row.get("imdb_rating"),
        country: row.get("country"),
        genres: row.get("genres"),
        studio: row.get("studio"),
        overview: row.get("overview"),
        poster_path: row.get("poster_path"),
        backdrop_path: row.get("backdrop_path"),
        series_title: row.get("series_title"),
        series_source_title: row.get("series_source_title"),
        series_original_title: row.get("series_original_title"),
        series_sort_title: row.get("series_sort_title"),
        series_year: row.get("series_year"),
        series_imdb_rating: row.get("series_imdb_rating"),
        series_country: row.get("series_country"),
        series_genres: row.get("series_genres"),
        series_studio: row.get("series_studio"),
        series_overview: row.get("series_overview"),
        series_poster_path: row.get("series_poster_path"),
        series_backdrop_path: row.get("series_backdrop_path"),
        season_title: row.get("season_title"),
        season_overview: row.get("season_overview"),
        season_poster_path: row.get("season_poster_path"),
        season_backdrop_path: row.get("season_backdrop_path"),
        episode_title: row.get("episode_title"),
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
