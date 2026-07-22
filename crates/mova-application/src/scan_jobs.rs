use crate::{
    error::{ApplicationError, ApplicationResult},
    libraries::get_library,
    media_classification::classify_media_type,
    media_enrichment::{MetadataEnrichmentContext, MetadataEnrichmentStage},
    metadata::{MetadataProvider, MetadataSeasonAirYearHint, TMDB_PROVIDER_NAME},
};
use mova_domain::{
    Library, ScanJob, ScanNotificationIssue, ScanNotificationSummary, MAX_SCAN_NOTIFICATION_ISSUES,
};
use mova_domain::{
    METADATA_FAILURE_NO_REMOTE_MATCH, METADATA_FAILURE_PROVIDER_DISABLED,
    METADATA_FAILURE_PROVIDER_ERROR, METADATA_STATUS_FAILED, METADATA_STATUS_MATCHED,
    METADATA_STATUS_PENDING, METADATA_STATUS_SKIPPED, METADATA_STATUS_UNMATCHED,
    REMOTE_MEDIA_TYPE_MOVIE, REMOTE_MEDIA_TYPE_SERIES,
};
use mova_scan::{
    discovered_media_file_inventory_scan_hash, discovered_media_file_scan_hash,
    infer_series_file_metadata, infer_series_sidecar_metadata, DiscoveredAudioTrack,
    DiscoveredMediaFile, DiscoveredMediaFileInventory, DiscoveredSubtitleTrack,
};
use sqlx::postgres::PgPool;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicI32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

/// 触发媒体库扫描时返回的结果。
/// `created = false` 表示本次没有新建任务，而是复用了当前库已有的活跃任务。
#[derive(Debug)]
pub struct EnqueueLibraryScanResult {
    pub scan_job: ScanJob,
    pub created: bool,
}

/// 扫描任务执行完成后的结果。
#[derive(Debug)]
pub enum ExecuteScanJobOutcome {
    Completed(ScanJob),
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum ScanJobEvent {
    Updated(ScanJobProgressUpdate),
    Checkpoint(ScanJobProgressUpdate),
    Finished(ScanJobProgressUpdate),
    ItemUpdated(ScanJobItemProgressUpdate),
}

#[derive(Debug, Clone)]
pub struct ScanJobProgressUpdate {
    pub scan_job: ScanJob,
    pub phase: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ScanJobItemProgressUpdate {
    pub scan_job_id: i64,
    pub library_id: i64,
    pub item_key: String,
    pub media_type: String,
    pub title: String,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub metadata_status: Option<String>,
    pub remote_media_type: Option<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub item_index: i32,
    pub total_items: i32,
    pub stage: String,
    pub progress_percent: i32,
}

#[derive(Debug, Clone, Copy)]
enum ScanItemStage {
    Analyzed,
    PendingCommitted,
    Metadata,
    Artwork,
    Completed,
}

#[derive(Debug)]
enum DiscoverMediaFilesOutcome {
    Completed(Vec<DiscoveredMediaFileInventory>),
    Cancelled(i32),
}

#[derive(Debug, Clone)]
struct LocalSeriesGroup {
    lookup_title: String,
    display_title: String,
    year: Option<i32>,
    year_priority: u8,
    identity_from_sidecar: bool,
    identity_season_number: i32,
    has_first_season: bool,
    season_air_year: Option<MetadataSeasonAirYearHint>,
    file_indexes: Vec<usize>,
    classified_episode_count: usize,
}

#[derive(Debug, Clone)]
struct ScanPresentationGroup {
    item_key: String,
    media_type: String,
    title: String,
    lookup_title: String,
    year: Option<i32>,
    season_air_year: Option<MetadataSeasonAirYearHint>,
}

#[derive(Debug)]
struct ScanDiscoveredGroup {
    presentation: ScanPresentationGroup,
    files: Vec<DiscoveredMediaFile>,
}

#[derive(Debug)]
struct QueuedScanGroup {
    group: ScanDiscoveredGroup,
    item_index: i32,
    total_items: i32,
}

#[derive(Debug, Default)]
struct RemoteScanPipelineOutcome {
    sync: mova_db::SyncLibraryMediaBestEffortOutcome,
    notification_summary: ScanNotificationSummary,
}

#[derive(Debug)]
struct PendingScanGroup {
    files: Vec<IncrementalScanFile>,
}

#[derive(Debug)]
struct IncrementalScanPlan {
    discovered_paths: Vec<String>,
    changed_files: Vec<IncrementalScanFile>,
}

#[derive(Debug, Clone)]
struct IncrementalScanFile {
    inventory: DiscoveredMediaFileInventory,
    existing_metadata: Option<mova_db::ExistingMediaMetadataSummary>,
}

#[derive(Debug)]
struct PendingScanFile {
    changed_file: IncrementalScanFile,
    file: DiscoveredMediaFile,
}

#[derive(Debug, Clone)]
struct GroupMetadataLookupDecision {
    lookup_type: Option<&'static str>,
    metadata_status: &'static str,
    metadata_failure_reason: Option<&'static str>,
    remote_media_type: Option<&'static str>,
}

const SCAN_PHASE_DISCOVERING: &str = "discovering";
const SCAN_PHASE_PROCESSING: &str = "processing";
const SCAN_PHASE_FINALIZING: &str = "finalizing";
const SCAN_PHASE_FINISHED: &str = "finished";

const SCAN_ITEM_STAGE_ANALYZED: &str = "analyzed";
const SCAN_ITEM_STAGE_PENDING_COMMITTED: &str = "pending_committed";
const SCAN_ITEM_STAGE_METADATA: &str = "metadata";
const SCAN_ITEM_STAGE_ARTWORK: &str = "artwork";
const SCAN_ITEM_STAGE_COMPLETED: &str = "completed";

const SCAN_PHASE_INITIALIZING: &str = "initializing";
const SCAN_DISCOVERY_PROGRESS_MIN_FILE_DELTA: i32 = 25;
const SCAN_DISCOVERY_PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const LOCAL_ANALYSIS_VERSION: i32 = 5;

fn should_flush_discovery_progress(
    persisted_progress: i32,
    pending_progress: i32,
    last_flush_at: Option<Instant>,
    now: Instant,
) -> bool {
    if pending_progress <= persisted_progress {
        return false;
    }

    if persisted_progress <= 0 {
        return true;
    }

    if pending_progress.saturating_sub(persisted_progress) >= SCAN_DISCOVERY_PROGRESS_MIN_FILE_DELTA
    {
        return true;
    }

    last_flush_at.is_some_and(|last_flush_at| {
        now.saturating_duration_since(last_flush_at) >= SCAN_DISCOVERY_PROGRESS_MIN_INTERVAL
    })
}

async fn flush_discovery_progress(
    pool: &PgPool,
    scan_job_id: i64,
    scanned_files: i32,
    event_listener: &Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> Option<i32> {
    match mova_db::update_scan_job_progress(pool, scan_job_id, None, scanned_files).await {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_DISCOVERING,
            )));
            Some(scanned_files)
        }
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(
                scan_job_id,
                scanned_files,
                error = ?error,
                "failed to update throttled scan progress"
            );
            None
        }
    }
}

/// 读取某个媒体库的扫描历史。
pub async fn list_scan_jobs_for_library(
    pool: &PgPool,
    library_id: i64,
) -> ApplicationResult<Vec<ScanJob>> {
    get_library(pool, library_id).await?;

    mova_db::list_scan_jobs_for_library(pool, library_id)
        .await
        .map_err(ApplicationError::from)
}

/// 查询某个媒体库下的单个扫描任务详情。
/// 用于前端轮询 `POST /scan` 返回的任务状态。
pub async fn get_scan_job_for_library(
    pool: &PgPool,
    library_id: i64,
    scan_job_id: i64,
) -> ApplicationResult<ScanJob> {
    get_library(pool, library_id).await?;

    let scan_job = mova_db::get_scan_job(pool, scan_job_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| {
            ApplicationError::NotFound(format!("scan job not found: {}", scan_job_id))
        })?;

    if scan_job.library_id != library_id {
        return Err(ApplicationError::NotFound(format!(
            "scan job {} not found in library {}",
            scan_job_id, library_id
        )));
    }

    Ok(scan_job)
}

/// 创建一条 pending 状态的扫描任务，供 HTTP 层立即返回给客户端。
pub async fn enqueue_library_scan(
    pool: &PgPool,
    library_id: i64,
) -> ApplicationResult<EnqueueLibraryScanResult> {
    let library = get_library(pool, library_id).await?;

    let result = mova_db::enqueue_scan_job(
        pool,
        mova_db::CreateScanJobParams {
            library_id: library.id,
        },
    )
    .await
    .map_err(ApplicationError::from)?;

    Ok(EnqueueLibraryScanResult {
        scan_job: result.scan_job,
        created: result.created,
    })
}

/// 真正执行扫描任务：切到 running，扫描落库，最后写 success/failed 终态。
pub async fn execute_scan_job(
    pool: &PgPool,
    library_id: i64,
    scan_job_id: i64,
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> ApplicationResult<ScanJob> {
    let cancellation_flag = Arc::new(AtomicBool::new(false));
    let event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync> = Arc::new(|_| {});

    let outcome = match execute_scan_job_with_cancellation(
        pool,
        library_id,
        scan_job_id,
        cancellation_flag,
        artwork_cache_dir,
        metadata_provider,
        event_listener,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            if let Ok(Some(scan_job)) = mova_db::get_scan_job(pool, scan_job_id).await {
                let error_message = scan_job
                    .error_message
                    .as_deref()
                    .map(str::to_string)
                    .unwrap_or_else(|| error.to_string());
                let _ = mova_db::finalize_scan_job(
                    pool,
                    scan_job_id,
                    "failed",
                    scan_job.total_files,
                    scan_job.scanned_files,
                    Some(&error_message),
                    None,
                )
                .await;
            }
            return Err(error);
        }
    };

    match outcome {
        ExecuteScanJobOutcome::Completed(scan_job) => Ok(scan_job),
        ExecuteScanJobOutcome::Cancelled => Err(ApplicationError::Conflict(format!(
            "scan job {} was cancelled",
            scan_job_id
        ))),
    }
}

/// 执行可取消的扫描任务。
/// 当库正在删除或任务已被外部终止时，返回 `Cancelled` 而不是把它当成系统故障。
pub async fn execute_scan_job_with_cancellation(
    pool: &PgPool,
    library_id: i64,
    scan_job_id: i64,
    cancellation_flag: Arc<AtomicBool>,
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> ApplicationResult<ExecuteScanJobOutcome> {
    if is_cancelled(&cancellation_flag) {
        if let Some(scan_job) = finalize_cancelled_scan(pool, scan_job_id, 0, 0).await {
            event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_FINISHED,
            )));
        }
        return Ok(ExecuteScanJobOutcome::Cancelled);
    }

    let library = match get_library(pool, library_id).await {
        Ok(library) => library,
        Err(ApplicationError::NotFound(_)) => {
            return Ok(ExecuteScanJobOutcome::Cancelled);
        }
        Err(error) => {
            let message = format_scan_phase_error(
                SCAN_PHASE_INITIALIZING,
                format!("Failed to load library configuration: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, 0, 0, &message).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    };

    if is_cancelled(&cancellation_flag) {
        if let Some(scan_job) = finalize_cancelled_scan(pool, scan_job_id, 0, 0).await {
            event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_FINISHED,
            )));
        }
        return Ok(ExecuteScanJobOutcome::Cancelled);
    }

    match mova_db::mark_scan_job_running(pool, scan_job_id).await {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_DISCOVERING,
            )));
        }
        Ok(None) => {
            return Ok(ExecuteScanJobOutcome::Cancelled);
        }
        Err(error) => {
            let error = ApplicationError::from(error);
            let message = format_scan_phase_error(
                SCAN_PHASE_INITIALIZING,
                format!("Failed to start the scan job: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, 0, 0, &message).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    }

    if is_cancelled(&cancellation_flag) {
        if let Some(scan_job) = finalize_cancelled_scan(pool, scan_job_id, 0, 0).await {
            event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_FINISHED,
            )));
        }
        return Ok(ExecuteScanJobOutcome::Cancelled);
    }

    let mut sync_outcome = match reconcile_existing_media_paths(pool, library.id).await {
        Ok(outcome) => outcome,
        Err(error) => {
            let message = format_scan_phase_error(
                SCAN_PHASE_DISCOVERING,
                format!("Failed to reconcile existing media files: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, 0, 0, &message).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    };

    let discovered_files = match discover_media_files(
        pool,
        scan_job_id,
        &library,
        cancellation_flag.clone(),
        event_listener.clone(),
    )
    .await
    {
        Ok(DiscoverMediaFilesOutcome::Completed(files)) => files,
        Ok(DiscoverMediaFilesOutcome::Cancelled(scanned_files)) => {
            if let Some(scan_job) =
                finalize_cancelled_scan(pool, scan_job_id, scanned_files, scanned_files).await
            {
                event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                    scan_job,
                    SCAN_PHASE_FINISHED,
                )));
            }
            return Ok(ExecuteScanJobOutcome::Cancelled);
        }
        Err(error) => {
            let message = format_scan_phase_error(
                SCAN_PHASE_DISCOVERING,
                format!("Failed to scan library files: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, 0, 0, &message).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    };

    let total_files = discovered_files.len() as i32;
    match mova_db::update_scan_job_progress(pool, scan_job_id, Some(total_files), total_files).await
    {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_DISCOVERING,
            )));
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                scan_job_id,
                total_files,
                error = ?error,
                "failed to write final discovery progress"
            );
        }
    }

    let IncrementalScanPlan {
        discovered_paths,
        changed_files,
    } = match build_incremental_scan_plan(
        pool,
        library.id,
        discovered_files,
        metadata_provider.is_enabled(),
        &library.metadata_language,
    )
    .await
    {
        Ok(plan) => plan,
        Err(error) => {
            let message = format_scan_phase_error(
                SCAN_PHASE_INITIALIZING,
                format!("Failed to load existing media metadata: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, total_files, 0, &message).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    };

    let pending_file_count = i32::try_from(changed_files.len()).unwrap_or(i32::MAX);
    let pending_groups = match build_pending_scan_groups(&library, changed_files).await {
        Ok(groups) => groups,
        Err(error) => {
            let message = format_scan_phase_error(
                SCAN_PHASE_INITIALIZING,
                format!("Failed to plan changed media groups: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, total_files, 0, &message).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    };

    match mova_db::initialize_scan_job_work(pool, scan_job_id, total_files, pending_file_count)
        .await
    {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_PROCESSING,
            )));
        }
        Ok(None) => return Ok(ExecuteScanJobOutcome::Cancelled),
        Err(error) => {
            let message = format_scan_phase_error(
                SCAN_PHASE_PROCESSING,
                format!("Failed to initialize scan pipeline: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, total_files, 0, &message).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    }

    let total_items = i32::try_from(pending_groups.len()).unwrap_or(i32::MAX);
    let (group_sender, group_receiver) = mpsc::channel(2);
    let pipeline_result = tokio::try_join!(
        analyze_pending_scan_groups(
            pool,
            &library,
            scan_job_id,
            pending_groups,
            group_sender,
            cancellation_flag.clone(),
            event_listener.clone(),
        ),
        enrich_discovered_groups(
            pool,
            &library,
            scan_job_id,
            group_receiver,
            total_items,
            cancellation_flag.clone(),
            artwork_cache_dir,
            metadata_provider.clone(),
            event_listener.clone(),
        )
    );

    let notification_summary = match pipeline_result {
        Ok((local_outcome, remote_outcome)) => {
            merge_sync_outcome(&mut sync_outcome, local_outcome);
            merge_sync_outcome(&mut sync_outcome, remote_outcome.sync);
            remote_outcome.notification_summary
        }
        Err(error) => {
            let message = format_scan_phase_error(
                SCAN_PHASE_PROCESSING,
                format!("Failed to process scan pipeline: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, total_files, 0, &message).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    };

    if is_cancelled(&cancellation_flag) {
        if let Some(scan_job) =
            finalize_cancelled_scan(pool, scan_job_id, total_files, total_files).await
        {
            event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_FINISHED,
            )));
        }
        return Ok(ExecuteScanJobOutcome::Cancelled);
    }

    emit_scan_job_phase(
        pool,
        scan_job_id,
        SCAN_PHASE_FINALIZING,
        99,
        event_listener.clone(),
    )
    .await;

    let removal_outcome =
        match mova_db::sync_library_media_changes(pool, library.id, &discovered_paths, &[]).await {
            Ok(outcome) => outcome,
            Err(error) => {
                let message = format_scan_phase_error(
                    SCAN_PHASE_FINALIZING,
                    format!("Failed to reconcile missing media files: {}", error),
                );
                record_failed_scan_attempt(pool, scan_job_id, total_files, 0, &message).await;
                return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
            }
        };
    merge_sync_outcome(&mut sync_outcome, removal_outcome);

    if sync_outcome.failed_count > 0 {
        tracing::warn!(
            library_id = library.id,
            scan_job_id,
            removed_count = sync_outcome.removed_count,
            upserted_count = sync_outcome.upserted_count,
            failed_count = sync_outcome.failed_count,
            "incremental library sync skipped one or more problematic media changes"
        );
    }

    if sync_outcome.removed_count == 0
        && sync_outcome.upserted_count == 0
        && sync_outcome.failed_count > 0
    {
        let message =
            format_scan_phase_error(SCAN_PHASE_FINALIZING, "Failed to save changed library data");

        record_failed_scan_attempt(pool, scan_job_id, total_files, 0, &message).await;
        return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
    }

    match mova_db::finalize_scan_job(
        pool,
        scan_job_id,
        "success",
        total_files,
        total_files,
        None,
        Some(&notification_summary),
    )
    .await
    {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                scan_job.clone(),
                SCAN_PHASE_FINISHED,
            )));
            Ok(ExecuteScanJobOutcome::Completed(scan_job))
        }
        Ok(None) => Ok(ExecuteScanJobOutcome::Cancelled),
        Err(error) => Err(ApplicationError::from(error)),
    }
}

async fn build_pending_scan_groups(
    library: &Library,
    changed_files: Vec<IncrementalScanFile>,
) -> ApplicationResult<Vec<PendingScanGroup>> {
    let pending_files = inspect_incremental_scan_files_shallow(changed_files).await?;

    Ok(build_pending_scan_groups_from_files(library, pending_files))
}

async fn reconcile_existing_media_paths(
    pool: &PgPool,
    library_id: i64,
) -> ApplicationResult<mova_db::SyncLibraryMediaBestEffortOutcome> {
    let existing_paths = mova_db::list_library_media_file_paths(pool, library_id)
        .await
        .map_err(ApplicationError::Unexpected)?;

    if existing_paths.is_empty() {
        return Ok(mova_db::SyncLibraryMediaBestEffortOutcome::default());
    }

    let present_paths = tokio::task::spawn_blocking(move || {
        existing_paths
            .into_iter()
            .filter(|path| is_present_media_file_path(Path::new(path)))
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "The existing media path reconcile worker exited unexpectedly: {}",
            error
        ))
    })?;

    mova_db::sync_library_media_changes(pool, library_id, &present_paths, &[])
        .await
        .map_err(ApplicationError::Unexpected)
}

fn is_present_media_file_path(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

fn build_pending_scan_groups_from_files(
    library: &Library,
    pending_files: Vec<PendingScanFile>,
) -> Vec<PendingScanGroup> {
    let mut changed_files_by_path = HashMap::new();
    let mut shallow_files = Vec::with_capacity(pending_files.len());

    for pending_file in pending_files {
        let file_path = pending_file.file.file_path.to_string_lossy().to_string();
        shallow_files.push(pending_file.file);
        changed_files_by_path.insert(file_path, pending_file.changed_file);
    }

    let groups = group_discovered_files_for_scan(library, shallow_files);
    let mut pending_groups = Vec::with_capacity(groups.len());

    for group in groups {
        let mut group_files = Vec::with_capacity(group.files.len());

        for file in group.files {
            let file_path = file.file_path.to_string_lossy().to_string();
            if let Some(changed_file) = changed_files_by_path.remove(file_path.as_str()) {
                group_files.push(changed_file);
            }
        }

        if group_files.is_empty() {
            continue;
        }

        pending_groups.push(PendingScanGroup { files: group_files });
    }

    pending_groups
}

async fn analyze_pending_scan_groups(
    pool: &PgPool,
    library: &Library,
    scan_job_id: i64,
    pending_groups: Vec<PendingScanGroup>,
    group_sender: mpsc::Sender<QueuedScanGroup>,
    cancellation_flag: Arc<AtomicBool>,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> ApplicationResult<mova_db::SyncLibraryMediaBestEffortOutcome> {
    let total_items = i32::try_from(pending_groups.len()).unwrap_or(i32::MAX);
    let mut processed_items = 0_i32;
    let mut sync_outcome = mova_db::SyncLibraryMediaBestEffortOutcome::default();
    let mut completed_all_local_groups = true;

    'pending_groups: for pending_group in pending_groups {
        if is_cancelled(&cancellation_flag) {
            completed_all_local_groups = false;
            break;
        }

        let discovered_files = inspect_incremental_scan_files(pending_group.files).await?;
        let mut groups = group_discovered_files_for_scan(library, discovered_files);

        prepare_scan_groups_for_metadata_lookup(&mut groups);

        for group in groups {
            if is_cancelled(&cancellation_flag) {
                completed_all_local_groups = false;
                break 'pending_groups;
            }

            processed_items = processed_items.saturating_add(1);
            let item_index = processed_items;
            let effective_total_items = total_items.max(item_index);
            let analyzed_scan_job = mova_db::mark_scan_group_analyzed(
                pool,
                scan_job_id,
                &group.presentation.item_key,
                i32::try_from(group.files.len()).unwrap_or(i32::MAX),
            )
            .await
            .map_err(ApplicationError::Unexpected)?;
            event_listener(ScanJobEvent::ItemUpdated(build_scan_group_progress_update(
                scan_job_id,
                library.id,
                &group.presentation,
                group.files.first(),
                item_index,
                effective_total_items,
                ScanItemStage::Analyzed,
            )));
            if let Some(scan_job) = analyzed_scan_job {
                event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                    scan_job,
                    SCAN_PHASE_PROCESSING,
                )));
            }

            let group_outcome = sync_scan_group_media_entries(
                pool,
                scan_job_id,
                library,
                &group,
                mova_db::ScanGroupCommitStage::Local,
                false,
            )
            .await?;
            merge_sync_outcome(&mut sync_outcome, group_outcome);

            event_listener(ScanJobEvent::ItemUpdated(build_scan_group_progress_update(
                scan_job_id,
                library.id,
                &group.presentation,
                group.files.first(),
                item_index,
                effective_total_items,
                ScanItemStage::PendingCommitted,
            )));

            emit_current_scan_job_update(pool, scan_job_id, &event_listener).await;
            if is_cancelled(&cancellation_flag) {
                completed_all_local_groups = false;
                break 'pending_groups;
            }
            if group_sender
                .send(QueuedScanGroup {
                    group,
                    item_index,
                    total_items: effective_total_items,
                })
                .await
                .is_err()
            {
                if is_cancelled(&cancellation_flag) {
                    completed_all_local_groups = false;
                    break 'pending_groups;
                }
                return Err(ApplicationError::Unexpected(anyhow::anyhow!(
                    "remote scan pipeline stopped before local groups completed"
                )));
            }
        }
    }

    drop(group_sender);
    if completed_all_local_groups && processed_items > 0 {
        if let Ok(Some(scan_job)) = mova_db::get_scan_job(pool, scan_job_id).await {
            event_listener(ScanJobEvent::Checkpoint(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_PROCESSING,
            )));
        }
    }

    Ok(sync_outcome)
}

