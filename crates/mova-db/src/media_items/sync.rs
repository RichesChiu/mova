use super::{series, CreateMediaEntryParams};
use anyhow::{Context, Result};
use sqlx::{postgres::PgPool, Postgres, Row, Transaction};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncLibraryMediaBestEffortOutcome {
    pub removed_count: usize,
    pub upserted_count: usize,
    pub failed_count: usize,
}

/// 按文件路径把最新扫描结果增量同步到某个媒体库。
/// 同路径文件会原地更新；缺失路径会删除；新增路径会插入。
pub async fn sync_library_media(
    pool: &PgPool,
    library_id: i64,
    entries: &[CreateMediaEntryParams],
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start media sync transaction")?;

    let existing_records = list_library_media_files_for_sync(&mut tx, library_id).await?;
    let mut existing_by_path = existing_records
        .into_iter()
        .map(|record| (record.file_path.clone(), record))
        .collect::<HashMap<_, _>>();
    let discovered_paths = entries
        .iter()
        .map(|entry| entry.file_path.as_str())
        .collect::<HashSet<_>>();

    for record in existing_by_path.values() {
        if !discovered_paths.contains(record.file_path.as_str()) {
            delete_media_file_and_cleanup_item(&mut tx, record.media_item_id, record.media_file_id)
                .await?;
        }
    }

    for entry in entries {
        let existing = existing_by_path.remove(entry.file_path.as_str());
        upsert_media_entry(&mut tx, entry, existing).await?;
    }

    series::cleanup_orphan_series_structure(&mut tx, library_id).await?;

    tx.commit()
        .await
        .context("failed to commit media sync transaction")?;

    Ok(())
}

/// 当整库事务同步因为单条脏数据失败时，回退到逐条删除/逐条 upsert。
/// 这样可以尽量保住其余健康条目，不因为一条异常记录让整轮扫描完全失败。
pub async fn sync_library_media_best_effort(
    pool: &PgPool,
    library_id: i64,
    entries: &[CreateMediaEntryParams],
) -> Result<SyncLibraryMediaBestEffortOutcome> {
    let existing_paths = super::list_library_media_file_paths(pool, library_id)
        .await
        .context("failed to list existing library media paths for fallback sync")?;
    let discovered_paths = entries
        .iter()
        .map(|entry| entry.file_path.as_str())
        .collect::<HashSet<_>>();

    let mut outcome = SyncLibraryMediaBestEffortOutcome::default();

    for existing_path in existing_paths {
        if discovered_paths.contains(existing_path.as_str()) {
            continue;
        }

        match delete_library_media_by_file_path(pool, library_id, &existing_path).await {
            Ok(_) => {
                outcome.removed_count += 1;
            }
            Err(error) => {
                outcome.failed_count += 1;
                tracing::warn!(
                    library_id,
                    file_path = %existing_path,
                    error = ?error,
                    "best-effort library sync failed to delete missing media path"
                );
            }
        }
    }

    for entry in entries {
        match upsert_library_media_entry_by_file_path(pool, library_id, entry).await {
            Ok(_) => {
                outcome.upserted_count += 1;
            }
            Err(error) => {
                outcome.failed_count += 1;
                tracing::warn!(
                    library_id,
                    file_path = %entry.file_path,
                    media_type = %entry.media_type,
                    title = %entry.title,
                    error = ?error,
                    "best-effort library sync failed to upsert media entry"
                );
            }
        }
    }

    Ok(outcome)
}

/// 按文件路径增量 upsert 单条媒体记录。
pub async fn upsert_library_media_entry_by_file_path(
    pool: &PgPool,
    library_id: i64,
    entry: &CreateMediaEntryParams,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start single media upsert transaction")?;

    let existing =
        get_existing_library_media_file_by_path(&mut tx, library_id, &entry.file_path).await?;
    upsert_media_entry(&mut tx, entry, existing).await?;
    series::cleanup_orphan_series_structure(&mut tx, library_id).await?;

    tx.commit()
        .await
        .context("failed to commit single media upsert transaction")?;

    Ok(())
}

