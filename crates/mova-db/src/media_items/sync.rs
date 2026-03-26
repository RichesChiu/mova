use super::{series, CreateMediaEntryParams};
use anyhow::{Context, Result};
use sqlx::{postgres::PgPool, Postgres, Row, Transaction};
use std::collections::{HashMap, HashSet};

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
            delete_media_item(&mut tx, record.media_item_id).await?;
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
        delete_media_item(&mut tx, existing.media_item_id).await?;
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
        select distinct mi.id as media_item_id
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

    let media_item_ids = rows
        .into_iter()
        .map(|row| row.get::<i64, _>("media_item_id"))
        .collect::<Vec<_>>();

    for media_item_id in &media_item_ids {
        delete_media_item(&mut tx, *media_item_id).await?;
    }

    if !media_item_ids.is_empty() {
        series::cleanup_orphan_series_structure(&mut tx, library_id).await?;
    }

    tx.commit()
        .await
        .context("failed to commit directory media deletion transaction")?;

    Ok(media_item_ids.len() as u64)
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

async fn upsert_movie_media_entry(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
    existing: Option<ExistingLibraryMediaFileRecord>,
) -> Result<()> {
    if let Some(existing) = existing {
        if !existing.media_type.eq_ignore_ascii_case("movie") {
            delete_media_item(tx, existing.media_item_id).await?;
            let media_item_id = insert_media_item(tx, entry).await?;
            insert_media_file(tx, media_item_id, entry).await?;
            return Ok(());
        }

        update_media_item_from_entry(tx, existing.media_item_id, entry).await?;
        update_media_file_from_entry(tx, existing.media_file_id, entry).await?;
        return Ok(());
    }

    let media_item_id = insert_media_item(tx, entry).await?;
    insert_media_file(tx, media_item_id, entry).await?;
    Ok(())
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
    let row = sqlx::query(
        r#"
        insert into media_items (
            library_id,
            media_type,
            title,
            source_title,
            original_title,
            sort_title,
            year,
            overview,
            poster_path,
            backdrop_path
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        returning id
        "#,
    )
    .bind(entry.library_id)
    .bind(&entry.media_type)
    .bind(&entry.title)
    .bind(&entry.source_title)
    .bind(&entry.original_title)
    .bind(&entry.sort_title)
    .bind(entry.year)
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
    sqlx::query(
        r#"
        update media_items
        set
            media_type = $2,
            title = $3,
            source_title = $4,
            original_title = $5,
            sort_title = $6,
            year = $7,
            overview = $8,
            poster_path = $9,
            backdrop_path = $10,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .bind(&entry.media_type)
    .bind(&entry.title)
    .bind(&entry.source_title)
    .bind(&entry.original_title)
    .bind(&entry.sort_title)
    .bind(entry.year)
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
) -> Result<()> {
    sqlx::query(
        r#"
        insert into media_files (
            library_id,
            media_item_id,
            file_path,
            container,
            file_size,
            duration_seconds,
            video_codec,
            audio_codec,
            width,
            height,
            bitrate,
            scan_hash
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, null)
        "#,
    )
    .bind(entry.library_id)
    .bind(media_item_id)
    .bind(&entry.file_path)
    .bind(&entry.container)
    .bind(entry.file_size)
    .bind(entry.duration_seconds)
    .bind(&entry.video_codec)
    .bind(&entry.audio_codec)
    .bind(entry.width)
    .bind(entry.height)
    .bind(entry.bitrate)
    .execute(&mut **tx)
    .await
    .context("failed to insert media file")?;

    Ok(())
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
            video_codec = $6,
            audio_codec = $7,
            width = $8,
            height = $9,
            bitrate = $10,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_file_id)
    .bind(&entry.file_path)
    .bind(&entry.container)
    .bind(entry.file_size)
    .bind(entry.duration_seconds)
    .bind(&entry.video_codec)
    .bind(&entry.audio_codec)
    .bind(entry.width)
    .bind(entry.height)
    .bind(entry.bitrate)
    .execute(&mut **tx)
    .await
    .context("failed to update media file during library sync")?;

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