fn prepare_scan_groups_for_metadata_lookup(groups: &mut [ScanDiscoveredGroup]) {
    for group in groups {
        for file in &mut group.files {
            if group.presentation.media_type.eq_ignore_ascii_case("series") {
                file.source_title = group.presentation.lookup_title.clone();

                if file.year.is_none() {
                    file.year = group.presentation.year;
                }
            }

            file.metadata_status = Some(METADATA_STATUS_PENDING.to_string());
            file.metadata_failure_reason = None;
        }
    }
}

async fn enrich_discovered_groups(
    pool: &PgPool,
    library: &Library,
    scan_job_id: i64,
    mut group_receiver: mpsc::Receiver<QueuedScanGroup>,
    total_items: i32,
    cancellation_flag: Arc<AtomicBool>,
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> ApplicationResult<RemoteScanPipelineOutcome> {
    let mut enrichment = MetadataEnrichmentContext::new(
        artwork_cache_dir,
        metadata_provider.clone(),
        library.metadata_language.clone(),
    );
    let mut sync_outcome = mova_db::SyncLibraryMediaBestEffortOutcome::default();
    let mut notification_summary = ScanNotificationSummary::default();

    while let Some(queued_group) = group_receiver.recv().await {
        if is_cancelled(&cancellation_flag) {
            break;
        }

        let QueuedScanGroup {
            mut group,
            item_index,
            total_items: queued_total_items,
        } = queued_group;
        let total_items = total_items.max(queued_total_items).max(item_index);

        let metadata_decision =
            resolve_group_metadata_lookup_type(metadata_provider.as_ref(), &group);
        if group.files.is_empty() {
            continue;
        }
        let progress_listener = event_listener.clone();
        let mut presentation = group.presentation.clone();

        let Some(lookup_type) = metadata_decision.lookup_type else {
            for file in &mut group.files {
                clear_remote_metadata_for_review(
                    file,
                    metadata_decision.metadata_status,
                    metadata_decision.metadata_failure_reason,
                    metadata_decision.remote_media_type,
                );
            }
            let group_outcome = sync_scan_group_media_entries(
                pool,
                scan_job_id,
                library,
                &group,
                mova_db::ScanGroupCommitStage::Remote,
                true,
            )
            .await?;
            merge_sync_outcome(&mut sync_outcome, group_outcome);
            record_scan_notification_group(&mut notification_summary, &group, None);
            progress_listener(ScanJobEvent::ItemUpdated(build_scan_group_progress_update(
                scan_job_id,
                library.id,
                &group.presentation,
                group.files.first(),
                item_index,
                total_items,
                ScanItemStage::Completed,
            )));
            emit_current_scan_job_update(pool, scan_job_id, &progress_listener).await;
            continue;
        };

        let enrichment_progress_listener = progress_listener.clone();
        let season_air_year = group.presentation.season_air_year;
        let enrichment_result = enrichment
            .enrich_group_with_progress(
                lookup_type,
                &mut group.files,
                season_air_year,
                move |stage, file| {
                    if stage != MetadataEnrichmentStage::Metadata {
                        if !file.title.trim().is_empty() {
                            presentation.title = file.title.clone();
                        }
                    }

                    if stage == MetadataEnrichmentStage::Completed {
                        return;
                    }

                    enrichment_progress_listener(ScanJobEvent::ItemUpdated(
                        build_scan_group_progress_update(
                            scan_job_id,
                            library.id,
                            &presentation,
                            Some(file),
                            item_index,
                            total_items,
                            stage.into(),
                        ),
                    ));
                },
            )
            .await;

        let remote_media_type = metadata_decision.remote_media_type;
        if let Err(error) = enrichment_result {
            let failure_detail = compact_scan_failure_detail(error.root_cause().to_string());
            tracing::warn!(
                library_id = library.id,
                scan_job_id,
                title = %group.presentation.lookup_title,
                year = group.presentation.year,
                media_type = %group.presentation.media_type,
                error = ?error,
                "metadata enrichment failed for scan group"
            );

            for file in &mut group.files {
                clear_remote_metadata_for_review(
                    file,
                    METADATA_STATUS_FAILED,
                    Some(METADATA_FAILURE_PROVIDER_ERROR),
                    remote_media_type,
                );
            }

            let group_outcome = sync_scan_group_media_entries(
                pool,
                scan_job_id,
                library,
                &group,
                mova_db::ScanGroupCommitStage::Remote,
                true,
            )
            .await?;
            merge_sync_outcome(&mut sync_outcome, group_outcome);
            record_scan_notification_group(
                &mut notification_summary,
                &group,
                Some(&failure_detail),
            );
            progress_listener(ScanJobEvent::ItemUpdated(build_scan_group_progress_update(
                scan_job_id,
                library.id,
                &group.presentation,
                group.files.first(),
                item_index,
                total_items,
                ScanItemStage::Completed,
            )));
            emit_current_scan_job_update(pool, scan_job_id, &progress_listener).await;
            continue;
        }

        for file in &mut group.files {
            finalize_file_metadata_status(
                file,
                metadata_provider.is_enabled(),
                remote_media_type_for_lookup_type(lookup_type),
            );
        }

        if let Some(primary_file) = group.files.first() {
            if !primary_file.title.trim().is_empty() {
                group.presentation.title = primary_file.title.clone();
            }
        }

        let group_outcome = sync_scan_group_media_entries(
            pool,
            scan_job_id,
            library,
            &group,
            mova_db::ScanGroupCommitStage::Remote,
            true,
        )
        .await?;
        merge_sync_outcome(&mut sync_outcome, group_outcome);
        record_scan_notification_group(&mut notification_summary, &group, None);
        progress_listener(ScanJobEvent::ItemUpdated(build_scan_group_progress_update(
            scan_job_id,
            library.id,
            &group.presentation,
            group.files.first(),
            item_index,
            total_items,
            ScanItemStage::Completed,
        )));
        emit_current_scan_job_update(pool, scan_job_id, &progress_listener).await;
    }

    Ok(RemoteScanPipelineOutcome {
        sync: sync_outcome,
        notification_summary,
    })
}

async fn sync_scan_group_media_entries(
    pool: &PgPool,
    scan_job_id: i64,
    library: &Library,
    group: &ScanDiscoveredGroup,
    stage: mova_db::ScanGroupCommitStage,
    allow_artwork_clear: bool,
) -> ApplicationResult<mova_db::SyncLibraryMediaBestEffortOutcome> {
    let entries = build_media_entries(library, group.files.clone(), allow_artwork_clear)?;
    let upserted_count = mova_db::upsert_library_media_entries_by_file_path(
        pool,
        scan_job_id,
        library.id,
        &group.presentation.item_key,
        stage,
        &entries,
    )
    .await
    .map_err(ApplicationError::Unexpected)?;

    Ok(mova_db::SyncLibraryMediaBestEffortOutcome {
        upserted_count,
        ..Default::default()
    })
}

fn record_scan_notification_group(
    summary: &mut ScanNotificationSummary,
    group: &ScanDiscoveredGroup,
    failure_detail: Option<&str>,
) {
    let primary_file = group.files.first();
    let probe_warnings = group
        .files
        .iter()
        .filter_map(|file| {
            file.probe_error.as_ref().map(|detail| {
                (
                    file.file_path.to_string_lossy().to_string(),
                    compact_scan_failure_detail(detail),
                )
            })
        })
        .collect::<Vec<_>>();
    let first_probe_warning = probe_warnings.first().cloned();

    let metadata_status = primary_file
        .and_then(|file| file.metadata_status.clone())
        .unwrap_or_else(|| METADATA_STATUS_FAILED.to_string());
    let file_count = i32::try_from(group.files.len()).unwrap_or(i32::MAX);

    match metadata_status.as_str() {
        METADATA_STATUS_MATCHED => summary.matched_files += file_count,
        METADATA_STATUS_UNMATCHED => summary.unmatched_files += file_count,
        METADATA_STATUS_SKIPPED => summary.skipped_files += file_count,
        _ => summary.failed_files += file_count,
    }

    let probe_warning_count = i32::try_from(probe_warnings.len()).unwrap_or(i32::MAX);
    summary.probe_warning_count = summary
        .probe_warning_count
        .saturating_add(probe_warning_count);
    let has_issue = matches!(
        metadata_status.as_str(),
        METADATA_STATUS_UNMATCHED | METADATA_STATUS_FAILED
    ) || probe_warning_count > 0;
    if !has_issue {
        return;
    }

    summary.issue_count = summary.issue_count.saturating_add(1);
    if summary.issues.len() >= MAX_SCAN_NOTIFICATION_ISSUES {
        return;
    }

    summary.issues.push(ScanNotificationIssue {
        item_key: group.presentation.item_key.clone(),
        media_type: group.presentation.media_type.clone(),
        title: group.presentation.title.clone(),
        year: group.presentation.year,
        file_count,
        metadata_status,
        metadata_failure_reason: primary_file.and_then(|file| file.metadata_failure_reason.clone()),
        failure_detail: failure_detail.map(compact_scan_failure_detail),
        probe_warning_count,
        probe_warning_file_path: first_probe_warning
            .as_ref()
            .map(|(file_path, _)| file_path.clone()),
        probe_warning_detail: first_probe_warning.map(|(_, detail)| detail),
    });
}

fn compact_scan_failure_detail(detail: impl AsRef<str>) -> String {
    detail
        .as_ref()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(500)
        .collect()
}

fn merge_sync_outcome(
    target: &mut mova_db::SyncLibraryMediaBestEffortOutcome,
    source: mova_db::SyncLibraryMediaBestEffortOutcome,
) {
    target.removed_count += source.removed_count;
    target.upserted_count += source.upserted_count;
    target.failed_count += source.failed_count;
}

fn resolve_group_metadata_lookup_type(
    metadata_provider: &dyn MetadataProvider,
    group: &ScanDiscoveredGroup,
) -> GroupMetadataLookupDecision {
    let presentation = &group.presentation;
    let local_lookup_type = scan_presentation_lookup_type(presentation);

    if !metadata_provider.is_enabled() {
        return GroupMetadataLookupDecision {
            lookup_type: Some(local_lookup_type),
            metadata_status: METADATA_STATUS_SKIPPED,
            metadata_failure_reason: Some(METADATA_FAILURE_PROVIDER_DISABLED),
            remote_media_type: None,
        };
    }

    if let Some(decision) = existing_bound_group_lookup_decision(group) {
        return decision;
    }

    GroupMetadataLookupDecision {
        lookup_type: Some(local_lookup_type),
        metadata_status: METADATA_STATUS_PENDING,
        metadata_failure_reason: Some(METADATA_FAILURE_NO_REMOTE_MATCH),
        remote_media_type: None,
    }
}

fn existing_bound_group_lookup_decision(
    group: &ScanDiscoveredGroup,
) -> Option<GroupMetadataLookupDecision> {
    let has_remote_binding = group
        .files
        .iter()
        .any(|file| file.metadata_provider_item_id.is_some());
    if !has_remote_binding {
        return None;
    }

    let local_lookup_type = scan_presentation_lookup_type(&group.presentation);

    Some(GroupMetadataLookupDecision {
        lookup_type: Some(local_lookup_type),
        metadata_status: METADATA_STATUS_PENDING,
        metadata_failure_reason: Some(METADATA_FAILURE_NO_REMOTE_MATCH),
        remote_media_type: remote_media_type_for_lookup_type(local_lookup_type),
    })
}

fn scan_presentation_lookup_type(presentation: &ScanPresentationGroup) -> &'static str {
    if presentation.media_type.eq_ignore_ascii_case("series") {
        "series"
    } else {
        "movie"
    }
}

fn finalize_file_metadata_status(
    file: &mut DiscoveredMediaFile,
    metadata_provider_enabled: bool,
    remote_media_type: Option<&'static str>,
) {
    if !metadata_provider_enabled {
        file.remote_media_type = None;
        file.metadata_status = Some(METADATA_STATUS_SKIPPED.to_string());
        file.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_DISABLED.to_string());
        return;
    }

    if file.metadata_provider_item_id.is_some() {
        file.remote_media_type = remote_media_type.map(str::to_string);
        file.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
        file.metadata_failure_reason = None;
        return;
    }

    file.remote_media_type = None;
    file.metadata_status = Some(METADATA_STATUS_UNMATCHED.to_string());
    file.metadata_failure_reason = Some(METADATA_FAILURE_NO_REMOTE_MATCH.to_string());
}

fn remote_media_type_for_lookup_type(lookup_type: &str) -> Option<&'static str> {
    if lookup_type.eq_ignore_ascii_case("series") {
        return Some(REMOTE_MEDIA_TYPE_SERIES);
    }

    if lookup_type.eq_ignore_ascii_case("movie") {
        return Some(REMOTE_MEDIA_TYPE_MOVIE);
    }

    None
}