/// 删除某个库中指定文件路径对应的媒体记录。
pub async fn delete_library_media_by_file_path(
    pool: &PgPool,
    library_id: i64,
    file_path: &str,
) -> Result<u64> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start single media deletion transaction")?;

    let rows_affected = if let Some(existing) =
        get_existing_library_media_file_by_path(&mut tx, library_id, file_path).await?
    {
        delete_media_file_and_cleanup_item(&mut tx, existing.media_item_id, existing.media_file_id)
            .await?;
        series::cleanup_orphan_series_structure(&mut tx, library_id).await?;
        1
    } else {
        0
    };

    tx.commit()
        .await
        .context("failed to commit single media deletion transaction")?;

    Ok(rows_affected)
}

/// 删除某个库中某个目录前缀下的全部媒体记录。
pub async fn delete_library_media_by_path_prefix(
    pool: &PgPool,
    library_id: i64,
    path_prefix: &str,
) -> Result<u64> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start directory media deletion transaction")?;

    let rows = sqlx::query(
        r#"
        select
            mi.id as media_item_id,
            mf.id as media_file_id
        from media_items mi
        join media_files mf on mf.media_item_id = mi.id
        where mf.library_id = $1
          and (mf.file_path = $2 or mf.file_path like $2 || '/%')
        "#,
    )
    .bind(library_id)
    .bind(path_prefix)
    .fetch_all(&mut *tx)
    .await
    .context("failed to list media items for directory deletion")?;

    let media_file_records = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<i64, _>("media_item_id"),
                row.get::<i64, _>("media_file_id"),
            )
        })
        .collect::<Vec<_>>();

    for (media_item_id, media_file_id) in &media_file_records {
        delete_media_file_and_cleanup_item(&mut tx, *media_item_id, *media_file_id).await?;
    }

    if !media_file_records.is_empty() {
        series::cleanup_orphan_series_structure(&mut tx, library_id).await?;
    }

    tx.commit()
        .await
        .context("failed to commit directory media deletion transaction")?;

    Ok(media_file_records.len() as u64)
}

pub(super) async fn upsert_media_entry(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
    existing: Option<ExistingLibraryMediaFileRecord>,
) -> Result<()> {
    if entry.media_type.eq_ignore_ascii_case("episode") {
        series::upsert_episode_media_entry(tx, entry, existing).await
    } else {
        upsert_movie_media_entry(tx, entry, existing).await
    }
}

pub(super) fn display_title_for_entry(entry: &CreateMediaEntryParams) -> String {
    // 远端 metadata 缺失或返回异常标题时，列表仍然要能稳定展示本地资源。
    let title = entry.title.trim();
    if !title.is_empty() {
        return title.to_string();
    }

    let source_title = entry.source_title.trim();
    if !source_title.is_empty() {
        return source_title.to_string();
    }

    "Untitled".to_string()
}

async fn upsert_movie_media_entry(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
    existing: Option<ExistingLibraryMediaFileRecord>,
) -> Result<()> {
    let movie_group_title = movie_group_title_for_entry(entry);
    let existing_movie_media_item_id =
        find_existing_movie_media_item(tx, entry.library_id, &movie_group_title, entry.year)
            .await?;

    if let Some(existing) = existing {
        if !existing.media_type.eq_ignore_ascii_case("movie") {
            delete_media_item(tx, existing.media_item_id).await?;

            if let Some(existing_movie_media_item_id) = existing_movie_media_item_id {
                update_media_item_from_entry(tx, existing_movie_media_item_id, entry).await?;
                insert_media_file(tx, existing_movie_media_item_id, entry).await?;
            } else {
                let media_item_id = insert_media_item(tx, entry).await?;
                insert_media_file(tx, media_item_id, entry).await?;
            }

            return Ok(());
        }

        if let Some(existing_movie_media_item_id) = existing_movie_media_item_id {
            if existing_movie_media_item_id != existing.media_item_id {
                update_media_item_from_entry(tx, existing_movie_media_item_id, entry).await?;
                reassign_media_file_to_media_item(
                    tx,
                    existing.media_file_id,
                    existing_movie_media_item_id,
                    entry,
                )
                .await?;
                cleanup_media_item_if_no_files(tx, existing.media_item_id).await?;
                return Ok(());
            }
        }

        update_media_item_from_entry(tx, existing.media_item_id, entry).await?;
        update_media_file_from_entry(tx, existing.media_file_id, entry).await?;
        return Ok(());
    }

    if let Some(existing_movie_media_item_id) = existing_movie_media_item_id {
        update_media_item_from_entry(tx, existing_movie_media_item_id, entry).await?;
        insert_media_file(tx, existing_movie_media_item_id, entry).await?;
        return Ok(());
    }

    let media_item_id = insert_media_item(tx, entry).await?;
    insert_media_file(tx, media_item_id, entry).await?;
    Ok(())
}

