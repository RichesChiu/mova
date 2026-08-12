use super::{
    apply_local_metadata_projection_tx, capture_local_metadata_projection_checkpoint_tx,
    ratings::replace_media_item_remote_data, reconcile_local_metadata_snapshot_tx,
    restore_authoritative_local_metadata_projection_tx, series, CreateMediaEntryParams,
    LocalMetadataProjectionScope,
};
use anyhow::{Context, Result};
use mova_domain::{
    METADATA_FAILURE_PROVIDER_DISABLED, METADATA_FAILURE_PROVIDER_ERROR, METADATA_STATUS_MATCHED,
};
use sqlx::{postgres::PgPool, Postgres, QueryBuilder, Row, Transaction};
use std::collections::{HashMap, HashSet};

use crate::{
    background_jobs::{
        lock_library_scan_background_job_fence, BackgroundJobFence, LibraryScanFenceMode,
    },
    local_metadata::{reconcile_library_local_metadata_source_paths_tx, MediaLocalMetadataTarget},
    playback_progress::merge_media_item_user_state,
    tmdb_revalidation::{
        lock_library_tmdb_artwork_reference_write, record_authoritative_tmdb_snapshot_tx,
    },
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncLibraryMediaBestEffortOutcome {
    pub removed_count: usize,
    pub upserted_count: usize,
    pub failed_count: usize,
}

#[derive(Debug, Clone)]
pub struct ScanGroupCommitOutcome {
    pub upserted_count: usize,
    pub scan_job: mova_domain::ScanJob,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanGroupCommitStage {
    Local,
    Remote,
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
    lock_library_tmdb_artwork_reference_write(&mut tx, library_id).await?;

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

/// 增量同步当前扫描确认有变化的媒体记录。
/// `discovered_paths` 是本轮仍存在的全部视频路径；`entries` 只包含新增或内容发生变化的路径。
/// `allow_empty_discovery_with_observed_issues` 只能在遍历成功且至少观察到一个可恢复的
/// 单文件 discovery issue 时启用；遍历或文件 I/O 失败必须在调用此函数前中止。
#[allow(clippy::too_many_arguments)]
pub async fn sync_library_media_changes(
    pool: &PgPool,
    library_id: i64,
    scan_job_id: i64,
    discovered_paths: &[String],
    allow_empty_discovery_with_observed_issues: bool,
    retained_local_metadata_source_paths: &[String],
    entries: &[CreateMediaEntryParams],
    fence: &BackgroundJobFence,
) -> Result<SyncLibraryMediaBestEffortOutcome> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start authoritative media reconciliation transaction")?;
    lock_library_scan_background_job_fence(
        &mut tx,
        fence,
        scan_job_id,
        Some(library_id),
        LibraryScanFenceMode::Running,
    )
    .await?;
    lock_library_tmdb_artwork_reference_write(&mut tx, library_id).await?;
    sqlx::query("select set_config('mova.defer_catalog_revision', 'on', true)")
        .fetch_one(&mut *tx)
        .await
        .context("failed to defer catalog revisions for media reconciliation")?;

    let existing_records = list_library_media_files_for_sync(&mut tx, library_id)
        .await
        .context("failed to list existing library media paths for incremental sync")?;
    validate_authoritative_discovery(
        library_id,
        existing_records.len(),
        discovered_paths.len(),
        allow_empty_discovery_with_observed_issues,
    )?;
    let mut existing_by_path = existing_records
        .into_iter()
        .map(|record| (record.file_path.clone(), record))
        .collect::<HashMap<_, _>>();
    let discovered_paths = discovered_paths
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut outcome = SyncLibraryMediaBestEffortOutcome::default();

    let missing_paths = existing_by_path
        .keys()
        .filter(|path| !discovered_paths.contains(path.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    for missing_path in missing_paths {
        let record = existing_by_path
            .remove(&missing_path)
            .context("missing media path disappeared from the reconciliation snapshot")?;
        delete_media_file_and_cleanup_item(&mut tx, record.media_item_id, record.media_file_id)
            .await
            .with_context(|| {
                format!(
                    "failed to delete missing media path during authoritative reconciliation: {}",
                    record.file_path
                )
            })?;
        outcome.removed_count += 1;
    }

    for entry in entries {
        let existing = existing_by_path.remove(entry.file_path.as_str());
        upsert_media_entry(&mut tx, entry, existing)
            .await
            .with_context(|| {
                format!(
                    "failed to upsert media path during authoritative reconciliation: {}",
                    entry.file_path
                )
            })?;
        outcome.upserted_count += 1;
    }

    let removed_local_metadata_source_count = reconcile_library_local_metadata_source_paths_tx(
        &mut tx,
        library_id,
        retained_local_metadata_source_paths,
    )
    .await
    .context("failed to reconcile local metadata source paths")?;

    // Group commits intentionally defer whole-library orphan cleanup to this
    // authoritative finalization transaction. Running it once per scan avoids
    // two full-library anti-join deletes for every local and remote group.
    let removed_orphan_structure_count =
        series::cleanup_orphan_series_structure(&mut tx, library_id).await?;

    if outcome.removed_count > 0
        || outcome.upserted_count > 0
        || removed_local_metadata_source_count > 0
        || removed_orphan_structure_count > 0
    {
        sqlx::query("select mova_bump_realtime_revision($1)")
            .bind(format!("library:{library_id}:catalog"))
            .fetch_one(&mut *tx)
            .await
            .context("failed to bump reconciled catalog revision")?;
    }

    tx.commit()
        .await
        .context("failed to commit authoritative media reconciliation")?;

    Ok(outcome)
}

/// Cleans hierarchy left by scan groups when a pipeline exits before the
/// authoritative final reconciliation. The same fencing rule prevents a
/// stale worker from cleaning data owned by a replacement scan.
pub async fn cleanup_library_orphan_series_after_scan(
    pool: &PgPool,
    library_id: i64,
    scan_job_id: i64,
    fence: &BackgroundJobFence,
    allow_cancel_requested: bool,
) -> Result<usize> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start interrupted scan hierarchy cleanup")?;
    lock_library_scan_background_job_fence(
        &mut tx,
        fence,
        scan_job_id,
        Some(library_id),
        if allow_cancel_requested {
            LibraryScanFenceMode::Active
        } else {
            LibraryScanFenceMode::Running
        },
    )
    .await?;
    lock_library_tmdb_artwork_reference_write(&mut tx, library_id).await?;
    sqlx::query("select set_config('mova.defer_catalog_revision', 'on', true)")
        .fetch_one(&mut *tx)
        .await
        .context("failed to defer catalog revision during hierarchy cleanup")?;

    let removed_count = series::cleanup_orphan_series_structure(&mut tx, library_id).await?;
    if removed_count > 0 {
        sqlx::query("select mova_bump_realtime_revision($1)")
            .bind(format!("library:{library_id}:catalog"))
            .fetch_one(&mut *tx)
            .await
            .context("failed to bump catalog revision after hierarchy cleanup")?;
    }
    tx.commit()
        .await
        .context("failed to commit interrupted scan hierarchy cleanup")?;
    Ok(removed_count)
}

fn validate_authoritative_discovery(
    library_id: i64,
    existing_file_count: usize,
    discovered_file_count: usize,
    allow_empty_discovery_with_observed_issues: bool,
) -> Result<()> {
    if existing_file_count > 0
        && discovered_file_count == 0
        && !allow_empty_discovery_with_observed_issues
    {
        anyhow::bail!(
            "refusing authoritative media reconciliation for non-empty library {library_id}: discovery returned zero media files"
        );
    }

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
    lock_library_tmdb_artwork_reference_write(&mut tx, library_id).await?;

    let existing =
        get_existing_library_media_file_by_path(&mut tx, library_id, &entry.file_path).await?;
    upsert_media_entry(&mut tx, entry, existing).await?;
    series::cleanup_orphan_series_structure(&mut tx, library_id).await?;

    tx.commit()
        .await
        .context("failed to commit single media upsert transaction")?;

    Ok(())
}

/// 在一个短事务中写入同一扫描组的全部媒体文件，并且只执行一次孤儿结构清理。
/// 任一条目失败时整组回滚，避免同一电影版本组或同一剧集组出现半完成状态。
pub async fn upsert_library_media_entries_by_file_path(
    pool: &PgPool,
    scan_job_id: i64,
    library_id: i64,
    group_key: &str,
    stage: ScanGroupCommitStage,
    entries: &[CreateMediaEntryParams],
    fence: &BackgroundJobFence,
) -> Result<usize> {
    if entries.is_empty() {
        return Ok(0);
    }
    Ok(upsert_library_media_entries_by_file_path_internal(
        pool,
        scan_job_id,
        library_id,
        group_key,
        stage,
        entries,
        fence,
        true,
    )
    .await?
    .upserted_count)
}

/// Scan-pipeline variant that returns the authoritative task counters written
/// by the same transaction and defers whole-library orphan cleanup to the
/// final reconciliation transaction.
pub async fn upsert_library_media_entries_by_file_path_with_progress(
    pool: &PgPool,
    scan_job_id: i64,
    library_id: i64,
    group_key: &str,
    stage: ScanGroupCommitStage,
    entries: &[CreateMediaEntryParams],
    fence: &BackgroundJobFence,
) -> Result<ScanGroupCommitOutcome> {
    upsert_library_media_entries_by_file_path_internal(
        pool,
        scan_job_id,
        library_id,
        group_key,
        stage,
        entries,
        fence,
        false,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn upsert_library_media_entries_by_file_path_internal(
    pool: &PgPool,
    scan_job_id: i64,
    library_id: i64,
    group_key: &str,
    stage: ScanGroupCommitStage,
    entries: &[CreateMediaEntryParams],
    fence: &BackgroundJobFence,
    cleanup_orphans: bool,
) -> Result<ScanGroupCommitOutcome> {
    if entries.is_empty() {
        anyhow::bail!("scan group media upsert requires at least one entry");
    }

    let mut tx = pool
        .begin()
        .await
        .context("failed to start scan group media upsert transaction")?;
    lock_library_scan_background_job_fence(
        &mut tx,
        fence,
        scan_job_id,
        Some(library_id),
        LibraryScanFenceMode::Running,
    )
    .await?;
    lock_library_tmdb_artwork_reference_write(&mut tx, library_id).await?;

    sqlx::query("select set_config('mova.defer_catalog_revision', 'on', true)")
        .fetch_one(&mut *tx)
        .await
        .context("failed to defer row-level catalog revisions for scan group")?;

    let preserve_existing_parent = should_preserve_existing_parent(stage, entries);
    let entry_paths = entries
        .iter()
        .map(|entry| entry.file_path.clone())
        .collect::<Vec<_>>();
    let mut existing_by_path =
        list_library_media_files_for_paths(&mut tx, library_id, &entry_paths).await?;
    for entry in entries_ordered_for_local_metadata(entries) {
        let existing = existing_by_path.remove(entry.file_path.as_str());
        upsert_media_entry_with_policy(&mut tx, entry, existing, preserve_existing_parent).await?;
    }

    if cleanup_orphans {
        series::cleanup_orphan_series_structure(&mut tx, library_id).await?;
    }
    let scan_job = advance_scan_group_progress(
        &mut tx,
        scan_job_id,
        group_key,
        i32::try_from(entries.len()).unwrap_or(i32::MAX),
        stage,
    )
    .await?;
    sqlx::query("select mova_bump_realtime_revision($1)")
        .bind(format!("library:{library_id}:catalog"))
        .fetch_one(&mut *tx)
        .await
        .context("failed to bump scan group catalog revision")?;
    tx.commit()
        .await
        .context("failed to commit scan group media upsert transaction")?;

    Ok(ScanGroupCommitOutcome {
        upserted_count: entries.len(),
        scan_job,
    })
}

/// Persist unselected sidecars before the selected projection. This keeps the
/// final NFO projection deterministic when a multi-version group changes its
/// selected source during the same transaction.
fn entries_ordered_for_local_metadata(
    entries: &[CreateMediaEntryParams],
) -> Vec<&CreateMediaEntryParams> {
    let mut ordered = entries.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|entry| {
        entry
            .local_nfo
            .as_ref()
            .is_some_and(|source| source.is_selected)
            || entry
                .series_local_nfo
                .as_ref()
                .is_some_and(|source| source.is_selected)
    });
    ordered
}

/// Applies the remote-owned half of one already committed scan group.
///
/// The local stage is authoritative for hierarchy, file probe data, audio
/// tracks and subtitles. This transaction therefore requires every file path
/// to exist and only patches metadata-owned records (plus provider-id based
/// parent reassignment when two local groups are proven to be the same work).
pub async fn patch_library_media_entries_remote_by_file_path(
    pool: &PgPool,
    scan_job_id: i64,
    library_id: i64,
    group_key: &str,
    entries: &[CreateMediaEntryParams],
    fence: &BackgroundJobFence,
) -> Result<usize> {
    if entries.is_empty() {
        return Ok(0);
    }
    Ok(patch_library_media_entries_remote_by_file_path_internal(
        pool,
        scan_job_id,
        library_id,
        group_key,
        entries,
        fence,
        true,
    )
    .await?
    .upserted_count)
}

pub async fn patch_library_media_entries_remote_by_file_path_with_progress(
    pool: &PgPool,
    scan_job_id: i64,
    library_id: i64,
    group_key: &str,
    entries: &[CreateMediaEntryParams],
    fence: &BackgroundJobFence,
) -> Result<ScanGroupCommitOutcome> {
    patch_library_media_entries_remote_by_file_path_internal(
        pool,
        scan_job_id,
        library_id,
        group_key,
        entries,
        fence,
        false,
    )
    .await
}

async fn patch_library_media_entries_remote_by_file_path_internal(
    pool: &PgPool,
    scan_job_id: i64,
    library_id: i64,
    group_key: &str,
    entries: &[CreateMediaEntryParams],
    fence: &BackgroundJobFence,
    cleanup_orphans: bool,
) -> Result<ScanGroupCommitOutcome> {
    if entries.is_empty() {
        anyhow::bail!("remote scan patch requires at least one entry");
    }

    let mut tx = pool
        .begin()
        .await
        .context("failed to start scan group remote patch transaction")?;
    lock_library_scan_background_job_fence(
        &mut tx,
        fence,
        scan_job_id,
        Some(library_id),
        LibraryScanFenceMode::Running,
    )
    .await?;
    lock_library_tmdb_artwork_reference_write(&mut tx, library_id).await?;
    sqlx::query("select set_config('mova.defer_catalog_revision', 'on', true)")
        .fetch_one(&mut *tx)
        .await
        .context("failed to defer row-level catalog revisions for remote scan patch")?;

    let preserve_existing_parent =
        should_preserve_existing_parent(ScanGroupCommitStage::Remote, entries);
    let entry_paths = entries
        .iter()
        .map(|entry| entry.file_path.clone())
        .collect::<Vec<_>>();
    let mut existing_by_path =
        list_library_media_files_for_paths(&mut tx, library_id, &entry_paths).await?;
    for entry in entries {
        if entry.library_id != library_id {
            anyhow::bail!(
                "remote scan patch entry library {} does not match fenced library {library_id}",
                entry.library_id
            );
        }
        let existing = existing_by_path
            .remove(entry.file_path.as_str())
            .with_context(|| {
                format!(
                    "remote scan patch requires a locally committed media path: {}",
                    entry.file_path
                )
            })?;
        if entry.media_type.eq_ignore_ascii_case("episode") {
            series::patch_episode_remote_entry(&mut tx, entry, existing, preserve_existing_parent)
                .await?;
        } else if entry.media_type.eq_ignore_ascii_case("movie") {
            patch_movie_remote_entry(&mut tx, entry, existing, preserve_existing_parent).await?;
        } else {
            anyhow::bail!(
                "remote scan patch does not accept local media type {}",
                entry.media_type
            );
        }
    }

    if cleanup_orphans {
        series::cleanup_orphan_series_structure(&mut tx, library_id).await?;
    }
    let scan_job = advance_scan_group_progress(
        &mut tx,
        scan_job_id,
        group_key,
        i32::try_from(entries.len()).unwrap_or(i32::MAX),
        ScanGroupCommitStage::Remote,
    )
    .await?;
    sqlx::query("select mova_bump_realtime_revision($1)")
        .bind(format!("library:{library_id}:catalog"))
        .fetch_one(&mut *tx)
        .await
        .context("failed to bump remote scan patch catalog revision")?;
    tx.commit()
        .await
        .context("failed to commit scan group remote patch transaction")?;

    Ok(ScanGroupCommitOutcome {
        upserted_count: entries.len(),
        scan_job,
    })
}

async fn patch_movie_remote_entry(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
    existing: ExistingLibraryMediaFileRecord,
    preserve_existing_parent: bool,
) -> Result<()> {
    if !existing.media_type.eq_ignore_ascii_case("movie") {
        anyhow::bail!(
            "remote metadata cannot change locally committed media type {} for {}",
            existing.media_type,
            entry.file_path
        );
    }

    let target_media_item_id = if preserve_existing_parent {
        existing.media_item_id
    } else {
        let source_title = movie_group_title_for_entry(entry);
        find_existing_movie_media_item(tx, entry, &source_title)
            .await?
            .unwrap_or(existing.media_item_id)
    };
    let local_projection_checkpoint =
        capture_local_metadata_projection_checkpoint_tx(tx, target_media_item_id).await?;
    if preserve_existing_parent {
        apply_local_metadata_projection_tx(
            tx,
            target_media_item_id,
            entry,
            LocalMetadataProjectionScope::Movie,
        )
        .await?;
    }
    patch_media_item_remote_fields(
        tx,
        target_media_item_id,
        entry,
        preserve_existing_parent,
        entry.poster_path.as_deref(),
        entry.backdrop_path.as_deref(),
        entry.logo_path.as_deref(),
    )
    .await?;

    reconcile_local_metadata_snapshot_tx(
        tx,
        entry.library_id,
        MediaLocalMetadataTarget::MediaItem(target_media_item_id),
        entry.removed_local_nfo_source_path.as_deref(),
        entry.local_nfo.as_ref(),
    )
    .await?;
    restore_authoritative_local_metadata_projection_tx(
        tx,
        target_media_item_id,
        &local_projection_checkpoint,
        entry.local_nfo.as_ref(),
        LocalMetadataProjectionScope::Movie,
    )
    .await?;

    if target_media_item_id != existing.media_item_id {
        reassign_media_file_parent_only(tx, existing.media_file_id, target_media_item_id).await?;
        merge_media_item_user_state(tx, existing.media_item_id, target_media_item_id).await?;
        cleanup_media_item_if_no_files(tx, existing.media_item_id).await?;
    }

    Ok(())
}

pub(super) async fn patch_media_item_remote_fields(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    entry: &CreateMediaEntryParams,
    preserve_existing_parent: bool,
    poster_path: Option<&str>,
    backdrop_path: Option<&str>,
    logo_path: Option<&str>,
) -> Result<()> {
    if preserve_existing_parent
        && existing_media_item_has_accepted_remote_binding(tx, media_item_id).await?
    {
        record_transient_metadata_refresh_failure(tx, media_item_id, entry).await?;
        return promote_cached_artwork_paths(
            tx,
            media_item_id,
            entry,
            poster_path,
            backdrop_path,
            logo_path,
        )
        .await;
    }

    if !entry.replace_remote_data {
        sqlx::query(
            r#"
            update media_items
            set metadata_status = $2,
                metadata_failure_reason = $3,
                remote_media_type = coalesce($4, remote_media_type),
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(media_item_id)
        .bind(&entry.metadata_status)
        .bind(&entry.metadata_failure_reason)
        .bind(&entry.remote_media_type)
        .execute(&mut **tx)
        .await
        .context("failed to patch remote metadata review state")?;
        return Ok(());
    }

    let title = display_title_for_entry(entry);
    sqlx::query(
        r#"
        update media_items
        set title = $2,
            original_title = $3,
            metadata_provider = $4,
            metadata_provider_item_id = $5,
            metadata_status = $6,
            metadata_failure_reason = $7,
            remote_media_type = $8,
            year = $9,
            country = $10,
            genres = $11,
            studio = $12,
            overview = $13,
            poster_path = case when $17 then $14 else coalesce($14, poster_path) end,
            backdrop_path = case when $17 then $15 else coalesce($15, backdrop_path) end,
            logo_path = case when $17 then $16 else coalesce($16, logo_path) end,
            tagline = $18,
            premiere_date = $19,
            content_rating = $20,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .bind(title)
    .bind(&entry.original_title)
    .bind(&entry.metadata_provider)
    .bind(entry.metadata_provider_item_id.as_deref())
    .bind(&entry.metadata_status)
    .bind(&entry.metadata_failure_reason)
    .bind(&entry.remote_media_type)
    .bind(entry.year)
    .bind(&entry.country)
    .bind(&entry.genres)
    .bind(&entry.studio)
    .bind(&entry.overview)
    .bind(poster_path)
    .bind(backdrop_path)
    .bind(logo_path)
    .bind(entry.allow_artwork_clear)
    .bind(&entry.tagline)
    .bind(entry.premiere_date)
    .bind(&entry.content_rating)
    .execute(&mut **tx)
    .await
    .context("failed to patch remote-owned media item fields")?;

    replace_media_item_remote_data(
        tx,
        media_item_id,
        entry.metadata_provider.as_deref(),
        &entry.external_ids,
        &entry.ratings,
    )
    .await?;
    record_authoritative_tmdb_snapshot_tx(
        tx,
        media_item_id,
        entry.metadata_provider.as_deref(),
        entry.tmdb_remote_snapshot_json.as_deref(),
        entry.tmdb_remote_snapshot_renews_retention,
    )
    .await
}

async fn advance_scan_group_progress(
    tx: &mut Transaction<'_, Postgres>,
    scan_job_id: i64,
    group_key: &str,
    file_count: i32,
    stage: ScanGroupCommitStage,
) -> Result<mova_domain::ScanJob> {
    let previous = sqlx::query_as::<_, (i32, bool, bool, bool)>(
        r#"
        select file_count, local_analyzed, local_committed, remote_completed
        from scan_job_groups
        where scan_job_id = $1
          and group_key = $2
        for update
        "#,
    )
    .bind(scan_job_id)
    .bind(group_key)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock scan group checkpoint")?;
    if let Some((stored_file_count, ..)) = previous.as_ref() {
        if *stored_file_count != file_count {
            anyhow::bail!(
                "scan group {group_key} file count changed from {stored_file_count} to {file_count}"
            );
        }
    }

    let (was_analyzed, was_committed, was_remote_completed) = previous
        .map(|(_, analyzed, committed, remote)| (analyzed, committed, remote))
        .unwrap_or((false, false, false));
    let analyzed_delta = if was_analyzed { 0 } else { file_count };
    let committed_delta = if was_committed { 0 } else { file_count };
    let remote_delta = if stage == ScanGroupCommitStage::Remote && !was_remote_completed {
        file_count
    } else {
        0
    };

    match stage {
        ScanGroupCommitStage::Local => sqlx::query(
            r#"
                insert into scan_job_groups (
                    scan_job_id,
                    group_key,
                    file_count,
                    local_analyzed,
                    local_committed,
                    remote_completed
                )
                values ($1, $2, $3, true, true, false)
                on conflict (scan_job_id, group_key) do update
                    set local_analyzed = true,
                        local_committed = true
                where not scan_job_groups.local_committed
                  and scan_job_groups.file_count = excluded.file_count
                "#,
        )
        .bind(scan_job_id)
        .bind(group_key)
        .bind(file_count)
        .execute(&mut **tx)
        .await
        .context("failed to checkpoint local scan group commit")?,
        ScanGroupCommitStage::Remote => sqlx::query(
            r#"
                insert into scan_job_groups (
                    scan_job_id,
                    group_key,
                    file_count,
                    local_analyzed,
                    local_committed,
                    remote_completed
                )
                values ($1, $2, $3, true, true, true)
                on conflict (scan_job_id, group_key) do update
                    set local_analyzed = true,
                        local_committed = true,
                        remote_completed = true
                where not scan_job_groups.remote_completed
                  and scan_job_groups.file_count = excluded.file_count
                "#,
        )
        .bind(scan_job_id)
        .bind(group_key)
        .bind(file_count)
        .execute(&mut **tx)
        .await
        .context("failed to checkpoint remote scan group completion")?,
    };

    let counter_update = sqlx::query(
        r#"
        update scan_jobs
        set phase = 'processing',
            local_analyzed_files = least(
                total_files,
                local_analyzed_files + $2
            ),
            local_committed_files = least(
                total_files,
                local_committed_files + $3
            ),
            remote_completed_files = least(
                total_files,
                remote_completed_files + $4
            )
        where id = $1
          and status = 'running'
        "#,
    )
    .bind(scan_job_id)
    .bind(analyzed_delta)
    .bind(committed_delta)
    .bind(remote_delta)
    .execute(&mut **tx)
    .await
    .context("failed to advance scan group work counters")?;
    if counter_update.rows_affected() != 1 {
        anyhow::bail!(
            "scan job {scan_job_id} is no longer running while committing group {group_key}"
        );
    }

    let row = sqlx::query(
        r#"
        update scan_jobs
        set progress_percent = greatest(
            progress_percent,
            case
                when total_files = 0 then 10
                else least(99, floor(
                    10
                    + 20.0 * local_analyzed_files / total_files
                    + 20.0 * local_committed_files / total_files
                    + 49.0 * remote_completed_files / total_files
                )::integer)
            end
        )
        where id = $1
          and status = 'running'
        returning
            id,
            library_id,
            status,
            phase,
            total_files,
            scanned_files,
            local_analyzed_files,
            local_committed_files,
            remote_completed_files,
            progress_percent,
            created_at,
            started_at,
            finished_at,
            error_message
        "#,
    )
    .bind(scan_job_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to calculate authoritative scan pipeline progress")?;
    row.map(crate::scan_jobs::map_scan_job_row)
        .with_context(|| {
            format!("scan job {scan_job_id} is no longer running while calculating group progress")
        })
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
    upsert_media_entry_with_policy(tx, entry, existing, false).await
}

async fn upsert_media_entry_with_policy(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
    existing: Option<ExistingLibraryMediaFileRecord>,
    preserve_existing_parent: bool,
) -> Result<()> {
    if entry.media_type.eq_ignore_ascii_case("episode") {
        series::upsert_episode_media_entry(tx, entry, existing, preserve_existing_parent).await
    } else {
        upsert_movie_media_entry(tx, entry, existing, preserve_existing_parent).await
    }
}

fn should_preserve_existing_parent(
    stage: ScanGroupCommitStage,
    entries: &[CreateMediaEntryParams],
) -> bool {
    stage == ScanGroupCommitStage::Local
        || !entries.iter().any(|entry| entry.replace_remote_data)
        || entries.iter().any(|entry| {
            matches!(
                entry.metadata_failure_reason.as_deref(),
                Some(METADATA_FAILURE_PROVIDER_ERROR | METADATA_FAILURE_PROVIDER_DISABLED)
            )
        })
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
    preserve_existing_parent: bool,
) -> Result<()> {
    let movie_group_title = movie_group_title_for_entry(entry);
    let existing_movie_media_item_id =
        find_existing_movie_media_item(tx, entry, &movie_group_title).await?;

    if let Some(existing) = existing {
        if !existing.media_type.eq_ignore_ascii_case("movie") {
            let target_media_item_id =
                if let Some(existing_movie_media_item_id) = existing_movie_media_item_id {
                    update_existing_media_item_from_entry(
                        tx,
                        existing_movie_media_item_id,
                        entry,
                        preserve_existing_parent,
                    )
                    .await?;
                    existing_movie_media_item_id
                } else {
                    insert_media_item(tx, entry).await?
                };
            reassign_media_file_to_media_item(
                tx,
                existing.media_file_id,
                target_media_item_id,
                entry,
            )
            .await?;
            merge_media_item_user_state(tx, existing.media_item_id, target_media_item_id).await?;
            cleanup_media_item_if_no_files(tx, existing.media_item_id).await?;

            if existing.media_type.eq_ignore_ascii_case("series") {
                series::cleanup_orphan_series_structure(tx, entry.library_id).await?;
            }

            return Ok(());
        }

        if let Some(existing_movie_media_item_id) = existing_movie_media_item_id {
            if existing_movie_media_item_id != existing.media_item_id {
                update_existing_media_item_from_entry(
                    tx,
                    existing_movie_media_item_id,
                    entry,
                    preserve_existing_parent,
                )
                .await?;
                reassign_media_file_to_media_item(
                    tx,
                    existing.media_file_id,
                    existing_movie_media_item_id,
                    entry,
                )
                .await?;
                merge_media_item_user_state(
                    tx,
                    existing.media_item_id,
                    existing_movie_media_item_id,
                )
                .await?;
                cleanup_media_item_if_no_files(tx, existing.media_item_id).await?;
                return Ok(());
            }
        }

        update_existing_media_item_from_entry(
            tx,
            existing.media_item_id,
            entry,
            preserve_existing_parent,
        )
        .await?;
        update_media_file_from_entry(tx, existing.media_file_id, entry).await?;
        return Ok(());
    }

    if let Some(existing_movie_media_item_id) = existing_movie_media_item_id {
        update_existing_media_item_from_entry(
            tx,
            existing_movie_media_item_id,
            entry,
            preserve_existing_parent,
        )
        .await?;
        insert_media_file(tx, existing_movie_media_item_id, entry).await?;
        return Ok(());
    }

    let media_item_id = insert_media_item(tx, entry).await?;
    insert_media_file(tx, media_item_id, entry).await?;
    Ok(())
}

pub(super) async fn update_existing_media_item_from_entry(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    entry: &CreateMediaEntryParams,
    preserve_existing_parent: bool,
) -> Result<()> {
    if !preserve_existing_parent {
        return update_media_item_from_entry(tx, media_item_id, entry).await;
    }

    if existing_media_item_has_accepted_remote_binding(tx, media_item_id).await? {
        apply_local_metadata_projection_tx(
            tx,
            media_item_id,
            entry,
            LocalMetadataProjectionScope::Movie,
        )
        .await?;
        reconcile_local_metadata_snapshot_tx(
            tx,
            entry.library_id,
            MediaLocalMetadataTarget::MediaItem(media_item_id),
            entry.removed_local_nfo_source_path.as_deref(),
            entry.local_nfo.as_ref(),
        )
        .await?;
        record_transient_metadata_refresh_failure(tx, media_item_id, entry).await?;
        return promote_cached_artwork_paths(
            tx,
            media_item_id,
            entry,
            entry.poster_path.as_deref(),
            entry.backdrop_path.as_deref(),
            entry.logo_path.as_deref(),
        )
        .await;
    }

    update_media_item_from_entry(tx, media_item_id, entry).await
}

pub(super) async fn existing_media_item_has_accepted_remote_binding(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
) -> Result<bool> {
    let identity = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        r#"
        select metadata_status, metadata_provider, metadata_provider_item_id
        from media_items
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read existing media item metadata identity")?;

    Ok(
        identity.is_some_and(|(status, provider, provider_item_id)| {
            status.eq_ignore_ascii_case(METADATA_STATUS_MATCHED)
                && provider.is_some_and(|value| !value.trim().is_empty())
                && provider_item_id.is_some_and(|value| !value.trim().is_empty())
        }),
    )
}

pub(super) async fn existing_media_item_is_matched(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
) -> Result<bool> {
    let status =
        sqlx::query_scalar::<_, String>("select metadata_status from media_items where id = $1")
            .bind(media_item_id)
            .fetch_optional(&mut **tx)
            .await
            .context("failed to read existing media item metadata status")?;

    Ok(status
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case(METADATA_STATUS_MATCHED)))
}

pub(super) async fn record_transient_metadata_refresh_failure(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    entry: &CreateMediaEntryParams,
) -> Result<()> {
    if entry.metadata_failure_reason.as_deref() != Some(METADATA_FAILURE_PROVIDER_ERROR) {
        return Ok(());
    }

    sqlx::query(
        r#"
        update media_items
        set metadata_failure_reason = $2,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .bind(METADATA_FAILURE_PROVIDER_ERROR)
    .execute(&mut **tx)
    .await
    .context("failed to record transient metadata refresh failure")?;

    Ok(())
}

pub(super) async fn promote_cached_artwork_paths(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    entry: &CreateMediaEntryParams,
    poster_path: Option<&str>,
    backdrop_path: Option<&str>,
    logo_path: Option<&str>,
) -> Result<()> {
    if !entry
        .metadata_status
        .eq_ignore_ascii_case(METADATA_STATUS_MATCHED)
    {
        return Ok(());
    }

    let poster_path = local_artwork_path(poster_path);
    let backdrop_path = local_artwork_path(backdrop_path);
    let logo_path = local_artwork_path(logo_path);
    if poster_path.is_none() && backdrop_path.is_none() && logo_path.is_none() {
        return Ok(());
    }

    sqlx::query(
        r#"
        update media_items
        set poster_path = case
                when $2::text is not null
                     and (
                         lower(coalesce(poster_path, '')) like 'http://%'
                         or lower(coalesce(poster_path, '')) like 'https://%'
                     )
                    then $2
                else poster_path
            end,
            backdrop_path = case
                when $3::text is not null
                     and (
                         lower(coalesce(backdrop_path, '')) like 'http://%'
                         or lower(coalesce(backdrop_path, '')) like 'https://%'
                     )
                    then $3
                else backdrop_path
            end,
            logo_path = case
                when $4::text is not null
                     and (
                         lower(coalesce(logo_path, '')) like 'http://%'
                         or lower(coalesce(logo_path, '')) like 'https://%'
                     )
                    then $4
                else logo_path
            end,
            updated_at = now()
        where id = $1
          and (
                (
                    $2::text is not null
                    and (
                        lower(coalesce(poster_path, '')) like 'http://%'
                        or lower(coalesce(poster_path, '')) like 'https://%'
                    )
                )
             or (
                    $3::text is not null
                    and (
                        lower(coalesce(backdrop_path, '')) like 'http://%'
                        or lower(coalesce(backdrop_path, '')) like 'https://%'
                    )
                )
             or (
                    $4::text is not null
                    and (
                        lower(coalesce(logo_path, '')) like 'http://%'
                        or lower(coalesce(logo_path, '')) like 'https://%'
                    )
                )
          )
        "#,
    )
    .bind(media_item_id)
    .bind(poster_path)
    .bind(backdrop_path)
    .bind(logo_path)
    .execute(&mut **tx)
    .await
    .context("failed to promote cached media artwork")?;

    Ok(())
}

pub(super) fn local_artwork_path(path: Option<&str>) -> Option<&str> {
    path.map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| std::path::Path::new(value).is_absolute())
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
    entry: &CreateMediaEntryParams,
    source_title: &str,
) -> Result<Option<i64>> {
    if let (Some(provider), Some(provider_item_id)) = (
        entry.metadata_provider.as_deref(),
        entry.metadata_provider_item_id.clone(),
    ) {
        let row = sqlx::query(
            r#"
            select id
            from media_items
            where library_id = $1
              and media_type = 'movie'
              and metadata_provider = $2
              and metadata_provider_item_id = $3
            order by
              case when source_title = $4 then 0 else 1 end,
              id asc
            limit 1
            "#,
        )
        .bind(entry.library_id)
        .bind(provider)
        .bind(provider_item_id)
        .bind(source_title)
        .fetch_optional(&mut **tx)
        .await
        .context("failed to find existing movie item by remote metadata id")?;

        if let Some(row) = row {
            return Ok(Some(row.get("id")));
        }
    }

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
    .bind(entry.library_id)
    .bind(source_title)
    .bind(entry.year)
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

async fn list_library_media_files_for_paths(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
    file_paths: &[String],
) -> Result<HashMap<String, ExistingLibraryMediaFileRecord>> {
    if file_paths.is_empty() {
        return Ok(HashMap::new());
    }

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
          and mf.file_path = any($2)
        "#,
    )
    .bind(library_id)
    .bind(file_paths)
    .fetch_all(&mut **tx)
    .await
    .context("failed to prefetch existing scan-group media files")?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let record = ExistingLibraryMediaFileRecord {
                media_item_id: row.get("media_item_id"),
                media_file_id: row.get("media_file_id"),
                media_type: row.get("media_type"),
                file_path: row.get("file_path"),
            };
            (record.file_path.clone(), record)
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
            metadata_status,
            metadata_failure_reason,
            remote_media_type,
            year,
            country,
            genres,
            studio,
            overview,
            poster_path,
            backdrop_path,
            logo_path,
            tagline,
            premiere_date,
            content_rating
        )
        values (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
            $14, $15, $16, $17, $18, $19, $20, $21, $22
        )
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
    .bind(entry.metadata_provider_item_id.as_deref())
    .bind(&entry.metadata_status)
    .bind(&entry.metadata_failure_reason)
    .bind(&entry.remote_media_type)
    .bind(entry.year)
    .bind(&entry.country)
    .bind(&entry.genres)
    .bind(&entry.studio)
    .bind(&entry.overview)
    .bind(&entry.poster_path)
    .bind(&entry.backdrop_path)
    .bind(&entry.logo_path)
    .bind(&entry.tagline)
    .bind(entry.premiere_date)
    .bind(&entry.content_rating)
    .fetch_one(&mut **tx)
    .await
    .context("failed to insert media item")?;

    let media_item_id = row.get("id");
    if entry.replace_remote_data {
        replace_media_item_remote_data(
            tx,
            media_item_id,
            entry.metadata_provider.as_deref(),
            &entry.external_ids,
            &entry.ratings,
        )
        .await?;
        record_authoritative_tmdb_snapshot_tx(
            tx,
            media_item_id,
            entry.metadata_provider.as_deref(),
            entry.tmdb_remote_snapshot_json.as_deref(),
            entry.tmdb_remote_snapshot_renews_retention,
        )
        .await?;
    }

    reconcile_local_metadata_snapshot_tx(
        tx,
        entry.library_id,
        MediaLocalMetadataTarget::MediaItem(media_item_id),
        entry.removed_local_nfo_source_path.as_deref(),
        entry.local_nfo.as_ref(),
    )
    .await?;

    Ok(media_item_id)
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
            metadata_status = $9,
            metadata_failure_reason = $10,
            remote_media_type = $11,
            year = $12,
            country = $13,
            genres = $14,
            studio = $15,
            overview = $16,
            poster_path = $17,
            backdrop_path = $18,
            logo_path = $19,
            tagline = $20,
            premiere_date = $21,
            content_rating = $22,
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
    .bind(entry.metadata_provider_item_id.as_deref())
    .bind(&entry.metadata_status)
    .bind(&entry.metadata_failure_reason)
    .bind(&entry.remote_media_type)
    .bind(entry.year)
    .bind(&entry.country)
    .bind(&entry.genres)
    .bind(&entry.studio)
    .bind(&entry.overview)
    .bind(&entry.poster_path)
    .bind(&entry.backdrop_path)
    .bind(&entry.logo_path)
    .bind(&entry.tagline)
    .bind(entry.premiere_date)
    .bind(&entry.content_rating)
    .execute(&mut **tx)
    .await
    .context("failed to update media item during library sync")?;

    if entry.replace_remote_data {
        replace_media_item_remote_data(
            tx,
            media_item_id,
            entry.metadata_provider.as_deref(),
            &entry.external_ids,
            &entry.ratings,
        )
        .await?;
        record_authoritative_tmdb_snapshot_tx(
            tx,
            media_item_id,
            entry.metadata_provider.as_deref(),
            entry.tmdb_remote_snapshot_json.as_deref(),
            entry.tmdb_remote_snapshot_renews_retention,
        )
        .await?;
    }

    reconcile_local_metadata_snapshot_tx(
        tx,
        entry.library_id,
        MediaLocalMetadataTarget::MediaItem(media_item_id),
        entry.removed_local_nfo_source_path.as_deref(),
        entry.local_nfo.as_ref(),
    )
    .await?;

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
            local_analysis_version,
            scan_hash
        )
        values (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29
        )
        returning id
        "#,
    )
    .bind(entry.library_id)
    .bind(media_item_id)
    .bind(&entry.file_path)
    .bind(entry.source_kind.as_str())
    .bind(&entry.stream_reference_hash)
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
    .bind(&entry.technical_tags)
    .bind(entry.local_analysis_version)
    .bind(&entry.scan_hash)
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
            local_analysis_version = $27,
            scan_hash = $28,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_file_id)
    .bind(&entry.file_path)
    .bind(entry.source_kind.as_str())
    .bind(&entry.stream_reference_hash)
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
    .bind(&entry.technical_tags)
    .bind(entry.local_analysis_version)
    .bind(&entry.scan_hash)
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

    if !audio_tracks.is_empty() {
        let mut query = QueryBuilder::<Postgres>::new(
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
            ) "#,
        );
        query.push_values(audio_tracks, |mut values, audio_track| {
            values
                .push_bind(media_file_id)
                .push_bind(audio_track.stream_index)
                .push_bind(&audio_track.language)
                .push_bind(&audio_track.audio_codec)
                .push_bind(&audio_track.label)
                .push_bind(&audio_track.channel_layout)
                .push_bind(audio_track.channels)
                .push_bind(audio_track.bitrate)
                .push_bind(audio_track.sample_rate)
                .push_bind(audio_track.is_default);
        });
        query
            .build()
            .execute(&mut **tx)
            .await
            .context("failed to batch insert audio tracks during media sync")?;
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

    if !subtitles.is_empty() {
        let mut query = QueryBuilder::<Postgres>::new(
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
            ) "#,
        );
        query.push_values(subtitles, |mut values, subtitle| {
            values
                .push_bind(media_file_id)
                .push_bind(&subtitle.source_kind)
                .push_bind(&subtitle.file_path)
                .push_bind(subtitle.stream_index)
                .push_bind(&subtitle.language)
                .push_bind(&subtitle.subtitle_format)
                .push_bind(&subtitle.label)
                .push_bind(subtitle.is_default)
                .push_bind(subtitle.is_forced)
                .push_bind(subtitle.is_hearing_impaired);
        });
        query
            .build()
            .execute(&mut **tx)
            .await
            .context("failed to batch insert subtitle files during media sync")?;
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
            source_kind = $4,
            stream_reference_hash = $5,
            container = $6,
            file_size = $7,
            duration_seconds = $8,
            video_title = $9,
            video_codec = $10,
            video_profile = $11,
            video_level = $12,
            audio_codec = $13,
            width = $14,
            height = $15,
            bitrate = $16,
            video_bitrate = $17,
            video_frame_rate = $18,
            video_aspect_ratio = $19,
            video_scan_type = $20,
            video_color_primaries = $21,
            video_color_space = $22,
            video_color_transfer = $23,
            video_bit_depth = $24,
            video_pixel_format = $25,
            video_reference_frames = $26,
            technical_tags = $27,
            local_analysis_version = $28,
            scan_hash = $29,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_file_id)
    .bind(target_media_item_id)
    .bind(&entry.file_path)
    .bind(entry.source_kind.as_str())
    .bind(&entry.stream_reference_hash)
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
    .bind(&entry.technical_tags)
    .bind(entry.local_analysis_version)
    .bind(&entry.scan_hash)
    .execute(&mut **tx)
    .await
    .context("failed to reassign media file during library sync")?;

    replace_audio_tracks_for_media_file_tx(tx, media_file_id, &entry.audio_tracks).await?;
    replace_subtitle_files_for_media_file_tx(tx, media_file_id, &entry.subtitle_tracks).await?;

    Ok(())
}

pub(super) async fn reassign_media_file_parent_only(
    tx: &mut Transaction<'_, Postgres>,
    media_file_id: i64,
    target_media_item_id: i64,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        update media_files
        set media_item_id = $2,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_file_id)
    .bind(target_media_item_id)
    .execute(&mut **tx)
    .await
    .context("failed to reassign media file during remote metadata patch")?;
    if result.rows_affected() != 1 {
        anyhow::bail!("media file {media_file_id} disappeared during remote metadata patch");
    }

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
#[path = "sync/tests.rs"]
mod tests;