fn clear_remote_metadata_for_review(
    file: &mut DiscoveredMediaFile,
    metadata_status: &str,
    metadata_failure_reason: Option<&str>,
    remote_media_type: Option<&str>,
) {
    file.metadata_provider = None;
    file.metadata_provider_item_id = None;
    file.metadata_status = Some(metadata_status.to_string());
    file.metadata_failure_reason = metadata_failure_reason.map(str::to_string);
    file.remote_media_type = remote_media_type.map(str::to_string);
    if !file.source_title.trim().is_empty() {
        file.title = file.source_title.clone();
    }
    file.original_title = None;
    file.sort_title = None;
    file.external_ids.clear();
    file.ratings.clear();
    file.country = None;
    file.genres = None;
    file.studio = None;
    file.overview = None;
    file.poster_path = None;
    file.backdrop_path = None;
    file.logo_path = None;
    file.series_logo_path = None;
}

async fn build_incremental_scan_plan(
    pool: &PgPool,
    library_id: i64,
    discovered_files: Vec<DiscoveredMediaFileInventory>,
    metadata_provider_enabled: bool,
    metadata_language: &str,
) -> ApplicationResult<IncrementalScanPlan> {
    let file_paths = discovered_files
        .iter()
        .map(|file| file.file_path.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    let existing_metadata =
        mova_db::list_existing_media_metadata_for_file_paths(pool, library_id, &file_paths)
            .await
            .map_err(ApplicationError::Unexpected)?;

    let existing_by_path = existing_metadata
        .into_iter()
        .map(|summary| (summary.file_path.clone(), summary))
        .collect::<HashMap<_, _>>();

    let mut changed_files = Vec::new();

    for inventory in discovered_files {
        let file_path = inventory.file_path.to_string_lossy().to_string();
        let scan_hash = discovered_media_file_inventory_scan_hash(&inventory);

        match existing_by_path.get(file_path.as_str()) {
            Some(summary)
                if can_skip_existing_media_summary(
                    summary,
                    scan_hash.as_str(),
                    metadata_provider_enabled,
                    metadata_language,
                    &inventory.file_path,
                ) =>
            {
                continue;
            }
            existing_metadata => changed_files.push(IncrementalScanFile {
                inventory,
                existing_metadata: existing_metadata.cloned(),
            }),
        }
    }

    hydrate_incremental_scan_file_cached_tracks(pool, &mut changed_files).await;

    Ok(IncrementalScanPlan {
        discovered_paths: file_paths,
        changed_files,
    })
}

async fn hydrate_incremental_scan_file_cached_tracks(
    pool: &PgPool,
    changed_files: &mut [IncrementalScanFile],
) {
    let reusable_media_file_ids = changed_files
        .iter()
        .filter_map(|changed_file| {
            let existing_metadata = changed_file.existing_metadata.as_ref()?;
            let scan_hash = discovered_media_file_inventory_scan_hash(&changed_file.inventory);

            can_reuse_cached_local_analysis(existing_metadata, scan_hash.as_str())
                .then_some(existing_metadata.media_file_id)
        })
        .collect::<Vec<_>>();

    if reusable_media_file_ids.is_empty() {
        return;
    }
    let reusable_media_file_id_set = reusable_media_file_ids
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let audio_tracks =
        match mova_db::list_audio_tracks_for_media_files(pool, &reusable_media_file_ids).await {
            Ok(audio_tracks) => audio_tracks,
            Err(error) => {
                tracing::warn!(
                    media_file_count = reusable_media_file_ids.len(),
                    error = ?error,
                    "failed to batch-load cached audio tracks; falling back to fresh local analysis"
                );
                invalidate_reusable_local_analysis(changed_files, &reusable_media_file_id_set);
                return;
            }
        };
    let subtitle_tracks = match mova_db::list_subtitle_files_for_media_files(
        pool,
        &reusable_media_file_ids,
    )
    .await
    {
        Ok(subtitle_tracks) => subtitle_tracks,
        Err(error) => {
            tracing::warn!(
                media_file_count = reusable_media_file_ids.len(),
                error = ?error,
                "failed to batch-load cached subtitle tracks; falling back to fresh local analysis"
            );
            invalidate_reusable_local_analysis(changed_files, &reusable_media_file_id_set);
            return;
        }
    };

    let mut audio_tracks_by_media_file = HashMap::new();
    for track in audio_tracks {
        audio_tracks_by_media_file
            .entry(track.media_file_id)
            .or_insert_with(Vec::new)
            .push(track);
    }
    let mut subtitle_tracks_by_media_file = HashMap::new();
    for subtitle in subtitle_tracks {
        subtitle_tracks_by_media_file
            .entry(subtitle.media_file_id)
            .or_insert_with(Vec::new)
            .push(subtitle);
    }

    for changed_file in changed_files {
        let Some(existing_metadata) = changed_file.existing_metadata.as_mut() else {
            continue;
        };
        if !reusable_media_file_id_set.contains(&existing_metadata.media_file_id) {
            continue;
        }

        existing_metadata.audio_tracks = audio_tracks_by_media_file
            .remove(&existing_metadata.media_file_id)
            .unwrap_or_default()
            .into_iter()
            .map(|track| mova_db::CreateAudioTrackParams {
                stream_index: track.stream_index,
                language: track.language,
                audio_codec: track.audio_codec,
                label: track.label,
                channel_layout: track.channel_layout,
                channels: track.channels,
                bitrate: track.bitrate,
                sample_rate: track.sample_rate,
                is_default: track.is_default,
            })
            .collect();
        existing_metadata.subtitle_tracks = subtitle_tracks_by_media_file
            .remove(&existing_metadata.media_file_id)
            .unwrap_or_default()
            .into_iter()
            .map(|subtitle| mova_db::CreateSubtitleTrackParams {
                source_kind: subtitle.source_kind,
                file_path: subtitle.file_path,
                stream_index: subtitle.stream_index,
                language: subtitle.language,
                subtitle_format: subtitle.subtitle_format,
                label: subtitle.label,
                is_default: subtitle.is_default,
                is_forced: subtitle.is_forced,
                is_hearing_impaired: subtitle.is_hearing_impaired,
            })
            .collect();
    }
}

fn invalidate_reusable_local_analysis(
    changed_files: &mut [IncrementalScanFile],
    reusable_media_file_ids: &HashSet<i64>,
) {
    for changed_file in changed_files {
        if changed_file
            .existing_metadata
            .as_ref()
            .is_some_and(|metadata| reusable_media_file_ids.contains(&metadata.media_file_id))
        {
            changed_file.existing_metadata = None;
        }
    }
}

fn can_skip_existing_media_summary(
    summary: &mova_db::ExistingMediaMetadataSummary,
    scan_hash: &str,
    metadata_provider_enabled: bool,
    metadata_language: &str,
    file_path: &std::path::Path,
) -> bool {
    if summary.scan_hash.as_deref() != Some(scan_hash) {
        return false;
    }

    if summary.local_analysis_version != LOCAL_ANALYSIS_VERSION {
        return false;
    }

    !should_rescan_unchanged_existing_media_summary(
        summary,
        metadata_provider_enabled,
        metadata_language,
        file_path,
    )
}

fn should_rescan_unchanged_existing_media_summary(
    summary: &mova_db::ExistingMediaMetadataSummary,
    metadata_provider_enabled: bool,
    _metadata_language: &str,
    file_path: &std::path::Path,
) -> bool {
    if is_existing_summary_in_other_review_section(summary) {
        return true;
    }

    if should_retry_review_metadata_status(summary) {
        return true;
    }

    if metadata_provider_enabled && should_retry_incomplete_remote_match(summary) {
        return true;
    }

    if metadata_provider_enabled && should_retry_local_series_title_override(summary, file_path) {
        return true;
    }

    if metadata_provider_enabled && should_retry_external_cached_artwork(summary) {
        return true;
    }

    false
}

fn can_reuse_cached_local_analysis(
    summary: &mova_db::ExistingMediaMetadataSummary,
    scan_hash: &str,
) -> bool {
    summary.scan_hash.as_deref() == Some(scan_hash)
        && summary.local_analysis_version == LOCAL_ANALYSIS_VERSION
}

fn should_retry_incomplete_remote_match(summary: &mova_db::ExistingMediaMetadataSummary) -> bool {
    if summary.metadata_status != METADATA_STATUS_MATCHED {
        return true;
    }

    effective_existing_metadata_provider_item_id(summary).is_none()
        || !effective_existing_metadata_provider(summary)
            .is_some_and(|value| value.eq_ignore_ascii_case(TMDB_PROVIDER_NAME))
}

fn should_retry_review_metadata_status(summary: &mova_db::ExistingMediaMetadataSummary) -> bool {
    matches!(
        summary.metadata_status.as_str(),
        METADATA_STATUS_PENDING | METADATA_STATUS_UNMATCHED | METADATA_STATUS_FAILED
    )
}

fn is_existing_summary_in_other_review_section(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> bool {
    matches!(
        summary.metadata_status.as_str(),
        METADATA_STATUS_SKIPPED | METADATA_STATUS_UNMATCHED | METADATA_STATUS_FAILED
    ) && !has_existing_remote_enrichment(summary)
}

fn has_existing_remote_enrichment(summary: &mova_db::ExistingMediaMetadataSummary) -> bool {
    effective_existing_metadata_provider_item_id(summary).is_some()
        || existing_text_values(summary).any(has_text)
        || existing_artwork_paths(summary).any(has_text)
}

fn existing_text_values(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> impl Iterator<Item = &str> {
    [
        summary.original_title.as_deref(),
        summary.overview.as_deref(),
        summary.series_original_title.as_deref(),
        summary.series_overview.as_deref(),
    ]
    .into_iter()
    .flatten()
}

fn has_text(value: &str) -> bool {
    !value.trim().is_empty()
}

fn effective_existing_metadata_provider(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> Option<&str> {
    if summary.media_type.eq_ignore_ascii_case("episode") {
        return summary
            .series_metadata_provider
            .as_deref()
            .or(summary.metadata_provider.as_deref());
    }

    summary.metadata_provider.as_deref()
}

fn effective_existing_metadata_provider_item_id(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> Option<i64> {
    if summary.media_type.eq_ignore_ascii_case("episode") {
        return summary
            .series_metadata_provider_item_id
            .or(summary.metadata_provider_item_id);
    }

    summary.metadata_provider_item_id
}

fn should_retry_external_cached_artwork(summary: &mova_db::ExistingMediaMetadataSummary) -> bool {
    if summary.metadata_status != METADATA_STATUS_MATCHED {
        return false;
    }

    existing_artwork_paths(summary).any(is_external_artwork_path)
}

fn existing_artwork_paths(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> impl Iterator<Item = &str> {
    [
        summary.poster_path.as_deref(),
        summary.backdrop_path.as_deref(),
        summary.series_poster_path.as_deref(),
        summary.series_backdrop_path.as_deref(),
        summary.series_logo_path.as_deref(),
        summary.season_poster_path.as_deref(),
        summary.season_backdrop_path.as_deref(),
        summary.logo_path.as_deref(),
    ]
    .into_iter()
    .flatten()
}

fn is_external_artwork_path(path: &str) -> bool {
    let path = path.trim();
    path.starts_with("http://") || path.starts_with("https://")
}

fn should_retry_local_series_title_override(
    summary: &mova_db::ExistingMediaMetadataSummary,
    file_path: &std::path::Path,
) -> bool {
    if summary.metadata_status != METADATA_STATUS_MATCHED {
        return false;
    }

    if !summary.media_type.eq_ignore_ascii_case("episode")
        && !summary
            .remote_media_type
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(REMOTE_MEDIA_TYPE_SERIES))
    {
        return false;
    }

    if effective_existing_metadata_provider_item_id(summary).is_none() {
        return false;
    }

    let Some(file_metadata) = infer_series_file_metadata(file_path) else {
        return false;
    };
    let current_title = summary
        .series_title
        .as_deref()
        .unwrap_or(summary.title.as_str())
        .trim();
    let local_display_title = file_metadata.display_title.trim();
    let local_lookup_title = file_metadata.title.trim();

    !local_display_title.eq_ignore_ascii_case(local_lookup_title)
        && current_title.eq_ignore_ascii_case(local_display_title)
}

async fn inspect_incremental_scan_files(
    changed_files: Vec<IncrementalScanFile>,
) -> ApplicationResult<Vec<DiscoveredMediaFile>> {
    tokio::task::spawn_blocking(move || {
        let mut discovered_files = Vec::with_capacity(changed_files.len());
        let mut series_sidecars =
            HashMap::<String, Option<mova_scan::SeriesSidecarMetadata>>::new();

        for changed_file in changed_files {
            let file_path = changed_file.inventory.file_path.display().to_string();
            if let Some(existing_metadata) = changed_file.existing_metadata.as_ref() {
                let scan_hash = discovered_media_file_inventory_scan_hash(&changed_file.inventory);
                if can_reuse_cached_local_analysis(existing_metadata, scan_hash.as_str()) {
                    let mut discovered_file = discovered_file_from_existing_local_analysis(
                        &changed_file.inventory,
                        existing_metadata,
                    )?;
                    populate_series_sidecar_metadata(&mut discovered_file, &mut series_sidecars);
                    discovered_files.push(discovered_file);
                    continue;
                }
            }

            let mut discovered_file = mova_scan::inspect_media_file_inventory(
                changed_file.inventory,
            )
            .map_err(|error| {
                ApplicationError::Unexpected(anyhow::anyhow!(
                    "Unable to inspect changed media file {}: {}",
                    file_path,
                    error
                ))
            })?;

            if let Some(existing_metadata) = changed_file.existing_metadata.as_ref() {
                apply_existing_media_metadata(&mut discovered_file, existing_metadata);
            }

            populate_series_sidecar_metadata(&mut discovered_file, &mut series_sidecars);
            discovered_files.push(discovered_file);
        }

        Ok(discovered_files)
    })
    .await
    .map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "The changed media inspection worker exited unexpectedly: {}",
            error
        ))
    })?
}

fn populate_series_sidecar_metadata(
    file: &mut DiscoveredMediaFile,
    cache: &mut HashMap<String, Option<mova_scan::SeriesSidecarMetadata>>,
) {
    if file.season_number.is_none() || file.episode_number.is_none() {
        return;
    }

    let cache_key = series_container_item_key(&file.file_path).unwrap_or_else(|| {
        file.file_path
            .parent()
            .unwrap_or(file.file_path.as_path())
            .to_string_lossy()
            .to_string()
    });
    let metadata = cache
        .entry(cache_key)
        .or_insert_with(|| infer_series_sidecar_metadata(&file.file_path));

    file.series_sidecar_title = metadata
        .as_ref()
        .and_then(|metadata| metadata.title.clone());
    file.series_sidecar_year = metadata.as_ref().and_then(|metadata| metadata.year);
}

async fn inspect_incremental_scan_files_shallow(
    changed_files: Vec<IncrementalScanFile>,
) -> ApplicationResult<Vec<PendingScanFile>> {
    tokio::task::spawn_blocking(move || {
        let mut pending_files = Vec::with_capacity(changed_files.len());

        for changed_file in changed_files {
            let file_path = changed_file.inventory.file_path.display().to_string();
            let file =
                mova_scan::inspect_media_file_inventory_shallow(changed_file.inventory.clone())
                    .map_err(|error| {
                        ApplicationError::Unexpected(anyhow::anyhow!(
                            "Unable to inspect changed media file {}: {}",
                            file_path,
                            error
                        ))
                    })?;

            pending_files.push(PendingScanFile { changed_file, file });
        }

        Ok(pending_files)
    })
    .await
    .map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "The shallow media inspection worker exited unexpectedly: {}",
            error
        ))
    })?
}

fn discovered_file_from_existing_local_analysis(
    inventory: &DiscoveredMediaFileInventory,
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> ApplicationResult<DiscoveredMediaFile> {
    let file_size = u64::try_from(summary.file_size).map_err(|_| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "stored media file size is invalid: {}",
            summary.file_path
        ))
    })?;
    let (
        title,
        source_title,
        original_title,
        sort_title,
        year,
        country,
        genres,
        studio,
        overview,
        poster_path,
        backdrop_path,
    ) = if summary.media_type.eq_ignore_ascii_case("episode") {
        (
            summary
                .series_title
                .as_ref()
                .cloned()
                .unwrap_or_else(|| summary.title.clone()),
            summary
                .series_source_title
                .as_ref()
                .cloned()
                .unwrap_or_else(|| summary.source_title.clone()),
            summary.series_original_title.clone(),
            summary.series_sort_title.clone(),
            summary.series_year,
            summary.series_country.clone(),
            summary.series_genres.clone(),
            summary.series_studio.clone(),
            summary
                .series_overview
                .clone()
                .or_else(|| summary.overview.clone()),
            summary.poster_path.clone(),
            summary.backdrop_path.clone(),
        )
    } else {
        (
            summary.title.clone(),
            summary.source_title.clone(),
            summary.original_title.clone(),
            summary.sort_title.clone(),
            summary.year,
            summary.country.clone(),
            summary.genres.clone(),
            summary.studio.clone(),
            summary.overview.clone(),
            summary.poster_path.clone(),
            summary.backdrop_path.clone(),
        )
    };

    Ok(DiscoveredMediaFile {
        file_path: inventory.file_path.clone(),
        file_modified_at_ms: inventory.file_modified_at_ms,
        probe_error: None,
        metadata_provider: effective_existing_metadata_provider(summary).map(str::to_string),
        metadata_provider_item_id: effective_existing_metadata_provider_item_id(summary),
        title,
        source_title,
        original_title,
        sort_title,
        series_sidecar_title: None,
        series_sidecar_year: None,
        year,
        external_ids: Vec::new(),
        ratings: Vec::new(),
        metadata_status: Some(summary.metadata_status.clone()),
        metadata_failure_reason: summary.metadata_failure_reason.clone(),
        remote_media_type: summary.remote_media_type.clone(),
        country,
        genres,
        studio,
        season_number: summary.season_number,
        season_title: summary.season_title.clone(),
        season_overview: summary.season_overview.clone(),
        season_poster_path: summary.season_poster_path.clone(),
        season_backdrop_path: summary.season_backdrop_path.clone(),
        episode_number: summary.episode_number,
        episode_title: summary.episode_title.clone(),
        overview,
        series_poster_path: summary.series_poster_path.clone(),
        series_backdrop_path: summary.series_backdrop_path.clone(),
        series_logo_path: summary.series_logo_path.clone(),
        poster_path,
        backdrop_path,
        logo_path: summary.logo_path.clone(),
        file_size: inventory.file_size.max(file_size),
        container: summary.container.clone(),
        duration_seconds: summary.duration_seconds,
        video_title: summary.video_title.clone(),
        video_codec: summary.video_codec.clone(),
        video_profile: summary.video_profile.clone(),
        video_level: summary.video_level.clone(),
        audio_codec: summary.audio_codec.clone(),
        width: summary.width,
        height: summary.height,
        bitrate: summary.bitrate,
        video_bitrate: summary.video_bitrate,
        video_frame_rate: summary.video_frame_rate,
        video_aspect_ratio: summary.video_aspect_ratio.clone(),
        video_scan_type: summary.video_scan_type.clone(),
        video_color_primaries: summary.video_color_primaries.clone(),
        video_color_space: summary.video_color_space.clone(),
        video_color_transfer: summary.video_color_transfer.clone(),
        video_bit_depth: summary.video_bit_depth,
        video_pixel_format: summary.video_pixel_format.clone(),
        video_reference_frames: summary.video_reference_frames,
        technical_tags: summary.technical_tags.clone(),
        audio_tracks: summary
            .audio_tracks
            .iter()
            .map(|track| DiscoveredAudioTrack {
                stream_index: track.stream_index,
                language: track.language.clone(),
                audio_codec: track.audio_codec.clone(),
                label: track.label.clone(),
                channel_layout: track.channel_layout.clone(),
                channels: track.channels,
                bitrate: track.bitrate,
                sample_rate: track.sample_rate,
                is_default: track.is_default,
            })
            .collect(),
        subtitle_tracks: summary
            .subtitle_tracks
            .iter()
            .map(|subtitle| DiscoveredSubtitleTrack {
                source_kind: subtitle.source_kind.clone(),
                file_path: subtitle.file_path.as_ref().map(PathBuf::from),
                stream_index: subtitle.stream_index,
                language: subtitle.language.clone(),
                subtitle_format: subtitle.subtitle_format.clone(),
                label: subtitle.label.clone(),
                is_default: subtitle.is_default,
                is_forced: subtitle.is_forced,
                is_hearing_impaired: subtitle.is_hearing_impaired,
            })
            .collect(),
    })
}