fn movie_group_title_for_entry(entry: &CreateMediaEntryParams) -> String {
    let source_title = entry.source_title.trim();
    if !source_title.is_empty() {
        return source_title.to_string();
    }

    display_title_for_entry(entry)
}

async fn find_existing_movie_media_item(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
    source_title: &str,
    year: Option<i32>,
) -> Result<Option<i64>> {
    let row = sqlx::query(
        r#"
        select id
        from media_items
        where library_id = $1
          and media_type = 'movie'
          and source_title = $2
          and (
                ($3::int is null and year is null)
                or year = $3
              )
        order by id asc
        limit 1
        "#,
    )
    .bind(library_id)
    .bind(source_title)
    .bind(year)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to find existing movie item")?;

    Ok(row.map(|row| row.get("id")))
}

#[derive(Debug, Clone)]
pub(super) struct ExistingLibraryMediaFileRecord {
    pub media_item_id: i64,
    pub media_file_id: i64,
    pub media_type: String,
    pub file_path: String,
}

pub(super) async fn list_library_media_files_for_sync(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
) -> Result<Vec<ExistingLibraryMediaFileRecord>> {
    let rows = sqlx::query(
        r#"
        select
            mi.id as media_item_id,
            mf.id as media_file_id,
            mi.media_type,
            mf.file_path
        from media_files mf
        join media_items mi on mi.id = mf.media_item_id
        where mf.library_id = $1
        "#,
    )
    .bind(library_id)
    .fetch_all(&mut **tx)
    .await
    .context("failed to list existing library media files for sync")?;

    Ok(rows
        .into_iter()
        .map(|row| ExistingLibraryMediaFileRecord {
            media_item_id: row.get("media_item_id"),
            media_file_id: row.get("media_file_id"),
            media_type: row.get("media_type"),
            file_path: row.get("file_path"),
        })
        .collect())
}

pub(super) async fn get_existing_library_media_file_by_path(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
    file_path: &str,
) -> Result<Option<ExistingLibraryMediaFileRecord>> {
    let row = sqlx::query(
        r#"
        select
            mi.id as media_item_id,
            mf.id as media_file_id,
            mi.media_type,
            mf.file_path
        from media_files mf
        join media_items mi on mi.id = mf.media_item_id
        where mf.library_id = $1
          and mf.file_path = $2
        limit 1
        "#,
    )
    .bind(library_id)
    .bind(file_path)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to get existing library media file by path")?;

    Ok(row.map(|row| ExistingLibraryMediaFileRecord {
        media_item_id: row.get("media_item_id"),
        media_file_id: row.get("media_file_id"),
        media_type: row.get("media_type"),
        file_path: row.get("file_path"),
    }))
}