async fn discover_media_files(
    pool: &PgPool,
    scan_job_id: i64,
    library: &Library,
    cancellation_flag: Arc<AtomicBool>,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> ApplicationResult<DiscoverMediaFilesOutcome> {
    let root_path = library.root_path.as_str();
    let root_path_string = root_path.to_string();
    let root_path_for_task = root_path_string.clone();
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<i32>();
    let progress_pool = pool.clone();
    let last_progress = Arc::new(AtomicI32::new(0));
    let last_progress_for_task = last_progress.clone();
    let progress_event_listener = event_listener.clone();

    let progress_task = tokio::spawn(async move {
        let mut persisted_progress = 0;
        let mut pending_progress = 0;
        let mut last_flush_at: Option<Instant> = None;

        while let Some(scanned_files) = progress_rx.recv().await {
            if scanned_files <= pending_progress {
                continue;
            }

            pending_progress = scanned_files;
            let now = Instant::now();

            if !should_flush_discovery_progress(
                persisted_progress,
                pending_progress,
                last_flush_at,
                now,
            ) {
                continue;
            }

            if let Some(flushed_progress) = flush_discovery_progress(
                &progress_pool,
                scan_job_id,
                pending_progress,
                &progress_event_listener,
            )
            .await
            {
                persisted_progress = flushed_progress;
                last_flush_at = Some(now);
                last_progress_for_task.store(flushed_progress, Ordering::SeqCst);
            }
        }

        if pending_progress > persisted_progress {
            if let Some(flushed_progress) = flush_discovery_progress(
                &progress_pool,
                scan_job_id,
                pending_progress,
                &progress_event_listener,
            )
            .await
            {
                last_progress_for_task.store(flushed_progress, Ordering::SeqCst);
            }
        }
    });

    let cancellation_for_task = cancellation_flag.clone();
    let result = tokio::task::spawn_blocking(move || {
        mova_scan::discover_media_file_inventory_with_progress_and_cancel(
            std::path::Path::new(&root_path_for_task),
            |count| {
                let _ = progress_tx.send(count as i32);
            },
            || cancellation_for_task.load(Ordering::SeqCst),
        )
    })
    .await
    .map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "The file discovery worker exited unexpectedly ({}): {}",
            root_path_string,
            error
        ))
    })?;

    let _ = progress_task.await;

    match result {
        Ok(files) => Ok(DiscoverMediaFilesOutcome::Completed(files)),
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => Ok(
            DiscoverMediaFilesOutcome::Cancelled(last_progress.load(Ordering::SeqCst)),
        ),
        Err(error) => Err(ApplicationError::Unexpected(anyhow::anyhow!(
            "Unable to read library directory {}: {}",
            root_path,
            error
        ))),
    }
}

fn build_media_entries(
    library: &Library,
    discovered_files: Vec<DiscoveredMediaFile>,
    allow_artwork_clear: bool,
) -> ApplicationResult<Vec<mova_db::CreateMediaEntryParams>> {
    let discovered_files =
        normalize_discovered_files_for_local_structure(library, discovered_files);
    let mut entries = Vec::new();
    let mut remote_data_pending = allow_artwork_clear;

    for file in discovered_files {
        let media_type = effective_media_type(&library.library_type, &file).to_string();
        if media_type == "episode"
            && (file.season_number.is_none() || file.episode_number.is_none())
        {
            tracing::warn!(
                file_path = %file.file_path.display(),
                library_id = library.id,
                "skipping episode-like file because season/episode number could not be parsed"
            );
            continue;
        }

        let file_path = file.file_path.to_string_lossy().to_string();
        let file_size = i64::try_from(file.file_size).map_err(|_| {
            ApplicationError::Unexpected(anyhow::anyhow!(
                "file is too large to store in database: {}",
                file_path
            ))
        })?;
        let scan_hash = discovered_media_file_scan_hash(&file);
        let metadata_status = file.metadata_status.ok_or_else(|| {
            ApplicationError::Unexpected(anyhow::anyhow!(
                "metadata status was not finalized before sync: {}",
                file_path
            ))
        })?;
        let entry_allow_artwork_clear =
            allow_artwork_clear && metadata_status.eq_ignore_ascii_case(METADATA_STATUS_MATCHED);
        let replace_remote_data = remote_data_pending
            && (metadata_status.eq_ignore_ascii_case(METADATA_STATUS_MATCHED)
                || metadata_status.eq_ignore_ascii_case(METADATA_STATUS_UNMATCHED));
        if replace_remote_data {
            remote_data_pending = false;
        }

        entries.push(mova_db::CreateMediaEntryParams {
            library_id: library.id,
            media_type,
            metadata_provider: file.metadata_provider,
            metadata_provider_item_id: file.metadata_provider_item_id,
            metadata_status,
            metadata_failure_reason: file.metadata_failure_reason,
            allow_artwork_clear: entry_allow_artwork_clear,
            replace_remote_data,
            remote_media_type: file.remote_media_type,
            title: file.title,
            source_title: file.source_title,
            original_title: file.original_title,
            sort_title: file.sort_title,
            year: file.year,
            external_ids: file.external_ids,
            ratings: file.ratings,
            country: file.country,
            genres: file.genres,
            studio: file.studio,
            season_number: file.season_number,
            season_title: file.season_title,
            season_overview: file.season_overview,
            season_poster_path: file.season_poster_path,
            season_backdrop_path: file.season_backdrop_path,
            episode_number: file.episode_number,
            episode_title: file.episode_title,
            overview: file.overview,
            series_poster_path: file.series_poster_path,
            series_backdrop_path: file.series_backdrop_path,
            series_logo_path: file.series_logo_path,
            poster_path: file.poster_path,
            backdrop_path: file.backdrop_path,
            logo_path: file.logo_path,
            file_path,
            container: file.container,
            file_size,
            duration_seconds: file.duration_seconds,
            video_title: file.video_title,
            video_codec: file.video_codec,
            video_profile: file.video_profile,
            video_level: file.video_level,
            audio_codec: file.audio_codec,
            width: file.width,
            height: file.height,
            bitrate: file.bitrate,
            video_bitrate: file.video_bitrate,
            video_frame_rate: file.video_frame_rate,
            video_aspect_ratio: file.video_aspect_ratio,
            video_scan_type: file.video_scan_type,
            video_color_primaries: file.video_color_primaries,
            video_color_space: file.video_color_space,
            video_color_transfer: file.video_color_transfer,
            video_bit_depth: file.video_bit_depth,
            video_pixel_format: file.video_pixel_format,
            video_reference_frames: file.video_reference_frames,
            technical_tags: file.technical_tags,
            audio_tracks: file
                .audio_tracks
                .into_iter()
                .map(|audio_track| mova_db::CreateAudioTrackParams {
                    stream_index: audio_track.stream_index,
                    language: audio_track.language,
                    audio_codec: audio_track.audio_codec,
                    label: audio_track.label,
                    channel_layout: audio_track.channel_layout,
                    channels: audio_track.channels,
                    bitrate: audio_track.bitrate,
                    sample_rate: audio_track.sample_rate,
                    is_default: audio_track.is_default,
                })
                .collect(),
            // 全量扫库时同样带上已经解析好的字幕轨道，后续播放器直接从数据库读取即可。
            subtitle_tracks: file
                .subtitle_tracks
                .into_iter()
                .map(|subtitle| mova_db::CreateSubtitleTrackParams {
                    source_kind: subtitle.source_kind,
                    file_path: subtitle
                        .file_path
                        .map(|path| path.to_string_lossy().to_string()),
                    stream_index: subtitle.stream_index,
                    language: subtitle.language,
                    subtitle_format: subtitle.subtitle_format,
                    label: subtitle.label,
                    is_default: subtitle.is_default,
                    is_forced: subtitle.is_forced,
                    is_hearing_impaired: subtitle.is_hearing_impaired,
                })
                .collect(),
            local_analysis_version: LOCAL_ANALYSIS_VERSION,
            scan_hash: Some(scan_hash),
        });
    }

    Ok(entries)
}

fn normalize_discovered_files_for_local_structure(
    library: &Library,
    mut discovered_files: Vec<DiscoveredMediaFile>,
) -> Vec<DiscoveredMediaFile> {
    discovered_files.sort_by(|left, right| left.file_path.cmp(&right.file_path));

    let mut groups = HashMap::<String, LocalSeriesGroup>::new();
    for (index, file) in discovered_files.iter().enumerate() {
        if file.season_number.is_none() || file.episode_number.is_none() {
            continue;
        }

        let Some(group_seed) = local_series_group_seed_for_file(file) else {
            continue;
        };

        let group = groups
            .entry(group_seed.item_key.clone())
            .or_insert_with(|| LocalSeriesGroup {
                lookup_title: group_seed.lookup_title.clone(),
                display_title: group_seed.display_title.clone(),
                year: group_seed.year,
                year_priority: group_seed.year_priority,
                identity_from_sidecar: group_seed.identity_from_sidecar,
                identity_season_number: group_seed.season_number,
                has_first_season: group_seed.season_number == 1,
                season_air_year: group_seed.season_air_year,
                file_indexes: Vec::new(),
                classified_episode_count: 0,
            });

        apply_local_series_group_seed(group, &group_seed);
        group.file_indexes.push(index);

        if file.season_number.is_some() && file.episode_number.is_some()
            || classify_media_type(&library.library_type, &file.file_path)
                .eq_ignore_ascii_case("episode")
        {
            group.classified_episode_count += 1;
        }
    }

    for mut group in groups.into_values() {
        let should_promote_to_series = should_promote_local_series_group(&group);

        if !should_promote_to_series {
            continue;
        }

        if group.year.is_some() || group.has_first_season {
            group.season_air_year = None;
        }

        assign_local_series_structure(&mut discovered_files, &group);
    }

    discovered_files
}

#[derive(Debug, Clone)]
struct LocalSeriesGroupSeed {
    item_key: String,
    lookup_title: String,
    display_title: String,
    year: Option<i32>,
    year_priority: u8,
    identity_from_sidecar: bool,
    season_number: i32,
    season_air_year: Option<MetadataSeasonAirYearHint>,
}

fn local_series_group_seed_for_file(file: &DiscoveredMediaFile) -> Option<LocalSeriesGroupSeed> {
    if file.season_number.is_some() && file.episode_number.is_some() {
        let file_metadata = infer_series_file_metadata(&file.file_path);
        let sidecar_title = file
            .series_sidecar_title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty());
        let fallback_title = file_metadata
            .as_ref()
            .map(|metadata| metadata.title.as_str());
        let lookup_title = sidecar_title.or(fallback_title)?.to_string();
        let display_title = sidecar_title
            .map(str::to_string)
            .or_else(|| {
                file_metadata
                    .as_ref()
                    .map(|metadata| metadata.display_title.clone())
            })
            .unwrap_or_else(|| lookup_title.clone());
        let season_number = file_metadata
            .as_ref()
            .map(|metadata| metadata.season_number)
            .or(file.season_number)?;
        let sidecar_year = file.series_sidecar_year;
        let file_first_air_year = file_metadata.as_ref().and_then(|metadata| metadata.year);
        let year = sidecar_year.or(file_first_air_year);
        let year_priority = if sidecar_year.is_some() {
            2
        } else if season_number == 1 && file_first_air_year.is_some() {
            1
        } else {
            0
        };
        let season_air_year = sidecar_year
            .is_none()
            .then(|| {
                file_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.season_air_year)
                    .map(|year| MetadataSeasonAirYearHint {
                        season_number,
                        year,
                    })
            })
            .flatten();

        return Some(LocalSeriesGroupSeed {
            item_key: series_group_item_key(&file.file_path, &lookup_title),
            lookup_title,
            display_title,
            year,
            year_priority,
            identity_from_sidecar: sidecar_title.is_some(),
            season_number,
            season_air_year,
        });
    }

    None
}

fn apply_local_series_group_seed(group: &mut LocalSeriesGroup, group_seed: &LocalSeriesGroupSeed) {
    group.has_first_season |= group_seed.season_number == 1;

    if group_seed.year_priority > group.year_priority {
        group.year = group_seed.year;
        group.year_priority = group_seed.year_priority;
    }

    if let Some(candidate) = group_seed.season_air_year {
        let should_replace_season_hint = group
            .season_air_year
            .is_none_or(|current| candidate.season_number < current.season_number);
        if should_replace_season_hint {
            group.season_air_year = Some(candidate);
        }
    }

    let should_replace_identity = (group_seed.identity_from_sidecar
        && !group.identity_from_sidecar)
        || (group_seed.identity_from_sidecar == group.identity_from_sidecar
            && group_seed.season_number < group.identity_season_number);

    if should_replace_identity {
        group.lookup_title = group_seed.lookup_title.clone();
        group.display_title = group_seed.display_title.clone();
        group.identity_from_sidecar = group_seed.identity_from_sidecar;
        group.identity_season_number = group_seed.season_number;
    }
}

fn series_group_item_key(file_path: &std::path::Path, title: &str) -> String {
    series_container_item_key(file_path).unwrap_or_else(|| series_title_item_key(title))
}

fn series_title_item_key(title: &str) -> String {
    let normalized_title = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    format!("series-title:{normalized_title}")
}

fn series_container_item_key(file_path: &std::path::Path) -> Option<String> {
    let parent = file_path.parent()?;
    let mut directories = parent
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|component| !component.trim().is_empty())
        .collect::<Vec<_>>();

    while directories
        .last()
        .is_some_and(|directory| is_series_variant_directory_name(directory))
    {
        directories.pop();
    }

    let season_directory_index = directories
        .iter()
        .rposition(|directory| is_season_directory_name(directory))?;

    if season_directory_index == 0 {
        return None;
    }

    let container_key = directories[..season_directory_index]
        .iter()
        .map(|component| normalize_series_key_component(component))
        .collect::<Vec<_>>()
        .join("/");

    (!container_key.is_empty()).then(|| format!("series-folder:{container_key}"))
}

fn normalize_series_key_component(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn is_series_variant_directory_name(name: &str) -> bool {
    let normalized = name
        .trim()
        .replace(['.', '_', '-', '—', '–'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    matches!(
        normalized.as_str(),
        "dv" | "dovi" | "dolby vision" | "hdr" | "hdr10" | "hdr10+" | "sdr"
    ) || normalized.contains("杜比")
}

fn is_season_directory_name(name: &str) -> bool {
    let normalized = name.trim().replace(['.', '_', '-', '—', '–'], " ");
    let normalized_lower = normalized.to_ascii_lowercase();
    let has_ascii_digit = normalized_lower.chars().any(|value| value.is_ascii_digit());

    if has_ascii_digit && normalized_lower.contains("season") {
        return true;
    }

    if has_ascii_digit && normalized.contains('季') {
        return true;
    }

    normalized_lower.split_whitespace().any(|token| {
        token.strip_prefix('s').is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix.len() <= 2
                && suffix.chars().all(|value| value.is_ascii_digit())
        })
    })
}

fn should_promote_local_series_group(group: &LocalSeriesGroup) -> bool {
    group.classified_episode_count > 0
}

fn assign_local_series_structure(
    discovered_files: &mut [DiscoveredMediaFile],
    group: &LocalSeriesGroup,
) {
    let mut season_episode_indexes = HashMap::<i32, Vec<usize>>::new();

    for index in &group.file_indexes {
        let file = &mut discovered_files[*index];
        file.source_title = group.lookup_title.clone();
        if should_use_local_series_display_metadata(file) {
            file.title = group.display_title.clone();
            file.year = group.year;
        } else if file.year.is_none() {
            file.year = group.year;
        }

        let season_number = file.season_number.unwrap_or(1);
        file.season_number = Some(season_number);
        season_episode_indexes
            .entry(season_number)
            .or_default()
            .push(*index);
    }

    for indexes in season_episode_indexes.values_mut() {
        indexes.sort_by(|left, right| {
            discovered_files[*left]
                .file_path
                .cmp(&discovered_files[*right].file_path)
        });

        let mut next_episode_number = 1;
        let mut used_episode_numbers = HashSet::<i32>::new();

        for index in indexes.iter().copied() {
            if let Some(existing) = discovered_files[index].episode_number {
                used_episode_numbers.insert(existing);
                if existing >= next_episode_number {
                    next_episode_number = existing + 1;
                }
            }
        }

        for index in indexes.iter().copied() {
            let file = &mut discovered_files[index];

            if file.episode_number.is_none() {
                while used_episode_numbers.contains(&next_episode_number) {
                    next_episode_number += 1;
                }

                file.episode_number = Some(next_episode_number);
                used_episode_numbers.insert(next_episode_number);
                next_episode_number += 1;
            }

            if file.episode_title.is_none() {
                file.episode_title = Some(
                    file.file_path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("Episode")
                        .replace(['.', '_'], " "),
                );
            }
        }
    }
}

fn should_use_local_series_display_metadata(file: &DiscoveredMediaFile) -> bool {
    file.metadata_provider_item_id.is_none()
}

fn effective_media_type(library_type: &str, file: &DiscoveredMediaFile) -> &'static str {
    if file.season_number.is_some() && file.episode_number.is_some() {
        "episode"
    } else {
        classify_media_type(library_type, &file.file_path)
    }
}