async fn insert_media_item(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
) -> Result<i64> {
    let title = display_title_for_entry(entry);
    let row = sqlx::query(
        r#"
        insert into media_items (
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
            backdrop_path
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
        returning id
        "#,
    )
    .bind(entry.library_id)
    .bind(&entry.media_type)
    .bind(title)
    .bind(&entry.source_title)
    .bind(&entry.original_title)
    .bind(&entry.sort_title)
    .bind(&entry.metadata_provider)
    .bind(entry.metadata_provider_item_id)
    .bind(entry.year)
    .bind(&entry.imdb_rating)
    .bind(&entry.country)
    .bind(&entry.genres)
    .bind(&entry.studio)
    .bind(&entry.overview)
    .bind(&entry.poster_path)
    .bind(&entry.backdrop_path)
    .fetch_one(&mut **tx)
    .await
    .context("failed to insert media item")?;

    Ok(row.get("id"))
}

async fn update_media_item_from_entry(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    entry: &CreateMediaEntryParams,
) -> Result<()> {
    let title = display_title_for_entry(entry);

    sqlx::query(
        r#"
        update media_items
        set
            media_type = $2,
            title = $3,
            source_title = $4,
            original_title = $5,
            sort_title = $6,
            metadata_provider = $7,
            metadata_provider_item_id = $8,
            year = $9,
            imdb_rating = $10,
            country = $11,
            genres = $12,
            studio = $13,
            overview = $14,
            poster_path = $15,
            backdrop_path = $16,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .bind(&entry.media_type)
    .bind(title)
    .bind(&entry.source_title)
    .bind(&entry.original_title)
    .bind(&entry.sort_title)
    .bind(&entry.metadata_provider)
    .bind(entry.metadata_provider_item_id)
    .bind(entry.year)
    .bind(&entry.imdb_rating)
    .bind(&entry.country)
    .bind(&entry.genres)
    .bind(&entry.studio)
    .bind(&entry.overview)
    .bind(&entry.poster_path)
    .bind(&entry.backdrop_path)
    .execute(&mut **tx)
    .await
    .context("failed to update media item during library sync")?;

    Ok(())
}

pub(super) async fn insert_media_file(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    entry: &CreateMediaEntryParams,
) -> Result<i64> {
    let row = sqlx::query(
        r#"
        insert into media_files (
            library_id,
            media_item_id,
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
            scan_hash
        )
        values (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19, $20, $21, $22, $23, $24, null
        )
        returning id
        "#,
    )
    .bind(entry.library_id)
    .bind(media_item_id)
    .bind(&entry.file_path)
    .bind(&entry.container)
    .bind(entry.file_size)
    .bind(entry.duration_seconds)
    .bind(&entry.video_title)
    .bind(&entry.video_codec)
    .bind(&entry.video_profile)
    .bind(&entry.video_level)
    .bind(&entry.audio_codec)
    .bind(entry.width)
    .bind(entry.height)
    .bind(entry.bitrate)
    .bind(entry.video_bitrate)
    .bind(entry.video_frame_rate)
    .bind(&entry.video_aspect_ratio)
    .bind(&entry.video_scan_type)
    .bind(&entry.video_color_primaries)
    .bind(&entry.video_color_space)
    .bind(&entry.video_color_transfer)
    .bind(entry.video_bit_depth)
    .bind(&entry.video_pixel_format)
    .bind(entry.video_reference_frames)
    .fetch_one(&mut **tx)
    .await
    .context("failed to insert media file")?;

    let media_file_id = row.get("id");
    replace_audio_tracks_for_media_file_tx(tx, media_file_id, &entry.audio_tracks).await?;
    replace_subtitle_files_for_media_file_tx(tx, media_file_id, &entry.subtitle_tracks).await?;

    Ok(media_file_id)
}

pub(super) async fn update_media_file_from_entry(
    tx: &mut Transaction<'_, Postgres>,
    media_file_id: i64,
    entry: &CreateMediaEntryParams,
) -> Result<()> {
    sqlx::query(
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
        "#,
    )
    .bind(media_file_id)
    .bind(&entry.file_path)
    .bind(&entry.container)
    .bind(entry.file_size)
    .bind(entry.duration_seconds)
    .bind(&entry.video_title)
    .bind(&entry.video_codec)
    .bind(&entry.video_profile)
    .bind(&entry.video_level)
    .bind(&entry.audio_codec)
    .bind(entry.width)
    .bind(entry.height)
    .bind(entry.bitrate)
    .bind(entry.video_bitrate)
    .bind(entry.video_frame_rate)
    .bind(&entry.video_aspect_ratio)
    .bind(&entry.video_scan_type)
    .bind(&entry.video_color_primaries)
    .bind(&entry.video_color_space)
    .bind(&entry.video_color_transfer)
    .bind(entry.video_bit_depth)
    .bind(&entry.video_pixel_format)
    .bind(entry.video_reference_frames)
    .execute(&mut **tx)
    .await
    .context("failed to update media file during library sync")?;

    replace_audio_tracks_for_media_file_tx(tx, media_file_id, &entry.audio_tracks).await?;
    replace_subtitle_files_for_media_file_tx(tx, media_file_id, &entry.subtitle_tracks).await?;

    Ok(())
}

async fn replace_audio_tracks_for_media_file_tx(
    tx: &mut Transaction<'_, Postgres>,
    media_file_id: i64,
    audio_tracks: &[super::CreateAudioTrackParams],
) -> Result<()> {
    sqlx::query(
        r#"
        delete from audio_tracks
        where media_file_id = $1
        "#,
    )
    .bind(media_file_id)
    .execute(&mut **tx)
    .await
    .context("failed to delete audio tracks during media sync")?;

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
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
        .execute(&mut **tx)
        .await
        .context("failed to insert audio track during media sync")?;
    }

    Ok(())
}

async fn replace_subtitle_files_for_media_file_tx(
    tx: &mut Transaction<'_, Postgres>,
    media_file_id: i64,
    subtitles: &[super::CreateSubtitleTrackParams],
) -> Result<()> {
    sqlx::query(
        r#"
        delete from subtitle_files
        where media_file_id = $1
        "#,
    )
    .bind(media_file_id)
    .execute(&mut **tx)
    .await
    .context("failed to delete subtitle files during media sync")?;

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
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
        .execute(&mut **tx)
        .await
        .context("failed to insert subtitle file during media sync")?;
    }

    Ok(())
}