fn apply_existing_media_metadata(
    file: &mut DiscoveredMediaFile,
    summary: &mova_db::ExistingMediaMetadataSummary,
) {
    if summary.metadata_status != METADATA_STATUS_MATCHED
        || effective_existing_metadata_provider_item_id(summary).is_none()
    {
        return;
    }

    if summary.media_type.eq_ignore_ascii_case("episode") {
        replace_option_if_present(
            &mut file.metadata_provider,
            summary.metadata_provider.as_ref(),
        );
        replace_copy_if_present(
            &mut file.metadata_provider_item_id,
            summary.metadata_provider_item_id,
        );
        replace_string_option_if_present(
            &mut file.metadata_status,
            Some(summary.metadata_status.as_str()),
        );
        replace_string_option_if_present(
            &mut file.metadata_failure_reason,
            summary.metadata_failure_reason.as_deref(),
        );
        replace_string_option_if_present(
            &mut file.remote_media_type,
            summary.remote_media_type.as_deref(),
        );
        replace_string_if_present(&mut file.title, summary.series_title.as_deref());
        fill_string_if_missing(
            &mut file.source_title,
            summary.series_source_title.as_deref(),
        );
        replace_option_if_present(
            &mut file.original_title,
            summary.series_original_title.as_ref(),
        );
        replace_option_if_present(&mut file.sort_title, summary.series_sort_title.as_ref());
        replace_copy_if_present(&mut file.year, summary.series_year);
        replace_option_if_present(&mut file.country, summary.series_country.as_ref());
        replace_option_if_present(&mut file.genres, summary.series_genres.as_ref());
        replace_option_if_present(&mut file.studio, summary.series_studio.as_ref());
        replace_option_if_present(&mut file.overview, summary.series_overview.as_ref());
        fill_option_ref_if_missing(
            &mut file.series_poster_path,
            summary.series_poster_path.as_ref(),
        );
        fill_option_ref_if_missing(
            &mut file.series_backdrop_path,
            summary.series_backdrop_path.as_ref(),
        );
        fill_option_ref_if_missing(
            &mut file.series_logo_path,
            summary.series_logo_path.as_ref(),
        );
        fill_option_ref_if_missing(&mut file.season_title, summary.season_title.as_ref());
        fill_option_ref_if_missing(&mut file.season_overview, summary.season_overview.as_ref());
        fill_option_ref_if_missing(
            &mut file.season_poster_path,
            summary.season_poster_path.as_ref(),
        );
        fill_option_ref_if_missing(
            &mut file.season_backdrop_path,
            summary.season_backdrop_path.as_ref(),
        );
        replace_option_if_present(&mut file.episode_title, summary.episode_title.as_ref());
        fill_option_ref_if_missing(&mut file.poster_path, summary.poster_path.as_ref());
        fill_option_ref_if_missing(&mut file.backdrop_path, summary.backdrop_path.as_ref());
        fill_option_ref_if_missing(&mut file.logo_path, summary.logo_path.as_ref());
        return;
    }

    replace_option_if_present(
        &mut file.metadata_provider,
        summary.metadata_provider.as_ref(),
    );
    replace_copy_if_present(
        &mut file.metadata_provider_item_id,
        summary.metadata_provider_item_id,
    );
    replace_string_option_if_present(
        &mut file.metadata_status,
        Some(summary.metadata_status.as_str()),
    );
    replace_string_option_if_present(
        &mut file.metadata_failure_reason,
        summary.metadata_failure_reason.as_deref(),
    );
    replace_string_option_if_present(
        &mut file.remote_media_type,
        summary.remote_media_type.as_deref(),
    );
    replace_string_if_present(&mut file.title, Some(summary.title.as_str()));
    fill_string_if_missing(&mut file.source_title, Some(summary.source_title.as_str()));
    replace_option_if_present(&mut file.original_title, summary.original_title.as_ref());
    replace_option_if_present(&mut file.sort_title, summary.sort_title.as_ref());
    replace_copy_if_present(&mut file.year, summary.year);
    replace_option_if_present(&mut file.country, summary.country.as_ref());
    replace_option_if_present(&mut file.genres, summary.genres.as_ref());
    replace_option_if_present(&mut file.studio, summary.studio.as_ref());
    replace_option_if_present(&mut file.overview, summary.overview.as_ref());
    fill_option_ref_if_missing(&mut file.poster_path, summary.poster_path.as_ref());
    fill_option_ref_if_missing(&mut file.backdrop_path, summary.backdrop_path.as_ref());
    fill_option_ref_if_missing(&mut file.logo_path, summary.logo_path.as_ref());
}

fn replace_string_if_present(target: &mut String, candidate: Option<&str>) {
    let Some(candidate) = candidate.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };

    target.clear();
    target.push_str(candidate);
}

fn fill_string_if_missing(target: &mut String, candidate: Option<&str>) {
    if !target.trim().is_empty() {
        return;
    }

    replace_string_if_present(target, candidate);
}

fn replace_string_option_if_present(target: &mut Option<String>, candidate: Option<&str>) {
    let Some(candidate) = candidate.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };

    *target = Some(candidate.to_string());
}

fn fill_option_ref_if_missing<T: Clone>(target: &mut Option<T>, candidate: Option<&T>) {
    if target.is_some() {
        return;
    }

    *target = candidate.cloned();
}

fn replace_option_if_present<T: Clone>(target: &mut Option<T>, candidate: Option<&T>) {
    if let Some(candidate) = candidate {
        *target = Some(candidate.clone());
    }
}

fn replace_copy_if_present<T: Copy>(target: &mut Option<T>, candidate: Option<T>) {
    *target = candidate;
}

fn is_cancelled(cancellation_flag: &Arc<AtomicBool>) -> bool {
    cancellation_flag.load(Ordering::SeqCst)
}

fn build_scan_job_progress_update(scan_job: ScanJob, phase: &str) -> ScanJobProgressUpdate {
    ScanJobProgressUpdate {
        scan_job,
        phase: Some(phase.to_string()),
    }
}

fn format_scan_phase_error(phase: &str, detail: impl AsRef<str>) -> String {
    format!("{}: {}", scan_phase_label(phase), detail.as_ref())
}

fn scan_phase_label(phase: &str) -> &'static str {
    match phase {
        SCAN_PHASE_INITIALIZING => "Initialization failed",
        SCAN_PHASE_DISCOVERING => "Directory scan failed",
        SCAN_PHASE_PROCESSING => "Media processing failed",
        SCAN_PHASE_FINALIZING => "Library finalization failed",
        SCAN_PHASE_FINISHED => "Finalization failed",
        _ => "Scan job failed",
    }
}

fn build_scan_presentation_group(
    library_type: &str,
    file: &DiscoveredMediaFile,
) -> ScanPresentationGroup {
    let media_type = effective_media_type(library_type, file);

    if media_type == "episode" {
        if let Some(file_metadata) = infer_series_file_metadata(&file.file_path) {
            let source_title = file.source_title.trim();
            let lookup_title =
                if source_title.is_empty() || is_episode_like_source_title(source_title) {
                    file_metadata.title.clone()
                } else {
                    file.source_title.clone()
                };
            let file_title = file.title.trim();
            let title =
                if file_title.is_empty() || file_title.eq_ignore_ascii_case(&file_metadata.title) {
                    file_metadata.display_title
                } else {
                    file.title.clone()
                };
            let year = file.year.or(file_metadata.year);
            let season_air_year = year
                .is_none()
                .then(|| {
                    file_metadata
                        .season_air_year
                        .map(|year| MetadataSeasonAirYearHint {
                            season_number: file_metadata.season_number,
                            year,
                        })
                })
                .flatten();
            return ScanPresentationGroup {
                item_key: series_group_item_key(&file.file_path, &lookup_title),
                media_type: "series".to_string(),
                title,
                lookup_title,
                year,
                season_air_year,
            };
        }

        return ScanPresentationGroup {
            item_key: series_group_item_key(&file.file_path, &file.source_title),
            media_type: "series".to_string(),
            title: file
                .title
                .trim()
                .is_empty()
                .then(|| file.source_title.clone())
                .unwrap_or_else(|| file.title.clone()),
            lookup_title: file.source_title.clone(),
            year: file.year,
            season_air_year: None,
        };
    }

    ScanPresentationGroup {
        item_key: file.file_path.to_string_lossy().to_string(),
        media_type: "movie".to_string(),
        title: file
            .title
            .trim()
            .is_empty()
            .then(|| file.source_title.clone())
            .unwrap_or_else(|| file.title.clone()),
        lookup_title: file.source_title.clone(),
        year: file.year,
        season_air_year: None,
    }
}

fn is_episode_like_source_title(value: &str) -> bool {
    let pseudo_file_name = format!("{value}.mkv");
    infer_series_file_metadata(std::path::Path::new(&pseudo_file_name)).is_some()
}

fn group_discovered_files_for_scan(
    library: &Library,
    discovered_files: Vec<DiscoveredMediaFile>,
) -> Vec<ScanDiscoveredGroup> {
    let discovered_files =
        normalize_discovered_files_for_local_structure(library, discovered_files);
    let mut groups = Vec::<ScanDiscoveredGroup>::new();
    let mut group_indexes = HashMap::<String, usize>::new();

    for file in discovered_files {
        let presentation = build_scan_presentation_group(&library.library_type, &file);

        if let Some(index) = group_indexes.get(&presentation.item_key).copied() {
            groups[index].files.push(file);
            continue;
        }

        let next_index = groups.len();
        group_indexes.insert(presentation.item_key.clone(), next_index);
        groups.push(ScanDiscoveredGroup {
            presentation,
            files: vec![file],
        });
    }

    groups
}

fn build_scan_group_progress_update(
    scan_job_id: i64,
    library_id: i64,
    presentation: &ScanPresentationGroup,
    preview_file: Option<&DiscoveredMediaFile>,
    item_index: i32,
    total_items: i32,
    stage: ScanItemStage,
) -> ScanJobItemProgressUpdate {
    let (stage_name, progress_percent) = match stage {
        ScanItemStage::Analyzed => (SCAN_ITEM_STAGE_ANALYZED, 30),
        ScanItemStage::PendingCommitted => (SCAN_ITEM_STAGE_PENDING_COMMITTED, 40),
        ScanItemStage::Metadata => (SCAN_ITEM_STAGE_METADATA, 60),
        ScanItemStage::Artwork => (SCAN_ITEM_STAGE_ARTWORK, 85),
        ScanItemStage::Completed => (SCAN_ITEM_STAGE_COMPLETED, 100),
    };
    let artwork_preview_file = scan_progress_artwork_preview_file(stage, preview_file);

    ScanJobItemProgressUpdate {
        scan_job_id,
        library_id,
        item_key: presentation.item_key.clone(),
        media_type: presentation.media_type.clone(),
        title: presentation.title.clone(),
        year: preview_file
            .and_then(|file| file.year)
            .or(presentation.year),
        overview: scan_progress_overview(presentation, preview_file),
        poster_path: browser_visible_scan_artwork_path(scan_progress_poster_path(
            presentation,
            artwork_preview_file,
        )),
        backdrop_path: browser_visible_scan_artwork_path(scan_progress_backdrop_path(
            presentation,
            artwork_preview_file,
        )),
        metadata_status: preview_file.and_then(|file| file.metadata_status.clone()),
        remote_media_type: preview_file.and_then(|file| file.remote_media_type.clone()),
        season_number: None,
        episode_number: None,
        item_index,
        total_items,
        stage: stage_name.to_string(),
        progress_percent,
    }
}

fn scan_progress_artwork_preview_file<'a>(
    stage: ScanItemStage,
    file: Option<&'a DiscoveredMediaFile>,
) -> Option<&'a DiscoveredMediaFile> {
    if matches!(stage, ScanItemStage::Completed) {
        file
    } else {
        None
    }
}

fn browser_visible_scan_artwork_path(path: Option<String>) -> Option<String> {
    let path = path?;
    let trimmed = path.trim();

    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("/api/")
    {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn scan_progress_poster_path(
    presentation: &ScanPresentationGroup,
    file: Option<&DiscoveredMediaFile>,
) -> Option<String> {
    let file = file?;

    if presentation.media_type.eq_ignore_ascii_case("series") {
        return file.series_poster_path.clone();
    }

    file.poster_path.clone()
}

fn scan_progress_backdrop_path(
    presentation: &ScanPresentationGroup,
    file: Option<&DiscoveredMediaFile>,
) -> Option<String> {
    let file = file?;

    if presentation.media_type.eq_ignore_ascii_case("series") {
        return file.series_backdrop_path.clone();
    }

    file.backdrop_path.clone()
}

fn scan_progress_overview(
    presentation: &ScanPresentationGroup,
    file: Option<&DiscoveredMediaFile>,
) -> Option<String> {
    let file = file?;

    if presentation.media_type.eq_ignore_ascii_case("series") {
        return file
            .season_overview
            .clone()
            .or_else(|| file.overview.clone());
    }

    file.overview.clone()
}

impl From<MetadataEnrichmentStage> for ScanItemStage {
    fn from(value: MetadataEnrichmentStage) -> Self {
        match value {
            MetadataEnrichmentStage::Metadata => Self::Metadata,
            MetadataEnrichmentStage::Artwork => Self::Artwork,
            MetadataEnrichmentStage::Completed => Self::Completed,
        }
    }
}

async fn emit_scan_job_phase(
    pool: &PgPool,
    scan_job_id: i64,
    phase: &str,
    progress_percent: i32,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) {
    match mova_db::update_scan_job_phase(pool, scan_job_id, phase, progress_percent).await {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                scan_job, phase,
            )));
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                scan_job_id,
                phase,
                error = ?error,
                "failed to fetch scan job before publishing phase update"
            );
        }
    }
}

async fn emit_current_scan_job_update(
    pool: &PgPool,
    scan_job_id: i64,
    event_listener: &Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) {
    match mova_db::get_scan_job(pool, scan_job_id).await {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_PROCESSING,
            )));
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                scan_job_id,
                error = ?error,
                "failed to load authoritative scan pipeline progress"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        media_classification::{LIBRARY_TYPE_MIXED, LIBRARY_TYPE_SERIES},
        metadata::{MetadataLookup, MetadataProvider, RemoteMetadata},
    };
    use async_trait::async_trait;
    use mova_db::ExistingMediaMetadataSummary;
    use mova_domain::{
        Library, METADATA_FAILURE_NO_REMOTE_MATCH, METADATA_FAILURE_PROVIDER_ERROR,
        METADATA_STATUS_FAILED, METADATA_STATUS_MATCHED, METADATA_STATUS_PENDING,
        METADATA_STATUS_SKIPPED, METADATA_STATUS_UNMATCHED, REMOTE_MEDIA_TYPE_MOVIE,
        REMOTE_MEDIA_TYPE_SERIES,
    };
    use mova_scan::{
        discovered_media_file_inventory_scan_hash, discovered_media_file_scan_hash,
        DiscoveredMediaFile, DiscoveredMediaFileInventory,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        time::Instant,
    };
    use time::OffsetDateTime;

    fn build_discovered_file() -> DiscoveredMediaFile {
        DiscoveredMediaFile {
            file_path: PathBuf::from("/media/series/Arcane/Arcane.S01E01.mkv"),
            file_modified_at_ms: Some(1_700_000_000_000),
            probe_error: None,
            metadata_provider: None,
            metadata_provider_item_id: None,
            title: "Arcane".to_string(),
            source_title: "Arcane.S01E01".to_string(),
            original_title: None,
            sort_title: None,
            series_sidecar_title: None,
            series_sidecar_year: None,
            year: Some(2021),
            external_ids: Vec::new(),
            ratings: Vec::new(),
            metadata_status: Some(METADATA_STATUS_MATCHED.to_string()),
            metadata_failure_reason: None,
            remote_media_type: None,
            country: None,
            genres: None,
            studio: None,
            season_number: Some(1),
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: Some(1),
            episode_title: Some("Welcome to the Playground".to_string()),
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            series_logo_path: None,
            poster_path: None,
            backdrop_path: None,
            logo_path: None,
            file_size: 1,
            container: Some("mkv".to_string()),
            duration_seconds: Some(2400),
            video_title: None,
            video_codec: None,
            video_profile: None,
            video_level: None,
            audio_codec: None,
            width: None,
            height: None,
            bitrate: None,
            video_bitrate: None,
            video_frame_rate: None,
            video_aspect_ratio: None,
            video_scan_type: None,
            video_color_primaries: None,
            video_color_space: None,
            video_color_transfer: None,
            video_bit_depth: None,
            video_pixel_format: None,
            video_reference_frames: None,
            technical_tags: Vec::new(),
            audio_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
        }
    }

    #[test]
    fn scan_notification_summary_keeps_provider_failure_and_probe_warning_separate() {
        let mut file = build_discovered_file();
        file.metadata_status = Some(METADATA_STATUS_FAILED.to_string());
        file.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
        file.probe_error = Some("ffprobe failed:\n EBML header parsing failed".to_string());
        let group = super::ScanDiscoveredGroup {
            presentation: super::ScanPresentationGroup {
                item_key: "movie:a-minecraft-movie:2025".to_string(),
                media_type: "movie".to_string(),
                title: "A Minecraft Movie".to_string(),
                lookup_title: "A Minecraft Movie".to_string(),
                year: Some(2025),
                season_air_year: None,
            },
            files: vec![file],
        };

        let mut summary = mova_domain::ScanNotificationSummary::default();
        super::record_scan_notification_group(&mut summary, &group, Some("operation\n timed out"));
        let result = &summary.issues[0];

        assert_eq!(summary.failed_files, 1);
        assert_eq!(summary.probe_warning_count, 1);
        assert_eq!(summary.issue_count, 1);
        assert_eq!(result.metadata_status, METADATA_STATUS_FAILED);
        assert_eq!(
            result.metadata_failure_reason.as_deref(),
            Some(METADATA_FAILURE_PROVIDER_ERROR)
        );
        assert_eq!(
            result.failure_detail.as_deref(),
            Some("operation timed out")
        );
        assert_eq!(result.probe_warning_count, 1);
        assert_eq!(
            result.probe_warning_detail.as_deref(),
            Some("ffprobe failed: EBML header parsing failed")
        );
    }

    #[test]
    fn scan_notification_summary_counts_all_issues_but_bounds_payload_details() {
        let issue_total = mova_domain::MAX_SCAN_NOTIFICATION_ISSUES + 5;
        let mut summary = mova_domain::ScanNotificationSummary::default();

        for index in 0..issue_total {
            let mut file = build_discovered_file();
            file.metadata_status = Some(METADATA_STATUS_UNMATCHED.to_string());
            file.metadata_failure_reason = Some(METADATA_FAILURE_NO_REMOTE_MATCH.to_string());
            let group = super::ScanDiscoveredGroup {
                presentation: super::ScanPresentationGroup {
                    item_key: format!("movie:unmatched:{index}"),
                    media_type: "movie".to_string(),
                    title: format!("Unmatched {index}"),
                    lookup_title: format!("Unmatched {index}"),
                    year: None,
                    season_air_year: None,
                },
                files: vec![file],
            };

            super::record_scan_notification_group(&mut summary, &group, None);
        }

        assert_eq!(summary.unmatched_files, issue_total as i32);
        assert_eq!(summary.issue_count, issue_total as i32);
        assert_eq!(
            summary.issues.len(),
            mova_domain::MAX_SCAN_NOTIFICATION_ISSUES
        );
    }

    fn build_pending_scan_file(file: DiscoveredMediaFile) -> super::PendingScanFile {
        super::PendingScanFile {
            changed_file: super::IncrementalScanFile {
                inventory: DiscoveredMediaFileInventory {
                    file_path: file.file_path.clone(),
                    file_size: file.file_size,
                    file_modified_at_ms: file.file_modified_at_ms,
                },
                existing_metadata: None,
            },
            file,
        }
    }

    #[derive(Debug, Clone)]
    struct FixedMetadataProvider {
        enabled: bool,
    }

    #[async_trait]
    impl MetadataProvider for FixedMetadataProvider {
        fn is_enabled(&self) -> bool {
            self.enabled
        }

        async fn lookup(&self, _lookup: &MetadataLookup) -> anyhow::Result<Option<RemoteMetadata>> {
            Ok(None)
        }
    }

    fn build_library(library_type: &str) -> Library {
        Library {
            id: 7,
            name: "Library".to_string(),
            description: None,
            library_type: library_type.to_string(),
            metadata_language: "zh-CN".to_string(),
            root_path: "/media".to_string(),
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
        }
    }

    fn build_existing_movie_metadata() -> ExistingMediaMetadataSummary {
        ExistingMediaMetadataSummary {
            media_file_id: 11,
            file_path: "/media/movies/Arcane.mkv".to_string(),
            media_type: "movie".to_string(),
            metadata_provider: Some("tmdb".to_string()),
            metadata_provider_item_id: Some(77),
            metadata_status: METADATA_STATUS_MATCHED.to_string(),
            metadata_failure_reason: None,
            remote_media_type: Some(REMOTE_MEDIA_TYPE_MOVIE.to_string()),
            title: "Arcane".to_string(),
            source_title: "Arcane".to_string(),
            original_title: Some("Arcane Original".to_string()),
            sort_title: Some("Arcane, The".to_string()),
            year: Some(2021),
            country: Some("United States".to_string()),
            genres: Some("Animation, Drama".to_string()),
            studio: Some("Fortiche".to_string()),
            overview: Some("Stored overview".to_string()),
            poster_path: Some("/cache/poster.jpg".to_string()),
            backdrop_path: Some("/cache/backdrop.jpg".to_string()),
            logo_path: Some("/cache/logo.png".to_string()),
            scan_hash: Some("movie-hash".to_string()),
            container: Some("mkv".to_string()),
            file_size: 1024,
            duration_seconds: Some(600),
            video_title: Some("Video stream".to_string()),
            video_codec: Some("h264".to_string()),
            video_profile: Some("main".to_string()),
            video_level: Some("4.1".to_string()),
            audio_codec: Some("aac".to_string()),
            width: Some(1920),
            height: Some(1080),
            bitrate: Some(1_000_000),
            video_bitrate: Some(800_000),
            video_frame_rate: Some(24.0),
            video_aspect_ratio: Some("16:9".to_string()),
            video_scan_type: Some("Progressive".to_string()),
            video_color_primaries: Some("bt709".to_string()),
            video_color_space: Some("bt709".to_string()),
            video_color_transfer: Some("bt709".to_string()),
            video_bit_depth: Some(8),
            video_pixel_format: Some("yuv420p".to_string()),
            video_reference_frames: Some(4),
            technical_tags: vec!["HDR10".to_string()],
            local_analysis_version: super::LOCAL_ANALYSIS_VERSION,
            audio_tracks: vec![mova_db::CreateAudioTrackParams {
                stream_index: 1,
                language: Some("eng".to_string()),
                audio_codec: Some("aac".to_string()),
                label: Some("English AAC".to_string()),
                channel_layout: Some("stereo".to_string()),
                channels: Some(2),
                bitrate: Some(160_000),
                sample_rate: Some(48_000),
                is_default: true,
            }],
            subtitle_tracks: vec![mova_db::CreateSubtitleTrackParams {
                source_kind: "embedded".to_string(),
                file_path: None,
                stream_index: Some(2),
                language: Some("eng".to_string()),
                subtitle_format: "subrip".to_string(),
                label: Some("English".to_string()),
                is_default: false,
                is_forced: false,
                is_hearing_impaired: false,
            }],
            series_title: None,
            series_metadata_provider: None,
            series_metadata_provider_item_id: None,
            series_source_title: None,
            series_original_title: None,
            series_sort_title: None,
            series_year: None,
            series_country: None,
            series_genres: None,
            series_studio: None,
            series_overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            series_logo_path: None,
            season_title: None,
            season_number: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_title: None,
            episode_number: None,
        }
    }

    fn build_existing_episode_metadata() -> ExistingMediaMetadataSummary {
        ExistingMediaMetadataSummary {
            media_file_id: 22,
            file_path: "/media/series/Arcane/Arcane.S01E01.mkv".to_string(),
            media_type: "episode".to_string(),
            metadata_provider: Some("tmdb".to_string()),
            metadata_provider_item_id: Some(88),
            metadata_status: METADATA_STATUS_MATCHED.to_string(),
            metadata_failure_reason: None,
            remote_media_type: Some(REMOTE_MEDIA_TYPE_SERIES.to_string()),
            title: "Welcome to the Playground".to_string(),
            source_title: "Arcane.S01E01".to_string(),
            original_title: None,
            sort_title: None,
            year: None,
            country: None,
            genres: None,
            studio: None,
            overview: Some("Episode overview".to_string()),
            poster_path: Some("/cache/episode-poster.jpg".to_string()),
            backdrop_path: Some("/cache/episode-backdrop.jpg".to_string()),
            logo_path: None,
            scan_hash: Some("episode-hash".to_string()),
            container: Some("mkv".to_string()),
            file_size: 2048,
            duration_seconds: Some(1200),
            video_title: Some("Episode video".to_string()),
            video_codec: Some("hevc".to_string()),
            video_profile: Some("main10".to_string()),
            video_level: Some("5.1".to_string()),
            audio_codec: Some("eac3".to_string()),
            width: Some(3840),
            height: Some(2160),
            bitrate: Some(8_000_000),
            video_bitrate: Some(7_000_000),
            video_frame_rate: Some(24.0),
            video_aspect_ratio: Some("16:9".to_string()),
            video_scan_type: Some("Progressive".to_string()),
            video_color_primaries: Some("bt2020".to_string()),
            video_color_space: Some("bt2020nc".to_string()),
            video_color_transfer: Some("smpte2084".to_string()),
            video_bit_depth: Some(10),
            video_pixel_format: Some("yuv420p10le".to_string()),
            video_reference_frames: Some(5),
            technical_tags: vec!["Dolby Vision".to_string()],
            local_analysis_version: super::LOCAL_ANALYSIS_VERSION,
            audio_tracks: vec![mova_db::CreateAudioTrackParams {
                stream_index: 1,
                language: Some("eng".to_string()),
                audio_codec: Some("eac3".to_string()),
                label: Some("English EAC3".to_string()),
                channel_layout: Some("5.1".to_string()),
                channels: Some(6),
                bitrate: Some(768_000),
                sample_rate: Some(48_000),
                is_default: true,
            }],
            subtitle_tracks: Vec::new(),
            series_title: Some("Arcane".to_string()),
            series_metadata_provider: Some("tmdb".to_string()),
            series_metadata_provider_item_id: Some(88),
            series_source_title: Some("Arcane".to_string()),
            series_original_title: Some("Arcane Original".to_string()),
            series_sort_title: Some("Arcane, The".to_string()),
            series_year: Some(2021),
            series_country: Some("United States".to_string()),
            series_genres: Some("Animation, Drama".to_string()),
            series_studio: Some("Fortiche".to_string()),
            series_overview: Some("Series overview".to_string()),
            series_poster_path: Some("/cache/series-poster.jpg".to_string()),
            series_backdrop_path: Some("/cache/series-backdrop.jpg".to_string()),
            series_logo_path: Some("/cache/series-logo.png".to_string()),
            season_title: Some("Season 01".to_string()),
            season_number: Some(1),
            season_overview: Some("Season overview".to_string()),
            season_poster_path: Some("/cache/season-poster.jpg".to_string()),
            season_backdrop_path: Some("/cache/season-backdrop.jpg".to_string()),
            episode_title: Some("Welcome to the Playground".to_string()),
            episode_number: Some(1),
        }
    }

    #[test]
    fn can_skip_existing_media_summary_only_skips_successful_rows() {
        let mut summary = build_existing_movie_metadata();
        summary.scan_hash = Some("same-hash".to_string());

        assert!(super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));

        summary.metadata_status = METADATA_STATUS_UNMATCHED.to_string();
        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));

        summary.metadata_status = METADATA_STATUS_FAILED.to_string();
        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));

        summary.metadata_status = METADATA_STATUS_SKIPPED.to_string();
        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));
        assert!(super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            false,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));

        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "changed-hash",
            false,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));

        summary.scan_hash = Some("same-hash".to_string());
        summary.local_analysis_version = super::LOCAL_ANALYSIS_VERSION - 1;
        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            false,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));
    }

    #[test]
    fn can_skip_existing_media_summary_rescans_other_review_rows_even_without_provider() {
        let mut summary = build_existing_movie_metadata();
        summary.scan_hash = Some("same-hash".to_string());
        summary.metadata_status = METADATA_STATUS_SKIPPED.to_string();
        summary.metadata_provider = None;
        summary.metadata_provider_item_id = None;
        summary.original_title = None;
        summary.overview = None;
        summary.poster_path = None;
        summary.backdrop_path = None;
        summary.logo_path = None;

        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            false,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));
    }

    #[test]
    fn prepare_scan_groups_marks_rows_as_pending_before_remote_confirmation() {
        let mut file = build_discovered_file();
        file.file_path = PathBuf::from("/media/movies/狂野时代 (2025)/狂野时代.2025.mp4");
        file.season_number = None;
        file.episode_number = None;
        file.title = "狂野时代".to_string();
        file.source_title = "狂野时代".to_string();
        file.metadata_status = Some(METADATA_STATUS_SKIPPED.to_string());
        file.metadata_failure_reason = None;

        let presentation = super::build_scan_presentation_group(LIBRARY_TYPE_MIXED, &file);
        let mut groups = vec![super::ScanDiscoveredGroup {
            presentation,
            files: vec![file],
        }];

        super::prepare_scan_groups_for_metadata_lookup(&mut groups);

        assert_eq!(
            groups[0].files[0].metadata_status.as_deref(),
            Some(METADATA_STATUS_PENDING)
        );
        assert_eq!(groups[0].files[0].metadata_failure_reason, None);
    }

    #[test]
    fn can_skip_existing_media_summary_rescans_review_rows_with_visible_metadata() {
        let mut summary = build_existing_movie_metadata();
        summary.scan_hash = Some("same-hash".to_string());
        summary.metadata_status = METADATA_STATUS_UNMATCHED.to_string();
        summary.metadata_provider = None;
        summary.metadata_provider_item_id = None;

        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new("/media/movies/Avatar.Fire.and.Ash.2025.mkv"),
        ));
        assert!(!super::is_existing_summary_in_other_review_section(
            &summary
        ));
    }

    #[test]
    fn can_skip_existing_media_summary_keeps_matched_movies_without_poster_stable() {
        let mut summary = build_existing_movie_metadata();
        summary.scan_hash = Some("same-hash".to_string());
        summary.poster_path = None;

        assert!(super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));

        summary.poster_path = Some("https://image.tmdb.org/t/p/original/poster.jpg".to_string());

        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));

        summary.poster_path = Some("/cache/poster.jpg".to_string());
        summary.backdrop_path =
            Some("https://image.tmdb.org/t/p/original/backdrop.jpg".to_string());

        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));
    }

    #[test]
    fn can_skip_existing_media_summary_retries_matched_rows_without_tmdb_binding() {
        let mut summary = build_existing_movie_metadata();
        summary.scan_hash = Some("same-hash".to_string());
        summary.metadata_provider_item_id = None;

        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));

        summary.metadata_provider_item_id = Some(77);
        summary.metadata_provider = None;

        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new("/media/movies/Arcane.mkv"),
        ));
    }

    #[test]
    fn discovered_file_from_existing_local_analysis_preserves_cached_probe_data() {
        let summary = build_existing_episode_metadata();
        let inventory = DiscoveredMediaFileInventory {
            file_path: PathBuf::from("/media/series/Arcane/Arcane.S01E01.mkv"),
            file_size: 2048,
            file_modified_at_ms: Some(1_700_000_000_000),
        };

        let file = super::discovered_file_from_existing_local_analysis(&inventory, &summary)
            .expect("cached local analysis should rebuild discovered file");

        assert_eq!(file.title, "Arcane");
        assert_eq!(file.source_title, "Arcane");
        assert_eq!(file.season_number, Some(1));
        assert_eq!(file.episode_number, Some(1));
        assert_eq!(file.video_codec.as_deref(), Some("hevc"));
        assert_eq!(file.technical_tags, vec!["Dolby Vision".to_string()]);
        assert_eq!(file.audio_tracks.len(), 1);
        assert_eq!(file.audio_tracks[0].channel_layout.as_deref(), Some("5.1"));
        assert_eq!(
            discovered_media_file_scan_hash(&file),
            discovered_media_file_inventory_scan_hash(&inventory)
        );
    }

    #[test]
    fn can_skip_existing_media_summary_keeps_matched_episodes_without_series_poster_stable() {
        let mut summary = build_existing_episode_metadata();
        summary.scan_hash = Some("same-hash".to_string());
        summary.series_poster_path = None;

        assert!(super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new("/media/series/Arcane/Arcane.S01E01.mkv"),
        ));

        summary.series_poster_path =
            Some("https://image.tmdb.org/t/p/original/series-poster.jpg".to_string());

        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new("/media/series/Arcane/Arcane.S01E01.mkv"),
        ));
    }

    #[test]
    fn can_skip_existing_media_summary_ignores_series_directory_title() {
        let mut summary = build_existing_episode_metadata();
        summary.scan_hash = Some("same-hash".to_string());
        summary.series_title = Some("Resolved Series".to_string());
        summary.series_source_title = Some("All Her Fault".to_string());

        assert!(super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new(
                "/media/overseas_tv/都是她的错.2025/Season 01/All.Her.Fault.2025.S01E01.2160p.PCOK.WEB-DL.DDP5.1.H.265-KRATOS.mkv",
            ),
        ));

        assert!(super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new(
                "/media/overseas_tv/莎拉的真伪人生(2026)/The.Art.of.Sarah.S01E01.2160p.NF.WEB-DL.DDP.5.1.DV.H.265.mkv",
            ),
        ));
    }

    #[test]
    fn can_skip_existing_media_summary_retries_local_display_title_override() {
        let mut summary = build_existing_episode_metadata();
        summary.scan_hash = Some("same-hash".to_string());
        summary.series_title = Some("Alls Fair (2025)".to_string());
        summary.series_source_title = Some("Alls Fair".to_string());
        summary.series_metadata_provider_item_id = Some(259909);
        summary.series_poster_path = Some("/cache/series-poster.jpg".to_string());
        summary.series_backdrop_path = Some("/cache/series-backdrop.jpg".to_string());

        assert!(!super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new(
                "/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E01.mkv",
            ),
        ));

        summary.series_title = Some("诉讼女王".to_string());
        assert!(super::can_skip_existing_media_summary(
            &summary,
            "same-hash",
            true,
            "zh-CN",
            Path::new(
                "/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E01.mkv",
            ),
        ));
    }

    #[test]
    fn mixed_library_classifies_episode_like_paths_as_episode() {
        assert_eq!(
            super::classify_media_type(LIBRARY_TYPE_MIXED, Path::new("Arcane.S01E01.mkv")),
            "episode"
        );
    }

    #[test]
    fn mixed_library_classifies_movie_like_paths_as_movie() {
        assert_eq!(
            super::classify_media_type(
                LIBRARY_TYPE_MIXED,
                Path::new("How.to.Train.Your.Dragon.2025.mkv")
            ),
            "movie"
        );
    }

    #[test]
    fn library_type_does_not_override_file_name_classification() {
        assert_eq!(
            super::classify_media_type(LIBRARY_TYPE_SERIES, Path::new("Movie.2025.mkv")),
            "movie"
        );
    }

    #[test]
    fn scan_phase_label_returns_user_facing_stage_name() {
        assert_eq!(
            super::scan_phase_label(super::SCAN_PHASE_DISCOVERING),
            "Directory scan failed"
        );
        assert_eq!(
            super::scan_phase_label(super::SCAN_PHASE_PROCESSING),
            "Media processing failed"
        );
        assert_eq!(
            super::scan_phase_label(super::SCAN_PHASE_FINALIZING),
            "Library finalization failed"
        );
    }

    #[test]
    fn format_scan_phase_error_prefixes_stage_context() {
        assert_eq!(
            super::format_scan_phase_error(
                super::SCAN_PHASE_DISCOVERING,
                "Failed to scan library files: No such file or directory"
            ),
            "Directory scan failed: Failed to scan library files: No such file or directory"
        );
    }

    #[test]
    fn should_flush_discovery_progress_for_first_visible_count() {
        let now = Instant::now();

        assert!(super::should_flush_discovery_progress(0, 1, None, now));
    }

    #[test]
    fn should_flush_discovery_progress_after_file_delta_or_interval() {
        let now = Instant::now();
        let last_flush_at = now
            .checked_sub(super::SCAN_DISCOVERY_PROGRESS_MIN_INTERVAL)
            .expect("test instant should support subtraction");

        assert!(!super::should_flush_discovery_progress(
            10,
            20,
            Some(now),
            now
        ));
        assert!(super::should_flush_discovery_progress(
            10,
            10 + super::SCAN_DISCOVERY_PROGRESS_MIN_FILE_DELTA,
            Some(now),
            now
        ));
        assert!(super::should_flush_discovery_progress(
            10,
            20,
            Some(last_flush_at),
            now
        ));
    }

    #[test]
    fn is_present_media_file_path_requires_existing_file() {
        let root = std::env::temp_dir().join(format!(
            "mova-scan-path-{}-{}",
            std::process::id(),
            OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        let file = root.join("video.mkv");
        let missing = root.join("missing.mkv");

        fs::create_dir(&root).expect("test temp directory should be created");
        fs::write(&file, b"video").expect("test media file should be created");

        assert!(super::is_present_media_file_path(&file));
        assert!(!super::is_present_media_file_path(&root));
        assert!(!super::is_present_media_file_path(&missing));

        let _ = fs::remove_file(file);
        let _ = fs::remove_dir(root);
    }

    #[test]
    fn build_scan_item_progress_update_emits_group_level_series_payload() {
        let presentation =
            super::build_scan_presentation_group(LIBRARY_TYPE_SERIES, &build_discovered_file());
        let progress = super::build_scan_group_progress_update(
            41,
            7,
            &presentation,
            None,
            1,
            3,
            super::ScanItemStage::Analyzed,
        );

        assert_eq!(progress.scan_job_id, 41);
        assert_eq!(progress.library_id, 7);
        assert_eq!(progress.media_type, "series");
        assert_eq!(progress.title, "Arcane");
        assert_eq!(progress.season_number, None);
        assert_eq!(progress.episode_number, None);
        assert_eq!(progress.stage, "analyzed");
        assert_eq!(progress.progress_percent, 30);
        assert_eq!(progress.item_index, 1);
        assert_eq!(progress.total_items, 3);
        assert_eq!(progress.item_key, "series-title:arcane");
    }

    #[test]
    fn build_scan_item_progress_update_holds_artwork_until_completed_and_browser_visible() {
        let mut file = build_discovered_file();
        file.series_poster_path =
            Some("https://image.tmdb.org/t/p/original/poster.jpg".to_string());
        file.series_backdrop_path =
            Some("https://image.tmdb.org/t/p/original/backdrop.jpg".to_string());
        let presentation = super::build_scan_presentation_group(LIBRARY_TYPE_SERIES, &file);

        let artwork_progress = super::build_scan_group_progress_update(
            41,
            7,
            &presentation,
            Some(&file),
            1,
            3,
            super::ScanItemStage::Artwork,
        );
        assert_eq!(artwork_progress.poster_path, None);
        assert_eq!(artwork_progress.backdrop_path, None);

        let completed_progress = super::build_scan_group_progress_update(
            41,
            7,
            &presentation,
            Some(&file),
            1,
            3,
            super::ScanItemStage::Completed,
        );
        assert_eq!(
            completed_progress.poster_path.as_deref(),
            Some("https://image.tmdb.org/t/p/original/poster.jpg")
        );
        assert_eq!(
            completed_progress.backdrop_path.as_deref(),
            Some("https://image.tmdb.org/t/p/original/backdrop.jpg")
        );

        file.series_poster_path = Some("/media/series/Arcane/poster.jpg".to_string());
        let completed_with_local_artwork = super::build_scan_group_progress_update(
            41,
            7,
            &presentation,
            Some(&file),
            1,
            3,
            super::ScanItemStage::Completed,
        );
        assert_eq!(completed_with_local_artwork.poster_path, None);
    }

    #[test]
    fn build_scan_item_progress_update_does_not_promote_episode_artwork_to_series() {
        let mut file = build_discovered_file();
        file.series_poster_path = None;
        file.series_backdrop_path = None;
        file.season_poster_path = None;
        file.season_backdrop_path = None;
        file.poster_path =
            Some("https://image.tmdb.org/t/p/original/episode-still.jpg".to_string());
        file.backdrop_path =
            Some("https://image.tmdb.org/t/p/original/episode-backdrop.jpg".to_string());
        let presentation = super::build_scan_presentation_group(LIBRARY_TYPE_SERIES, &file);

        let completed_progress = super::build_scan_group_progress_update(
            41,
            7,
            &presentation,
            Some(&file),
            1,
            3,
            super::ScanItemStage::Completed,
        );

        assert_eq!(completed_progress.poster_path, None);
        assert_eq!(completed_progress.backdrop_path, None);
    }

    #[test]
    fn build_scan_item_progress_update_does_not_promote_season_artwork_to_series() {
        let mut file = build_discovered_file();
        file.series_poster_path = None;
        file.series_backdrop_path = None;
        file.season_poster_path =
            Some("https://image.tmdb.org/t/p/original/season-poster.jpg".to_string());
        file.season_backdrop_path =
            Some("https://image.tmdb.org/t/p/original/season-backdrop.jpg".to_string());
        let presentation = super::build_scan_presentation_group(LIBRARY_TYPE_SERIES, &file);

        let completed_progress = super::build_scan_group_progress_update(
            41,
            7,
            &presentation,
            Some(&file),
            1,
            3,
            super::ScanItemStage::Completed,
        );

        assert_eq!(completed_progress.poster_path, None);
        assert_eq!(completed_progress.backdrop_path, None);
    }

    #[test]
    fn group_discovered_files_for_scan_merges_episode_files_by_series_folder() {
        let mut first_file = build_discovered_file();
        first_file.file_path = PathBuf::from("Arcane/Season 01/Arcane.S01E01.mkv");
        first_file.episode_number = Some(1);
        first_file.episode_title = Some("Welcome to the Playground".to_string());

        let mut second_file = build_discovered_file();
        second_file.file_path = PathBuf::from("Arcane/Season 01/Arcane.S01E02.mkv");
        second_file.episode_number = Some(2);
        second_file.episode_title = Some("Some Mysteries Are Better Left Unsolved".to_string());

        let groups = super::group_discovered_files_for_scan(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![first_file, second_file],
        );

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].presentation.media_type, "series");
        assert_eq!(groups[0].presentation.title, "Arcane");
        assert_eq!(groups[0].files.len(), 2);
        assert_eq!(groups[0].presentation.item_key, "series-folder:arcane");
    }

    #[test]
    fn group_discovered_files_for_scan_merges_multi_season_series_years_by_title() {
        let mut first_file = build_discovered_file();
        first_file.file_path = PathBuf::from("黑袍纠察队/Season 01/The Boys (2019) - S01E01.mkv");
        first_file.title = "The Boys".to_string();
        first_file.source_title = "The Boys".to_string();
        first_file.year = Some(2019);
        first_file.season_number = Some(1);
        first_file.episode_number = Some(1);

        let mut second_file = build_discovered_file();
        second_file.file_path = PathBuf::from("黑袍纠察队/Season 02/The Boys (2020) - S02E01.mkv");
        second_file.title = "The Boys".to_string();
        second_file.source_title = "The Boys".to_string();
        second_file.year = Some(2020);
        second_file.season_number = Some(2);
        second_file.episode_number = Some(1);

        let mut third_file = build_discovered_file();
        third_file.file_path =
            PathBuf::from("黑袍纠察队/Season 05/黑袍纠察队.S05E01.2026.2160p.mkv");
        third_file.title = "黑袍纠察队".to_string();
        third_file.source_title = "黑袍纠察队".to_string();
        third_file.year = None;
        third_file.season_number = Some(5);
        third_file.episode_number = Some(1);

        let groups = super::group_discovered_files_for_scan(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![first_file, second_file, third_file],
        );

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].presentation.media_type, "series");
        assert_eq!(groups[0].presentation.title, "The Boys (2019)");
        assert_eq!(groups[0].presentation.lookup_title, "The Boys");
        assert_eq!(groups[0].presentation.year, Some(2019));
        assert_eq!(groups[0].presentation.item_key, "series-folder:黑袍纠察队");
        assert_eq!(groups[0].files.len(), 3);
        assert!(groups[0].files.iter().all(|file| file.year == Some(2019)));
        assert!(groups[0]
            .files
            .iter()
            .all(|file| file.source_title == "The Boys"));
    }

    #[test]
    fn group_discovered_files_for_scan_prefers_first_season_year_as_series_year() {
        let mut later_file = build_discovered_file();
        later_file.file_path = PathBuf::from("The Boys/A Season 02/The Boys (2020) - S02E01.mkv");
        later_file.title = "The Boys".to_string();
        later_file.source_title = "The Boys".to_string();
        later_file.year = Some(2020);
        later_file.season_number = Some(2);
        later_file.episode_number = Some(1);

        let mut earlier_file = build_discovered_file();
        earlier_file.file_path = PathBuf::from("The Boys/Z Season 01/The Boys (2019) - S01E01.mkv");
        earlier_file.title = "The Boys".to_string();
        earlier_file.source_title = "The Boys".to_string();
        earlier_file.year = Some(2019);
        earlier_file.season_number = Some(1);
        earlier_file.episode_number = Some(1);

        let groups = super::group_discovered_files_for_scan(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![later_file, earlier_file],
        );

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].presentation.title, "The Boys (2019)");
        assert_eq!(groups[0].presentation.lookup_title, "The Boys");
        assert_eq!(groups[0].presentation.year, Some(2019));
        assert!(groups[0].files.iter().all(|file| file.year == Some(2019)));
    }

    #[test]
    fn group_discovered_files_for_scan_does_not_promote_later_season_year_when_s01_exists() {
        let mut first_season = build_discovered_file();
        first_season.file_path = PathBuf::from("Fallout/S01/Fallout.S01E01.mkv");
        first_season.title = "Fallout".to_string();
        first_season.source_title = "Fallout".to_string();
        first_season.year = None;
        first_season.season_number = Some(1);
        first_season.episode_number = Some(1);

        let mut second_season = build_discovered_file();
        second_season.file_path = PathBuf::from("Fallout/S02/Fallout.S02E01.2025.2160p.mkv");
        second_season.title = "Fallout".to_string();
        second_season.source_title = "Fallout".to_string();
        second_season.year = Some(2025);
        second_season.season_number = Some(2);
        second_season.episode_number = Some(1);

        let groups = super::group_discovered_files_for_scan(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![second_season, first_season],
        );

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].presentation.lookup_title, "Fallout");
        assert_eq!(groups[0].presentation.year, None);
        assert_eq!(groups[0].presentation.season_air_year, None);
        assert!(groups[0].files.iter().all(|file| file.year.is_none()));
    }

    #[test]
    fn group_discovered_files_for_scan_uses_later_season_year_only_when_s01_is_absent() {
        let mut second_season = build_discovered_file();
        second_season.file_path = PathBuf::from("Fallout/S02/Fallout.S02E01.2025.2160p.mkv");
        second_season.title = "Fallout".to_string();
        second_season.source_title = "Fallout".to_string();
        second_season.year = Some(2025);
        second_season.season_number = Some(2);
        second_season.episode_number = Some(1);

        let groups = super::group_discovered_files_for_scan(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![second_season],
        );

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].presentation.lookup_title, "Fallout");
        assert_eq!(groups[0].presentation.year, None);
        assert_eq!(
            groups[0].presentation.season_air_year,
            Some(crate::metadata::MetadataSeasonAirYearHint {
                season_number: 2,
                year: 2025,
            })
        );
        assert!(groups[0].files.iter().all(|file| file.year.is_none()));
    }

    #[test]
    fn group_discovered_files_for_scan_prefers_tvshow_nfo_identity() {
        let root = std::env::temp_dir().join(format!(
            "mova-series-nfo-{}",
            OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        let series_root = root.join("目录标题 2030");
        let file_path = series_root
            .join("S02")
            .join("Fallback.Title.S02E01.2025.mkv");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(
            series_root.join("tvshow.nfo"),
            "<tvshow><title>Authoritative Show</title><year>2021</year></tvshow>",
        )
        .unwrap();

        let mut file = build_discovered_file();
        file.file_path = file_path;
        file.title = "Fallback Title".to_string();
        file.source_title = "Fallback Title".to_string();
        super::populate_series_sidecar_metadata(&mut file, &mut std::collections::HashMap::new());
        file.year = Some(2025);
        file.season_number = Some(2);
        file.episode_number = Some(1);

        let groups =
            super::group_discovered_files_for_scan(&build_library(LIBRARY_TYPE_MIXED), vec![file]);
        let _ = fs::remove_dir_all(&root);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].presentation.lookup_title, "Authoritative Show");
        assert_eq!(groups[0].presentation.title, "Authoritative Show");
        assert_eq!(groups[0].presentation.year, Some(2021));
        assert_eq!(groups[0].presentation.season_air_year, None);
    }

    #[test]
    fn build_media_entries_normalizes_multi_season_series_years_before_sync() {
        let mut first_file = build_discovered_file();
        first_file.file_path = PathBuf::from("黑袍纠察队/Season 01/The Boys (2019) - S01E01.mkv");
        first_file.title = "The Boys".to_string();
        first_file.source_title = "The Boys".to_string();
        first_file.year = Some(2019);
        first_file.season_number = Some(1);
        first_file.episode_number = Some(1);

        let mut second_file = build_discovered_file();
        second_file.file_path = PathBuf::from("黑袍纠察队/Season 02/The Boys (2020) - S02E01.mkv");
        second_file.title = "The Boys".to_string();
        second_file.source_title = "The Boys".to_string();
        second_file.year = Some(2020);
        second_file.season_number = Some(2);
        second_file.episode_number = Some(1);

        let mut third_file = build_discovered_file();
        third_file.file_path =
            PathBuf::from("黑袍纠察队/Season 05/黑袍纠察队.S05E01.2026.2160p.mkv");
        third_file.title = "黑袍纠察队".to_string();
        third_file.source_title = "黑袍纠察队".to_string();
        third_file.year = None;
        third_file.season_number = Some(5);
        third_file.episode_number = Some(1);

        let entries = super::build_media_entries(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![first_file, second_file, third_file],
            true,
        )
        .unwrap();

        assert_eq!(entries.len(), 3);
        assert!(entries.iter().all(|entry| entry.media_type == "episode"));
        assert!(entries.iter().all(|entry| entry.source_title == "The Boys"));
        assert!(entries.iter().all(|entry| entry.year == Some(2019)));
    }

    #[test]
    fn build_media_entries_preserves_tmdb_series_title_after_local_grouping() {
        let mut file = build_discovered_file();
        file.file_path = PathBuf::from(
            "/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E01.mkv",
        );
        file.metadata_provider = Some("tmdb".to_string());
        file.metadata_provider_item_id = Some(259909);
        file.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
        file.remote_media_type = Some(REMOTE_MEDIA_TYPE_SERIES.to_string());
        file.title = "诉讼女王".to_string();
        file.source_title = "Alls Fair".to_string();
        file.original_title = Some("All's Fair".to_string());
        file.year = Some(2025);
        file.season_number = Some(1);
        file.episode_number = Some(1);

        let entries =
            super::build_media_entries(&build_library(LIBRARY_TYPE_MIXED), vec![file], true)
                .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].media_type, "episode");
        assert_eq!(entries[0].title, "诉讼女王");
        assert_eq!(entries[0].source_title, "Alls Fair");
        assert_eq!(entries[0].original_title.as_deref(), Some("All's Fair"));
        assert_eq!(entries[0].year, Some(2025));
    }

    #[test]
    fn build_media_entries_only_authoritative_matched_entries_can_clear_artwork() {
        let mut file = build_discovered_file();
        file.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
        file.metadata_provider_item_id = Some(259909);

        let mut second_file = file.clone();
        second_file.file_path = PathBuf::from("shows/example/S01E02.mkv");
        second_file.episode_number = Some(2);

        let pending_entries = super::build_media_entries(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![file.clone()],
            false,
        )
        .unwrap();
        assert!(!pending_entries[0].allow_artwork_clear);

        let matched_entries = super::build_media_entries(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![file.clone(), second_file],
            true,
        )
        .unwrap();
        assert!(matched_entries
            .iter()
            .all(|entry| entry.allow_artwork_clear));
        assert_eq!(
            matched_entries
                .iter()
                .filter(|entry| entry.replace_remote_data)
                .count(),
            1
        );

        file.metadata_status = Some(METADATA_STATUS_UNMATCHED.to_string());
        file.metadata_provider_item_id = None;
        let unmatched_entries =
            super::build_media_entries(&build_library(LIBRARY_TYPE_MIXED), vec![file], true)
                .unwrap();
        assert!(!unmatched_entries[0].allow_artwork_clear);
        assert!(unmatched_entries[0].replace_remote_data);
    }

    #[test]
    fn build_pending_scan_groups_groups_series_before_full_inspection() {
        let mut first_file = build_discovered_file();
        first_file.file_path = PathBuf::from(
            "/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E01.mkv",
        );
        first_file.title = "Alls Fair (2025)".to_string();
        first_file.source_title = "Alls Fair".to_string();
        first_file.year = Some(2025);

        let mut second_file = first_file.clone();
        second_file.file_path = PathBuf::from(
            "/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E02.mkv",
        );
        second_file.episode_number = Some(2);

        let groups = super::build_pending_scan_groups_from_files(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![
                build_pending_scan_file(first_file),
                build_pending_scan_file(second_file),
            ],
        );

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
    }

    #[tokio::test]
    async fn inspect_incremental_scan_files_shallow_ignores_stale_existing_titles() {
        let mut summary = build_existing_episode_metadata();
        summary.series_title = Some("Wrong Old Title".to_string());
        summary.series_source_title = Some("Wrong Old Title".to_string());
        summary.title = "Wrong Old Episode".to_string();
        summary.source_title = "Wrong Old Episode".to_string();

        let pending_files = super::inspect_incremental_scan_files_shallow(vec![
            super::IncrementalScanFile {
                inventory: DiscoveredMediaFileInventory {
                    file_path: PathBuf::from(
                        "/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E01.mkv",
                    ),
                    file_size: 2048,
                    file_modified_at_ms: Some(1_700_000_000_000),
                },
                existing_metadata: Some(summary),
            },
        ])
        .await
        .expect("shallow inspection should parse without touching the filesystem");

        assert_eq!(pending_files.len(), 1);
        assert_eq!(pending_files[0].file.title, "Alls Fair");
        assert_eq!(pending_files[0].file.source_title, "Alls Fair");
        assert_eq!(pending_files[0].file.year, Some(2025));
        assert_eq!(
            pending_files[0]
                .changed_file
                .existing_metadata
                .as_ref()
                .and_then(|metadata| metadata.series_title.as_deref()),
            Some("Wrong Old Title")
        );
    }

    #[test]
    fn group_discovered_files_for_scan_merges_named_season_files_by_file_title() {
        let mut first_file = build_discovered_file();
        first_file.file_path =
            PathBuf::from("布里杰顿家族 (2020)/布里杰顿家族 - S01/布里杰顿家族 - S01E01.mkv");
        first_file.title = "布里杰顿家族".to_string();
        first_file.source_title = "布里杰顿家族".to_string();
        first_file.year = None;
        first_file.season_number = Some(1);
        first_file.episode_number = Some(1);

        let mut second_file = build_discovered_file();
        second_file.file_path =
            PathBuf::from("布里杰顿家族 (2020)/布里杰顿家族 - S02/布里杰顿家族 - S02E01.mkv");
        second_file.title = "布里杰顿家族".to_string();
        second_file.source_title = "布里杰顿家族".to_string();
        second_file.year = None;
        second_file.season_number = Some(2);
        second_file.episode_number = Some(1);

        let groups = super::group_discovered_files_for_scan(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![first_file, second_file],
        );

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].presentation.media_type, "series");
        assert_eq!(groups[0].presentation.title, "布里杰顿家族");
        assert_eq!(groups[0].presentation.lookup_title, "布里杰顿家族");
        assert_eq!(groups[0].presentation.year, None);
        assert_eq!(groups[0].files.len(), 2);
        assert_eq!(
            groups[0].presentation.item_key,
            "series-folder:布里杰顿家族 (2020)"
        );
    }

    #[test]
    fn group_discovered_files_for_scan_prefers_explicit_episode_file_title_over_folders() {
        let mut first_file = build_discovered_file();
        first_file.file_path = PathBuf::from("乱七八糟/版本A/我是电视剧.S01E01.mkv");
        first_file.title = "我是电视剧".to_string();
        first_file.source_title = "我是电视剧".to_string();
        first_file.year = None;
        first_file.season_number = Some(1);
        first_file.episode_number = Some(1);

        let mut second_file = build_discovered_file();
        second_file.file_path = PathBuf::from("另一个目录/完全不重要/我是电视剧.S02E01.mkv");
        second_file.title = "我是电视剧".to_string();
        second_file.source_title = "我是电视剧".to_string();
        second_file.year = None;
        second_file.season_number = Some(2);
        second_file.episode_number = Some(1);

        let groups = super::group_discovered_files_for_scan(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![first_file, second_file],
        );

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].presentation.media_type, "series");
        assert_eq!(groups[0].presentation.title, "我是电视剧");
        assert_eq!(groups[0].presentation.lookup_title, "我是电视剧");
        assert_eq!(groups[0].presentation.item_key, "series-title:我是电视剧");
        assert_eq!(groups[0].files.len(), 2);
    }

    #[test]
    fn group_discovered_files_for_scan_extracts_embedded_sxxexx_suffix_as_series() {
        let mut first_file = build_discovered_file();
        first_file.file_path = PathBuf::from("美丽毒素/S01/The.BeautyS01E01.2026.2160p.WEB-DL.mkv");
        first_file.title = "The Beauty".to_string();
        first_file.source_title = "The Beauty".to_string();
        first_file.year = Some(2026);
        first_file.season_number = Some(1);
        first_file.episode_number = Some(1);
        first_file.episode_title = None;

        let mut second_file = build_discovered_file();
        second_file.file_path =
            PathBuf::from("美丽毒素/S01/The.BeautyS01E02.2026.2160p.WEB-DL.mkv");
        second_file.title = "The Beauty".to_string();
        second_file.source_title = "The Beauty".to_string();
        second_file.year = Some(2026);
        second_file.season_number = Some(1);
        second_file.episode_number = Some(2);
        second_file.episode_title = None;

        let groups = super::group_discovered_files_for_scan(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![first_file, second_file],
        );

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].presentation.media_type, "series");
        assert_eq!(groups[0].presentation.title, "The Beauty");
        assert_eq!(groups[0].presentation.lookup_title, "The Beauty");
        assert_eq!(groups[0].presentation.year, Some(2026));
        assert_eq!(groups[0].files.len(), 2);
    }

    #[test]
    fn group_discovered_files_for_scan_keeps_plain_multi_file_folder_as_movies() {
        let mut first_file = build_discovered_file();
        first_file.file_path = PathBuf::from("a/aa/Pilot.mkv");
        first_file.title = "Pilot".to_string();
        first_file.source_title = "Pilot".to_string();
        first_file.season_number = None;
        first_file.episode_number = None;
        first_file.episode_title = None;

        let mut second_file = build_discovered_file();
        second_file.file_path = PathBuf::from("a/aa/Finale.mkv");
        second_file.title = "Finale".to_string();
        second_file.source_title = "Finale".to_string();
        second_file.season_number = None;
        second_file.episode_number = None;
        second_file.episode_title = None;

        let mut movie_file = build_discovered_file();
        movie_file.file_path = PathBuf::from("a/ab/How.to.Train.Your.Dragon.2025.mkv");
        movie_file.title = "How to Train Your Dragon".to_string();
        movie_file.source_title = "How to Train Your Dragon".to_string();
        movie_file.year = Some(2025);
        movie_file.season_number = None;
        movie_file.episode_number = None;
        movie_file.episode_title = None;

        let groups = super::group_discovered_files_for_scan(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![first_file, second_file, movie_file],
        );

        assert_eq!(groups.len(), 3);
        assert!(groups
            .iter()
            .all(|group| group.presentation.media_type == "movie"));
        assert!(groups
            .iter()
            .any(|group| group.presentation.title == "Pilot"));
        assert!(groups
            .iter()
            .any(|group| group.presentation.title == "Finale"));
        assert!(groups
            .iter()
            .any(|group| group.presentation.title == "How to Train Your Dragon"));
    }

    #[test]
    fn resolve_group_metadata_lookup_type_routes_files_without_episode_coordinates_to_movie() {
        let mut file = build_discovered_file();
        file.file_path = PathBuf::from("movies/Dune.2021.mkv");
        file.title = "Dune".to_string();
        file.source_title = "Dune".to_string();
        file.year = Some(2021);
        file.season_number = None;
        file.episode_number = None;
        file.episode_title = None;

        let groups =
            super::group_discovered_files_for_scan(&build_library(LIBRARY_TYPE_MIXED), vec![file]);
        let provider = FixedMetadataProvider { enabled: true };

        let decision = super::resolve_group_metadata_lookup_type(&provider, &groups[0]);

        assert_eq!(decision.lookup_type, Some("movie"));
        assert_eq!(decision.remote_media_type, None);
        assert_eq!(decision.metadata_status, METADATA_STATUS_PENDING);
        assert_eq!(
            decision.metadata_failure_reason,
            Some(METADATA_FAILURE_NO_REMOTE_MATCH)
        );
    }

    #[test]
    fn resolve_group_metadata_lookup_type_routes_explicit_episode_coordinates_to_series() {
        let file = build_discovered_file();
        let groups =
            super::group_discovered_files_for_scan(&build_library(LIBRARY_TYPE_MIXED), vec![file]);
        let provider = FixedMetadataProvider { enabled: true };

        let decision = super::resolve_group_metadata_lookup_type(&provider, &groups[0]);

        assert_eq!(decision.lookup_type, Some("series"));
        assert_eq!(decision.remote_media_type, None);
        assert_eq!(decision.metadata_status, METADATA_STATUS_PENDING);
    }

    #[test]
    fn resolve_group_metadata_lookup_type_trusts_existing_movie_binding() {
        let mut file = build_discovered_file();
        file.file_path = PathBuf::from("movies/Unexpected Family (2026).mkv");
        file.title = "过家家".to_string();
        file.source_title = "Unexpected Family".to_string();
        file.year = Some(2026);
        file.season_number = None;
        file.episode_number = None;
        file.episode_title = None;
        file.metadata_provider = Some(super::TMDB_PROVIDER_NAME.to_string());
        file.metadata_provider_item_id = Some(1_234_567);
        file.metadata_status = Some(METADATA_STATUS_UNMATCHED.to_string());
        file.remote_media_type = Some(REMOTE_MEDIA_TYPE_MOVIE.to_string());

        let groups =
            super::group_discovered_files_for_scan(&build_library(LIBRARY_TYPE_MIXED), vec![file]);
        let provider = FixedMetadataProvider { enabled: true };

        let decision = super::resolve_group_metadata_lookup_type(&provider, &groups[0]);

        assert_eq!(decision.lookup_type, Some("movie"));
        assert_eq!(decision.remote_media_type, Some(REMOTE_MEDIA_TYPE_MOVIE));
    }

    #[test]
    fn resolve_group_metadata_lookup_type_skips_tmdb_when_provider_is_disabled() {
        let file = build_discovered_file();
        let groups =
            super::group_discovered_files_for_scan(&build_library(LIBRARY_TYPE_MIXED), vec![file]);
        let provider = FixedMetadataProvider { enabled: false };

        let decision = super::resolve_group_metadata_lookup_type(&provider, &groups[0]);

        assert_eq!(decision.lookup_type, Some("series"));
        assert_eq!(decision.metadata_status, METADATA_STATUS_SKIPPED);
        assert_eq!(decision.remote_media_type, None);
    }

    #[test]
    fn clear_remote_metadata_for_review_restores_local_title_and_unbinds_remote_fields() {
        let mut file = build_discovered_file();
        file.title = "Remote Movie Title".to_string();
        file.source_title = "Local File Title".to_string();
        file.metadata_provider = Some("tmdb".to_string());
        file.metadata_provider_item_id = Some(123);
        file.original_title = Some("Remote Original".to_string());
        file.poster_path = Some("/cache/tmdb/poster.jpg".to_string());
        file.backdrop_path = Some("/cache/tmdb/backdrop.jpg".to_string());

        super::clear_remote_metadata_for_review(
            &mut file,
            METADATA_STATUS_UNMATCHED,
            Some(METADATA_FAILURE_NO_REMOTE_MATCH),
            None,
        );

        assert_eq!(file.title, "Local File Title");
        assert_eq!(file.metadata_provider, None);
        assert_eq!(file.metadata_provider_item_id, None);
        assert_eq!(
            file.metadata_status.as_deref(),
            Some(METADATA_STATUS_UNMATCHED)
        );
        assert_eq!(
            file.metadata_failure_reason.as_deref(),
            Some(METADATA_FAILURE_NO_REMOTE_MATCH)
        );
        assert_eq!(file.remote_media_type, None);
        assert_eq!(file.original_title, None);
        assert_eq!(file.poster_path, None);
        assert_eq!(file.backdrop_path, None);
    }

    #[test]
    fn build_media_entries_keeps_plain_series_folder_files_as_movies() {
        let mut first_file = build_discovered_file();
        first_file.file_path = PathBuf::from("/media/Arcane/Pilot.mkv");
        first_file.title = "Pilot".to_string();
        first_file.source_title = "Pilot".to_string();
        first_file.season_number = None;
        first_file.episode_number = None;
        first_file.episode_title = None;

        let mut second_file = build_discovered_file();
        second_file.file_path = PathBuf::from("/media/Arcane/Finale.mkv");
        second_file.title = "Finale".to_string();
        second_file.source_title = "Finale".to_string();
        second_file.season_number = None;
        second_file.episode_number = None;
        second_file.episode_title = None;

        let entries = super::build_media_entries(
            &build_library(LIBRARY_TYPE_SERIES),
            vec![first_file, second_file],
            true,
        )
        .unwrap();

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|entry| entry.media_type == "movie"));
        assert!(entries.iter().all(|entry| entry.season_number.is_none()));
        assert!(entries.iter().all(|entry| entry.episode_number.is_none()));
        assert!(entries.iter().any(|entry| entry.title == "Finale"));
        assert!(entries.iter().any(|entry| entry.title == "Pilot"));
    }

    #[test]
    fn group_discovered_files_for_scan_keeps_multi_version_movie_folder_as_movie() {
        let mut first_file = build_discovered_file();
        first_file.file_path = PathBuf::from("Movie/Movie.2025.1080p.mkv");
        first_file.title = "Movie".to_string();
        first_file.source_title = "Movie".to_string();
        first_file.year = Some(2025);
        first_file.season_number = None;
        first_file.episode_number = None;
        first_file.episode_title = None;

        let mut second_file = build_discovered_file();
        second_file.file_path = PathBuf::from("Movie/Movie.2025.2160p.mkv");
        second_file.title = "Movie".to_string();
        second_file.source_title = "Movie".to_string();
        second_file.year = Some(2025);
        second_file.season_number = None;
        second_file.episode_number = None;
        second_file.episode_title = None;

        let groups = super::group_discovered_files_for_scan(
            &build_library(LIBRARY_TYPE_MIXED),
            vec![first_file, second_file],
        );

        assert_eq!(groups.len(), 2);
        assert!(groups
            .iter()
            .all(|group| group.presentation.media_type == "movie"));
    }

    #[test]
    fn apply_existing_movie_metadata_reuses_stored_remote_fields() {
        let mut file = build_discovered_file();
        file.file_path = PathBuf::from("/media/movies/Arcane.mkv");
        file.title = "Arcane.2021.2160p".to_string();
        file.source_title = "Arcane".to_string();
        file.original_title = None;
        file.overview = None;
        file.poster_path = None;
        file.backdrop_path = None;
        file.country = None;
        file.genres = None;
        file.studio = None;

        super::apply_existing_media_metadata(&mut file, &build_existing_movie_metadata());

        assert_eq!(file.title, "Arcane");
        assert_eq!(file.original_title.as_deref(), Some("Arcane Original"));
        assert_eq!(file.overview.as_deref(), Some("Stored overview"));
        assert_eq!(file.poster_path.as_deref(), Some("/cache/poster.jpg"));
        assert_eq!(file.backdrop_path.as_deref(), Some("/cache/backdrop.jpg"));
        assert_eq!(file.country.as_deref(), Some("United States"));
        assert_eq!(file.genres.as_deref(), Some("Animation, Drama"));
        assert_eq!(file.studio.as_deref(), Some("Fortiche"));
        assert_eq!(file.year, Some(2021));
    }

    #[test]
    fn apply_existing_unmatched_metadata_keeps_fresh_local_title() {
        let mut file = build_discovered_file();
        file.file_path = PathBuf::from(
            "/media/movies/惊变28年2白骨圣殿(2026)/28.Years.Later.The.Bone.Temple.2026.mkv",
        );
        file.title = "28 Years Later The Bone Temple".to_string();
        file.source_title = "28 Years Later The Bone Temple".to_string();
        file.year = Some(2026);

        let mut existing = build_existing_movie_metadata();
        existing.metadata_provider = None;
        existing.metadata_provider_item_id = None;
        existing.metadata_status = METADATA_STATUS_UNMATCHED.to_string();
        existing.metadata_failure_reason = Some(METADATA_FAILURE_NO_REMOTE_MATCH.to_string());
        existing.title = "Years Later The Bone Temple".to_string();
        existing.source_title = "Years Later The Bone Temple".to_string();
        existing.year = Some(2026);

        super::apply_existing_media_metadata(&mut file, &existing);

        assert_eq!(file.title, "28 Years Later The Bone Temple");
        assert_eq!(file.source_title, "28 Years Later The Bone Temple");
        assert_eq!(file.year, Some(2026));
    }

    #[test]
    fn apply_existing_episode_metadata_reuses_series_and_episode_fields() {
        let mut file = build_discovered_file();
        file.title = "Arcane.S01E01".to_string();
        file.source_title = "Arcane.S01E01".to_string();
        file.original_title = None;
        file.sort_title = None;
        file.year = Some(2020);
        file.country = None;
        file.genres = None;
        file.studio = None;
        file.overview = None;
        file.series_poster_path = None;
        file.series_backdrop_path = None;
        file.season_title = None;
        file.season_overview = None;
        file.season_poster_path = None;
        file.season_backdrop_path = None;
        file.poster_path = None;
        file.backdrop_path = None;

        super::apply_existing_media_metadata(&mut file, &build_existing_episode_metadata());

        assert_eq!(file.title, "Arcane");
        assert_eq!(file.original_title.as_deref(), Some("Arcane Original"));
        assert_eq!(file.sort_title.as_deref(), Some("Arcane, The"));
        assert_eq!(file.year, Some(2021));
        assert_eq!(file.overview.as_deref(), Some("Series overview"));
        assert_eq!(
            file.series_poster_path.as_deref(),
            Some("/cache/series-poster.jpg")
        );
        assert_eq!(file.season_title.as_deref(), Some("Season 01"));
        assert_eq!(
            file.episode_title.as_deref(),
            Some("Welcome to the Playground")
        );
        assert_eq!(
            file.poster_path.as_deref(),
            Some("/cache/episode-poster.jpg")
        );
        assert_eq!(
            file.backdrop_path.as_deref(),
            Some("/cache/episode-backdrop.jpg")
        );
    }
}

async fn record_failed_scan_attempt(
    pool: &PgPool,
    scan_job_id: i64,
    total_files: i32,
    scanned_files: i32,
    error_message: &str,
) {
    if let Err(error) = mova_db::record_scan_job_attempt_failure(
        pool,
        scan_job_id,
        total_files,
        scanned_files,
        error_message,
    )
    .await
    {
        tracing::warn!(
            scan_job_id,
            error = ?error,
            "failed to persist scan attempt failure context"
        );
    }
}

async fn finalize_cancelled_scan(
    pool: &PgPool,
    scan_job_id: i64,
    total_files: i32,
    scanned_files: i32,
) -> Option<ScanJob> {
    mova_db::finalize_scan_job(
        pool,
        scan_job_id,
        "failed",
        total_files,
        scanned_files,
        Some("scan cancelled"),
        None,
    )
    .await
    .ok()
    .flatten()
}