pub(super) async fn reassign_media_file_to_media_item(
    tx: &mut Transaction<'_, Postgres>,
    media_file_id: i64,
    target_media_item_id: i64,
    entry: &CreateMediaEntryParams,
) -> Result<()> {
    sqlx::query(
        r#"
        update media_files
        set
            media_item_id = $2,
            file_path = $3,
            container = $4,
            file_size = $5,
            duration_seconds = $6,
            video_title = $7,
            video_codec = $8,
            video_profile = $9,
            video_level = $10,
            audio_codec = $11,
            width = $12,
            height = $13,
            bitrate = $14,
            video_bitrate = $15,
            video_frame_rate = $16,
            video_aspect_ratio = $17,
            video_scan_type = $18,
            video_color_primaries = $19,
            video_color_space = $20,
            video_color_transfer = $21,
            video_bit_depth = $22,
            video_pixel_format = $23,
            video_reference_frames = $24,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_file_id)
    .bind(target_media_item_id)
    .bind(&entry.file_path)
    .bind(&entry.container)
    .bind(entry.file_size)
    .bind(entry.duration_seconds)
    .bind(&entry.video_title)
    .bind(&entry.video_codec)
    .bind(&entry.video_profile)
    .bind(&entry.video_level)
    .bind(&entry.audio_codec)
    .bind(entry.width)
    .bind(entry.height)
    .bind(entry.bitrate)
    .bind(entry.video_bitrate)
    .bind(entry.video_frame_rate)
    .bind(&entry.video_aspect_ratio)
    .bind(&entry.video_scan_type)
    .bind(&entry.video_color_primaries)
    .bind(&entry.video_color_space)
    .bind(&entry.video_color_transfer)
    .bind(entry.video_bit_depth)
    .bind(&entry.video_pixel_format)
    .bind(entry.video_reference_frames)
    .execute(&mut **tx)
    .await
    .context("failed to reassign media file during library sync")?;

    replace_audio_tracks_for_media_file_tx(tx, media_file_id, &entry.audio_tracks).await?;
    replace_subtitle_files_for_media_file_tx(tx, media_file_id, &entry.subtitle_tracks).await?;

    Ok(())
}

pub(super) async fn delete_media_file_and_cleanup_item(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    media_file_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        delete from media_files
        where id = $1
        "#,
    )
    .bind(media_file_id)
    .execute(&mut **tx)
    .await
    .context("failed to delete removed media file during library sync")?;

    cleanup_media_item_if_no_files(tx, media_item_id).await
}

pub(super) async fn cleanup_media_item_if_no_files(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
) -> Result<()> {
    let has_files = sqlx::query_scalar::<_, bool>(
        r#"
        select exists(
            select 1
            from media_files
            where media_item_id = $1
        )
        "#,
    )
    .bind(media_item_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to check remaining media files during library sync")?;

    if !has_files {
        delete_media_item(tx, media_item_id).await?;
    }

    Ok(())
}

pub(super) async fn delete_media_item(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        delete from media_items
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .execute(&mut **tx)
    .await
    .context("failed to delete removed media item during library sync")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::sync_library_media;
    use crate::{create_library, CreateLibraryParams, CreateMediaEntryParams};

    fn build_movie_entry(library_id: i64, file_path: &str) -> CreateMediaEntryParams {
        CreateMediaEntryParams {
            library_id,
            media_type: "movie".to_string(),
            metadata_provider: Some("tmdb".to_string()),
            metadata_provider_item_id: Some(101),
            title: "A Writer's Odyssey".to_string(),
            source_title: "A Writer's Odyssey".to_string(),
            original_title: Some("刺杀小说家".to_string()),
            sort_title: None,
            year: Some(2025),
            imdb_rating: Some("6.8".to_string()),
            country: Some("China".to_string()),
            genres: Some("Fantasy · Adventure".to_string()),
            studio: Some("Huayi Brothers".to_string()),
            season_number: None,
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: None,
            episode_title: None,
            overview: Some("A fantasy adventure.".to_string()),
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            file_path: file_path.to_string(),
            container: Some("mkv".to_string()),
            file_size: 1,
            duration_seconds: Some(7800),
            video_title: None,
            video_codec: Some("hevc".to_string()),
            video_profile: Some("Main 10".to_string()),
            video_level: Some("5.1".to_string()),
            audio_codec: Some("eac3".to_string()),
            width: Some(3840),
            height: Some(2160),
            bitrate: Some(18_000_000),
            video_bitrate: Some(17_000_000),
            video_frame_rate: Some(23.976),
            video_aspect_ratio: Some("16:9".to_string()),
            video_scan_type: Some("progressive".to_string()),
            video_color_primaries: Some("bt2020".to_string()),
            video_color_space: Some("bt2020nc".to_string()),
            video_color_transfer: Some("smpte2084".to_string()),
            video_bit_depth: Some(10),
            video_pixel_format: Some("yuv420p10le".to_string()),
            video_reference_frames: Some(4),
            audio_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
        }
    }

    fn build_episode_entry(library_id: i64, file_path: &str) -> CreateMediaEntryParams {
        CreateMediaEntryParams {
            library_id,
            media_type: "episode".to_string(),
            metadata_provider: Some("tmdb".to_string()),
            metadata_provider_item_id: Some(202),
            title: "Interstellar Classroom".to_string(),
            source_title: "Interstellar Classroom".to_string(),
            original_title: Some("Interstellar Classroom".to_string()),
            sort_title: None,
            year: Some(2024),
            imdb_rating: None,
            country: Some("Japan".to_string()),
            genres: Some("Animation · Sci-Fi".to_string()),
            studio: Some("Studio Trigger".to_string()),
            season_number: Some(1),
            season_title: Some("Season 01".to_string()),
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: Some(1),
            episode_title: Some("Pilot".to_string()),
            overview: Some("Pilot episode".to_string()),
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            file_path: file_path.to_string(),
            container: Some("mkv".to_string()),
            file_size: 1,
            duration_seconds: Some(1800),
            video_title: None,
            video_codec: Some("h264".to_string()),
            video_profile: None,
            video_level: None,
            audio_codec: Some("aac".to_string()),
            width: Some(1920),
            height: Some(1080),
            bitrate: Some(4_000_000),
            video_bitrate: Some(3_500_000),
            video_frame_rate: Some(23.976),
            video_aspect_ratio: Some("16:9".to_string()),
            video_scan_type: Some("progressive".to_string()),
            video_color_primaries: None,
            video_color_space: None,
            video_color_transfer: None,
            video_bit_depth: Some(8),
            video_pixel_format: Some("yuv420p".to_string()),
            video_reference_frames: None,
            audio_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn sync_library_media_reuses_one_movie_record_for_multiple_files(
        pool: sqlx::postgres::PgPool,
    ) {
        let library = create_library(
            &pool,
            CreateLibraryParams {
                name: "Movies".to_string(),
                description: None,
                metadata_language: "en-US".to_string(),
                root_path: "/media/movies".to_string(),
                is_enabled: true,
            },
        )
        .await
        .unwrap();

        let entries = vec![
            build_movie_entry(
                library.id,
                "/media/movies/A Writer's Odyssey (2025)/A Writer's Odyssey (2025).2160p.mkv",
            ),
            build_movie_entry(
                library.id,
                "/media/movies/A Writer's Odyssey (2025)/A Writer's Odyssey (2025).remux.mkv",
            ),
        ];

        sync_library_media(&pool, library.id, &entries)
            .await
            .unwrap();

        let movie_media_item_count = sqlx::query_scalar::<_, i64>(
            "select count(*) from media_items where media_type = 'movie'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let media_file_count = sqlx::query_scalar::<_, i64>("select count(*) from media_files")
            .fetch_one(&pool)
            .await
            .unwrap();
        let linked_file_count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)
            from media_files
            where media_item_id = (
                select id
                from media_items
                where media_type = 'movie'
                limit 1
            )
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(movie_media_item_count, 1);
        assert_eq!(media_file_count, 2);
        assert_eq!(linked_file_count, 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn sync_library_media_reuses_one_episode_record_for_multiple_files(
        pool: sqlx::postgres::PgPool,
    ) {
        let library = create_library(
            &pool,
            CreateLibraryParams {
                name: "Shows".to_string(),
                description: None,
                metadata_language: "en-US".to_string(),
                root_path: "/media/shows".to_string(),
                is_enabled: true,
            },
        )
        .await
        .unwrap();

        let entries = vec![
            build_episode_entry(
                library.id,
                "/media/shows/Interstellar Classroom/Season 01/S01E01.1080p.mkv",
            ),
            build_episode_entry(
                library.id,
                "/media/shows/Interstellar Classroom/Season 01/S01E01.4k.mkv",
            ),
        ];

        sync_library_media(&pool, library.id, &entries)
            .await
            .unwrap();

        let episode_count = sqlx::query_scalar::<_, i64>("select count(*) from episodes")
            .fetch_one(&pool)
            .await
            .unwrap();
        let episode_media_item_count = sqlx::query_scalar::<_, i64>(
            "select count(*) from media_items where media_type = 'episode'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let series_media_item_count = sqlx::query_scalar::<_, i64>(
            "select count(*) from media_items where media_type = 'series'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let media_file_count = sqlx::query_scalar::<_, i64>("select count(*) from media_files")
            .fetch_one(&pool)
            .await
            .unwrap();
        let linked_file_count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)
            from media_files
            where media_item_id = (
                select media_item_id
                from episodes
                limit 1
            )
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(series_media_item_count, 1);
        assert_eq!(episode_media_item_count, 1);
        assert_eq!(episode_count, 1);
        assert_eq!(media_file_count, 2);
        assert_eq!(linked_file_count, 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn sync_library_media_best_effort_keeps_healthy_entries_when_one_entry_is_invalid(
        pool: sqlx::postgres::PgPool,
    ) {
        let library = create_library(
            &pool,
            CreateLibraryParams {
                name: "Movies".to_string(),
                description: None,
                metadata_language: "en-US".to_string(),
                root_path: "/media/movies".to_string(),
                is_enabled: true,
            },
        )
        .await
        .unwrap();

        let mut invalid_entry =
            build_movie_entry(library.id, "/media/movies/Broken/Broken.invalid.mkv");
        invalid_entry.title = "X".repeat(700);
        invalid_entry.source_title = "X".repeat(700);

        let valid_entry = build_movie_entry(library.id, "/media/movies/Healthy/Healthy.mkv");

        let outcome =
            super::sync_library_media_best_effort(&pool, library.id, &[invalid_entry, valid_entry])
                .await
                .unwrap();

        let media_item_count =
            sqlx::query_scalar::<_, i64>("select count(*) from media_items where library_id = $1")
                .bind(library.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let media_file_count =
            sqlx::query_scalar::<_, i64>("select count(*) from media_files where library_id = $1")
                .bind(library.id)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(outcome.failed_count, 1);
        assert_eq!(outcome.upserted_count, 1);
        assert_eq!(media_item_count, 1);
        assert_eq!(media_file_count, 1);
    }
}
