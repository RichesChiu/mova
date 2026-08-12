use crate::{
    error::{ApplicationError, ApplicationResult},
    libraries::get_library,
    media_classification::{
        apply_movie_container_identity_when_title_is_missing, classify_media_type,
    },
    media_enrichment::{
        sanitize_file_artwork_sources, trusted_artwork_bases, MetadataEnrichmentContext,
        MetadataEnrichmentOutcome, MetadataEnrichmentStage,
    },
    metadata::{MetadataProvider, MetadataSeasonAirYearHint, TMDB_PROVIDER_NAME},
};
use futures_util::{stream, StreamExt, TryStreamExt};
use mova_db::BackgroundJobFence;
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
    infer_series_file_metadata, DiscoveredAudioTrack, DiscoveredMediaFile,
    DiscoveredMediaFileInventory, DiscoveredSubtitleTrack, MediaDiscoveryIssue,
    MediaDiscoveryReport,
};
use sqlx::postgres::PgPool;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicI32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

mod incremental;
mod presentation;

use incremental::*;
use presentation::*;

pub(crate) fn apply_existing_media_metadata_for_refresh(
    file: &mut DiscoveredMediaFile,
    summary: &mova_db::ExistingMediaMetadataSummary,
) {
    presentation::apply_existing_media_metadata(file, summary);
}

pub(crate) fn eligible_local_nfo_observations_for_refresh(
    files: &[DiscoveredMediaFile],
    root_path: &Path,
) -> Vec<(
    Option<mova_scan::LocalNfoObservation>,
    Option<mova_scan::LocalNfoObservation>,
)> {
    eligible_local_nfo_observations(files, root_path)
}

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
    Completed(MediaDiscoveryReport),
    Cancelled(i32),
}

#[derive(Debug)]
enum InspectIncrementalScanFilesOutcome {
    Completed(Vec<DiscoveredMediaFile>),
    Cancelled,
}

#[derive(Debug, Clone)]
struct LocalSeriesGroup {
    lookup_title: String,
    display_title: String,
    year: Option<i32>,
    year_priority: u8,
    identity_priority: u8,
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
    metadata_lookup_hint: Option<String>,
    metadata_binding_conflict: bool,
}

#[derive(Debug)]
struct QueuedScanGroup {
    group: ScanDiscoveredGroup,
    item_index: i32,
    total_items: i32,
}

struct PreparedRemoteScanGroup {
    group: ScanDiscoveredGroup,
    item_index: i32,
    total_items: i32,
    artwork_publication: Option<mova_db::TmdbArtworkPublicationGuard>,
    enrichment_outcome: Option<MetadataEnrichmentOutcome>,
    failure_detail: Option<String>,
    materialized_artwork: std::collections::BTreeSet<String>,
    allow_artwork_clear: bool,
    replace_remote_data: bool,
    retain_materialized_artwork: bool,
}

#[derive(Debug, Default)]
struct RemoteScanPipelineOutcome {
    sync: mova_db::SyncLibraryMediaBestEffortOutcome,
    notification_summary: ScanNotificationSummary,
    elapsed_ms: u64,
    processed_groups: usize,
}

#[derive(Debug, Default)]
struct LocalScanPipelineOutcome {
    sync: mova_db::SyncLibraryMediaBestEffortOutcome,
    elapsed_ms: u64,
    processed_groups: usize,
    subtitle_directories: usize,
}

#[derive(Debug)]
struct ScanGroupSyncOutcome {
    sync: mova_db::SyncLibraryMediaBestEffortOutcome,
    scan_job: ScanJob,
}

#[derive(Debug)]
struct PendingScanGroup {
    files: Vec<IncrementalScanFile>,
    metadata_lookup_hint: Option<String>,
    metadata_binding_conflict: bool,
    eligible_generic_movie_nfo_sources: HashSet<PathBuf>,
    eligible_series_nfo_sources: HashSet<PathBuf>,
}

#[derive(Debug)]
struct IncrementalScanPlan {
    discovered_paths: Vec<String>,
    scan_files: Vec<IncrementalScanFile>,
    container_bindings: HashMap<String, ContainerBindingResolution>,
}

#[derive(Debug, Clone)]
struct IncrementalScanFile {
    inventory: DiscoveredMediaFileInventory,
    existing_metadata: Option<mova_db::ExistingMediaMetadataSummary>,
    /// True when this physical file, or another carrier of the same logical
    /// movie/series metadata owner, changed during this discovery pass.
    requires_processing: bool,
}

#[derive(Debug)]
struct PendingScanFile {
    changed_file: IncrementalScanFile,
    file: DiscoveredMediaFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ContainerBindingResolution {
    Unique(String),
    Conflict,
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
// Four remote slots are the deliberately conservative production default.
// The deterministic 120-group capacity model measured 3.82x over one slot at
// four workers, while eight slots have not been validated against real TMDB,
// artwork bandwidth, memory, and database pressure. One uncontrolled real A/B
// proved correctness but not wall-clock speedup, so it is not evidence for
// raising this limit. Keep each queue at two: increasing it to sixteen did not
// improve modeled wall time and raised p95 queue latency at four slots. Keep
// this rationale and the measurements synchronized with SCAN_PERFORMANCE.md.
const REMOTE_ENRICHMENT_CONCURRENCY: usize = 4;
const REMOTE_ENRICHMENT_QUEUE_CAPACITY: usize = 2;
pub(crate) const LOCAL_ANALYSIS_VERSION: i32 = 9;

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
    fence: &BackgroundJobFence,
    event_listener: &Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> ApplicationResult<Option<i32>> {
    match mova_db::update_scan_job_progress(pool, scan_job_id, None, scanned_files, fence).await {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_DISCOVERING,
            )));
            Ok(Some(scanned_files))
        }
        Ok(None) => Ok(None),
        Err(error) => Err(ApplicationError::Unexpected(error)),
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

/// 执行可取消的扫描任务。
/// 当库正在删除或任务已被外部终止时，返回 `Cancelled` 而不是把它当成系统故障。
#[allow(clippy::too_many_arguments)]
pub async fn execute_scan_job_with_cancellation(
    pool: &PgPool,
    library_id: i64,
    scan_job_id: i64,
    fence: BackgroundJobFence,
    cancellation_flag: Arc<AtomicBool>,
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> ApplicationResult<ExecuteScanJobOutcome> {
    let scan_started_at = Instant::now();
    if is_cancelled(&cancellation_flag) {
        if let Some(scan_job) =
            finalize_cancelled_scan(pool, library_id, scan_job_id, 0, 0, &fence).await?
        {
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
            if let Some(scan_job) =
                finalize_cancelled_scan(pool, library_id, scan_job_id, 0, 0, &fence).await?
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
                SCAN_PHASE_INITIALIZING,
                format!("Failed to load library configuration: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, 0, 0, &message, &fence).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    };

    if is_cancelled(&cancellation_flag) {
        if let Some(scan_job) =
            finalize_cancelled_scan(pool, library_id, scan_job_id, 0, 0, &fence).await?
        {
            event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_FINISHED,
            )));
        }
        return Ok(ExecuteScanJobOutcome::Cancelled);
    }

    match mova_db::mark_scan_job_running(pool, scan_job_id, &fence).await {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_DISCOVERING,
            )));
        }
        Ok(None) => {
            if let Some(scan_job) =
                finalize_cancelled_scan(pool, library_id, scan_job_id, 0, 0, &fence).await?
            {
                event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                    scan_job,
                    SCAN_PHASE_FINISHED,
                )));
            }
            return Ok(ExecuteScanJobOutcome::Cancelled);
        }
        Err(error) => {
            let error = ApplicationError::from(error);
            let message = format_scan_phase_error(
                SCAN_PHASE_INITIALIZING,
                format!("Failed to start the scan job: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, 0, 0, &message, &fence).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    }

    if is_cancelled(&cancellation_flag) {
        if let Some(scan_job) =
            finalize_cancelled_scan(pool, library_id, scan_job_id, 0, 0, &fence).await?
        {
            event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_FINISHED,
            )));
        }
        return Ok(ExecuteScanJobOutcome::Cancelled);
    }

    let mut sync_outcome = mova_db::SyncLibraryMediaBestEffortOutcome::default();

    let discovery_started_at = Instant::now();
    let discovery_report = match discover_media_files(
        pool,
        scan_job_id,
        &library,
        &fence,
        cancellation_flag.clone(),
        event_listener.clone(),
    )
    .await
    {
        Ok(DiscoverMediaFilesOutcome::Completed(report)) => report,
        Ok(DiscoverMediaFilesOutcome::Cancelled(scanned_files)) => {
            if let Some(scan_job) = finalize_cancelled_scan(
                pool,
                library_id,
                scan_job_id,
                scanned_files,
                scanned_files,
                &fence,
            )
            .await?
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
            record_failed_scan_attempt(pool, scan_job_id, 0, 0, &message, &fence).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    };
    let discovery_elapsed_ms = elapsed_millis(discovery_started_at);

    let total_files = i32::try_from(
        discovery_report
            .files
            .len()
            .saturating_add(discovery_report.issues.len()),
    )
    .unwrap_or(i32::MAX);
    let MediaDiscoveryReport {
        files: discovered_files,
        issues: discovery_issues,
    } = discovery_report;
    match mova_db::update_scan_job_progress(
        pool,
        scan_job_id,
        Some(total_files),
        total_files,
        &fence,
    )
    .await
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

    let planning_started_at = Instant::now();
    let IncrementalScanPlan {
        discovered_paths,
        scan_files,
        container_bindings,
    } = match build_incremental_scan_plan(
        pool,
        library.id,
        std::path::Path::new(&library.root_path),
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
            record_failed_scan_attempt(pool, scan_job_id, total_files, 0, &message, &fence).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    };

    let pending_groups = match build_pending_scan_groups(
        scan_files,
        std::path::Path::new(&library.root_path),
        &container_bindings,
    )
    .await
    {
        Ok(groups) => groups,
        Err(error) => {
            let message = format_scan_phase_error(
                SCAN_PHASE_INITIALIZING,
                format!("Failed to plan changed media groups: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, total_files, 0, &message, &fence).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    };
    let planning_elapsed_ms = elapsed_millis(planning_started_at);
    let pending_file_count = pending_groups
        .iter()
        .map(|group| group.files.len())
        .sum::<usize>();
    let pending_file_count = i32::try_from(pending_file_count).unwrap_or(i32::MAX);

    match mova_db::initialize_scan_job_work(
        pool,
        scan_job_id,
        total_files,
        pending_file_count,
        &fence,
    )
    .await
    {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_PROCESSING,
            )));
        }
        Ok(None) => {
            if let Some(scan_job) =
                finalize_cancelled_scan(pool, library_id, scan_job_id, total_files, 0, &fence)
                    .await?
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
                SCAN_PHASE_PROCESSING,
                format!("Failed to initialize scan pipeline: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, total_files, 0, &message, &fence).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    }

    let total_items = i32::try_from(pending_groups.len()).unwrap_or(i32::MAX);
    let (group_sender, group_receiver) = mpsc::channel(REMOTE_ENRICHMENT_QUEUE_CAPACITY);
    let local_artwork_trust = trusted_artwork_bases(metadata_provider.as_ref());
    let pipeline_started_at = Instant::now();
    let pipeline_result = tokio::try_join!(
        analyze_pending_scan_groups(
            LocalScanPipelineContext {
                pool,
                library: &library,
                scan_job_id,
                fence: &fence,
                group_sender,
                cancellation_flag: cancellation_flag.clone(),
                trusted_artwork_bases: local_artwork_trust,
                event_listener: event_listener.clone(),
            },
            pending_groups,
        ),
        enrich_discovered_groups(
            pool,
            &library,
            scan_job_id,
            &fence,
            group_receiver,
            total_items,
            cancellation_flag.clone(),
            artwork_cache_dir,
            metadata_provider.clone(),
            event_listener.clone(),
        )
    );

    let pipeline_elapsed_ms = elapsed_millis(pipeline_started_at);
    let (mut notification_summary, local_metrics, remote_metrics) = match pipeline_result {
        Ok((local_outcome, remote_outcome)) => {
            merge_sync_outcome(&mut sync_outcome, local_outcome.sync);
            merge_sync_outcome(&mut sync_outcome, remote_outcome.sync);
            let local_metrics = (
                local_outcome.elapsed_ms,
                local_outcome.processed_groups,
                local_outcome.subtitle_directories,
            );
            let remote_metrics = (remote_outcome.elapsed_ms, remote_outcome.processed_groups);
            (
                remote_outcome.notification_summary,
                local_metrics,
                remote_metrics,
            )
        }
        Err(error) => {
            if let Err(cleanup_error) = mova_db::cleanup_library_orphan_series_after_scan(
                pool,
                library.id,
                scan_job_id,
                &fence,
                false,
            )
            .await
            {
                tracing::warn!(
                    library_id = library.id,
                    scan_job_id,
                    error = ?cleanup_error,
                    "failed to clean hierarchy after interrupted scan pipeline"
                );
            }
            let message = format_scan_phase_error(
                SCAN_PHASE_PROCESSING,
                format!("Failed to process scan pipeline: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, total_files, 0, &message, &fence).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    };
    record_media_discovery_issues(&mut notification_summary, &discovery_issues);

    if is_cancelled(&cancellation_flag) {
        if let Err(cleanup_error) = mova_db::cleanup_library_orphan_series_after_scan(
            pool,
            library.id,
            scan_job_id,
            &fence,
            true,
        )
        .await
        {
            tracing::warn!(
                library_id = library.id,
                scan_job_id,
                error = ?cleanup_error,
                "failed to clean hierarchy after cancelled scan pipeline"
            );
        }
        if let Some(scan_job) = finalize_cancelled_scan(
            pool,
            library_id,
            scan_job_id,
            total_files,
            total_files,
            &fence,
        )
        .await?
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
        &fence,
        event_listener.clone(),
    )
    .await?;

    let finalization_io_started_at = Instant::now();
    let authoritative_root_path = PathBuf::from(&library.root_path);
    let authoritative_discovered_paths = discovered_paths.clone();
    let retained_local_metadata_source_paths = tokio::task::spawn_blocking(move || {
        authoritative_local_metadata_source_paths(
            &authoritative_root_path,
            &authoritative_discovered_paths,
        )
    })
    .await
    .map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "The local metadata finalization worker exited unexpectedly: {error}"
        ))
    })?;
    let finalization_io_elapsed_ms = elapsed_millis(finalization_io_started_at);

    // Only a complete discovery result is authoritative enough to remove missing paths.
    // A cancelled or failed traversal returns before this point, so transient mount,
    // permission, or I/O failures cannot be mistaken for deleted media files.
    let finalization_db_started_at = Instant::now();
    let removal_outcome = match mova_db::sync_library_media_changes(
        pool,
        library.id,
        scan_job_id,
        &discovered_paths,
        !discovery_issues.is_empty(),
        &retained_local_metadata_source_paths,
        &[],
        &fence,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            let message = format_scan_phase_error(
                SCAN_PHASE_FINALIZING,
                format!("Failed to reconcile missing media files: {}", error),
            );
            record_failed_scan_attempt(pool, scan_job_id, total_files, 0, &message, &fence).await;
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
        }
    };
    let finalization_db_elapsed_ms = elapsed_millis(finalization_db_started_at);
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

        record_failed_scan_attempt(pool, scan_job_id, total_files, 0, &message, &fence).await;
        return Err(ApplicationError::Unexpected(anyhow::anyhow!(message)));
    }

    match mova_db::finalize_scan_job(
        pool,
        library.id,
        scan_job_id,
        "success",
        total_files,
        total_files,
        None,
        Some(&notification_summary),
        &fence,
    )
    .await
    {
        Ok(Some(scan_job)) => {
            tracing::info!(
                event = "library_scan_performance",
                library_id = library.id,
                scan_job_id,
                total_files,
                pending_files = pending_file_count,
                pending_groups = total_items,
                local_groups = local_metrics.1,
                remote_groups = remote_metrics.1,
                remote_concurrency = REMOTE_ENRICHMENT_CONCURRENCY,
                remote_queue_capacity = REMOTE_ENRICHMENT_QUEUE_CAPACITY,
                subtitle_directories = local_metrics.2,
                discovery_ms = discovery_elapsed_ms,
                planning_ms = planning_elapsed_ms,
                pipeline_wall_ms = pipeline_elapsed_ms,
                local_pipeline_ms = local_metrics.0,
                remote_pipeline_ms = remote_metrics.0,
                finalization_io_ms = finalization_io_elapsed_ms,
                finalization_db_ms = finalization_db_elapsed_ms,
                total_ms = elapsed_millis(scan_started_at),
                removed_count = sync_outcome.removed_count,
                upserted_count = sync_outcome.upserted_count,
                failed_count = sync_outcome.failed_count,
                "library scan performance summary"
            );
            event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                scan_job.clone(),
                SCAN_PHASE_FINISHED,
            )));
            if scan_job.status == "cancelled" {
                Ok(ExecuteScanJobOutcome::Cancelled)
            } else {
                Ok(ExecuteScanJobOutcome::Completed(scan_job))
            }
        }
        Ok(None) => Ok(ExecuteScanJobOutcome::Cancelled),
        Err(error) => Err(ApplicationError::from(error)),
    }
}

/// Resolve the exact sidecar paths that remain eligible after a complete file
/// discovery. Candidate precedence mirrors `mova-scan`: an existing invalid or
/// unreadable higher-priority NFO blocks fallback and is retained as
/// last-known-good; only a definitive `NotFound` advances to the next path.
fn authoritative_local_metadata_source_paths(
    root_path: &Path,
    discovered_paths: &[String],
) -> Vec<String> {
    let mut retained = std::collections::BTreeSet::new();
    let canonical_root = fs::canonicalize(root_path).ok();

    let shallow_files = discovered_paths
        .iter()
        .filter_map(|discovered_path| {
            mova_scan::inspect_media_file_inventory_shallow(DiscoveredMediaFileInventory {
                file_path: PathBuf::from(discovered_path),
                source_kind: if Path::new(discovered_path)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("strm"))
                {
                    mova_domain::MediaSourceKind::Strm
                } else {
                    mova_domain::MediaSourceKind::LocalFile
                },
                stream_reference_hash: None,
                file_size: 0,
                file_modified_at_ms: None,
                sidecar_fingerprint: String::new(),
            })
            .ok()
        })
        .collect::<Vec<_>>();
    for (media_observation, series_observation) in
        eligible_local_nfo_observations(&shallow_files, root_path)
    {
        for source_path in [media_observation.as_ref(), series_observation.as_ref()]
            .into_iter()
            .flatten()
            .filter_map(nfo_observation_source_path)
        {
            retain_local_metadata_source_path(
                &mut retained,
                source_path,
                root_path,
                canonical_root.as_deref(),
            );
        }
    }

    retained.into_iter().collect()
}

fn retain_local_metadata_source_path(
    retained: &mut std::collections::BTreeSet<String>,
    source_path: &Path,
    root_path: &Path,
    canonical_root: Option<&Path>,
) {
    retained.insert(source_path.to_string_lossy().to_string());
    let Some(canonical_root) = canonical_root else {
        return;
    };
    if let Ok(canonical_source) = fs::canonicalize(source_path) {
        if canonical_source.starts_with(canonical_root) {
            retained.insert(canonical_source.to_string_lossy().to_string());
            if let Ok(relative_source) = canonical_source.strip_prefix(canonical_root) {
                retained.insert(
                    root_path
                        .join(relative_source)
                        .to_string_lossy()
                        .to_string(),
                );
            }
        }
    }
}

async fn build_pending_scan_groups(
    scan_files: Vec<IncrementalScanFile>,
    root_path: &std::path::Path,
    container_bindings: &HashMap<String, ContainerBindingResolution>,
) -> ApplicationResult<Vec<PendingScanGroup>> {
    let pending_files =
        inspect_incremental_scan_files_shallow(scan_files, root_path.to_path_buf()).await?;

    Ok(build_pending_scan_groups_from_files(
        pending_files,
        root_path,
        container_bindings,
    ))
}

fn build_pending_scan_groups_from_files(
    pending_files: Vec<PendingScanFile>,
    root_path: &std::path::Path,
    container_bindings: &HashMap<String, ContainerBindingResolution>,
) -> Vec<PendingScanGroup> {
    let mut scan_files_by_path = HashMap::new();
    let mut metadata_owner_by_path = HashMap::new();
    let mut shallow_files = Vec::with_capacity(pending_files.len());

    for pending_file in pending_files {
        let file_path = pending_file.file.file_path.to_string_lossy().to_string();
        shallow_files.push(pending_file.file);
        if let Some(summary) = pending_file.changed_file.existing_metadata.as_ref() {
            metadata_owner_by_path.insert(file_path.clone(), summary.logical_metadata_owner_id);
        }
        scan_files_by_path.insert(file_path, pending_file.changed_file);
    }

    let groups = group_discovered_files_for_scan_with_root(shallow_files, root_path);
    let mut groups = coalesce_scan_groups_by_metadata_owner(groups, &metadata_owner_by_path);
    let mut pending_groups = Vec::with_capacity(groups.len());

    for mut group in groups.drain(..) {
        apply_container_binding_resolution(&mut group, root_path, container_bindings);
        let mut group_files = Vec::with_capacity(group.files.len());
        let mut eligible_generic_movie_nfo_sources = HashSet::new();
        let mut eligible_series_nfo_sources = HashSet::new();

        for file in group.files {
            let file_path = file.file_path.to_string_lossy().to_string();
            if let Some(source_path) = file
                .local_nfo
                .as_ref()
                .map(|metadata| metadata.source_path.as_path())
                .or(file.invalid_local_nfo_source_path.as_deref())
                .filter(|path| is_generic_movie_nfo(path))
            {
                eligible_generic_movie_nfo_sources.insert(normalize_nfo_source_path(source_path));
            }
            if let Some(source_path) = file
                .series_local_nfo
                .as_ref()
                .map(|metadata| metadata.source_path.as_path())
                .or(file.invalid_series_local_nfo_source_path.as_deref())
            {
                eligible_series_nfo_sources.insert(normalize_nfo_source_path(source_path));
            }
            if let Some(scan_file) = scan_files_by_path.remove(file_path.as_str()) {
                group_files.push(scan_file);
            }
        }

        if group_files.is_empty() || !group_files.iter().any(|file| file.requires_processing) {
            continue;
        }

        pending_groups.push(PendingScanGroup {
            files: group_files,
            metadata_lookup_hint: group.metadata_lookup_hint,
            metadata_binding_conflict: group.metadata_binding_conflict,
            eligible_generic_movie_nfo_sources,
            eligible_series_nfo_sources,
        });
    }

    pending_groups
}

/// A sidecar belongs to a logical movie/series rather than to one physical
/// version. Reconcile every live carrier of the same existing owner together,
/// even when filename grouping temporarily differs because a sidecar was
/// added, changed, or removed.
fn coalesce_scan_groups_by_metadata_owner(
    groups: Vec<ScanDiscoveredGroup>,
    metadata_owner_by_path: &HashMap<String, i64>,
) -> Vec<ScanDiscoveredGroup> {
    let mut coalesced = Vec::<ScanDiscoveredGroup>::new();
    let mut indexes = HashMap::<String, usize>::new();

    for mut group in groups {
        let owner_ids = group
            .files
            .iter()
            .filter_map(|file| {
                metadata_owner_by_path
                    .get(file.file_path.to_string_lossy().as_ref())
                    .copied()
            })
            .collect::<HashSet<_>>();
        let key = if owner_ids.len() == 1 {
            format!(
                "metadata-owner:{}",
                owner_ids
                    .iter()
                    .next()
                    .expect("one-element owner set must contain an id")
            )
        } else {
            group.presentation.item_key.clone()
        };
        group.presentation.item_key = key.clone();

        if let Some(index) = indexes.get(&key).copied() {
            let target = &mut coalesced[index];
            if group.metadata_binding_conflict {
                target.metadata_lookup_hint = None;
                target.metadata_binding_conflict = true;
            } else {
                merge_metadata_lookup_hint(target, group.metadata_lookup_hint.take());
            }
            target.files.extend(group.files);
            continue;
        }

        indexes.insert(key, coalesced.len());
        coalesced.push(group);
    }

    coalesced
}

fn apply_container_binding_resolution(
    group: &mut ScanDiscoveredGroup,
    root_path: &std::path::Path,
    container_bindings: &HashMap<String, ContainerBindingResolution>,
) {
    if group.metadata_binding_conflict || group.metadata_lookup_hint.is_some() {
        return;
    }

    let lookup_type = scan_presentation_lookup_type(&group.presentation);
    if lookup_type == "movie"
        && group
            .files
            .iter()
            .all(|file| mova_scan::has_meaningful_file_title(&file.file_path))
    {
        return;
    }

    let Some(container_key) = group
        .files
        .first()
        .and_then(|file| metadata_container_key_for_path(&file.file_path, root_path, lookup_type))
    else {
        return;
    };

    match container_bindings.get(&container_key) {
        Some(ContainerBindingResolution::Unique(provider_item_id)) => {
            group.metadata_lookup_hint = Some(provider_item_id.clone());
        }
        Some(ContainerBindingResolution::Conflict) => {
            group.metadata_binding_conflict = true;
        }
        None => {}
    }
}

fn merge_pending_group_lookup_state(
    group: &mut ScanDiscoveredGroup,
    pending_lookup_hint: Option<&str>,
    pending_binding_conflict: bool,
) {
    if group.metadata_binding_conflict {
        return;
    }

    if let (Some(current), Some(pending)) =
        (group.metadata_lookup_hint.as_deref(), pending_lookup_hint)
    {
        if current != pending {
            group.metadata_lookup_hint = None;
            group.metadata_binding_conflict = true;
        }
        return;
    }

    if group.metadata_lookup_hint.is_some() {
        return;
    }

    if pending_binding_conflict {
        group.metadata_binding_conflict = true;
    } else {
        group.metadata_lookup_hint = pending_lookup_hint.map(str::to_string);
    }
}

struct LocalScanPipelineContext<'a> {
    pool: &'a PgPool,
    library: &'a Library,
    scan_job_id: i64,
    fence: &'a BackgroundJobFence,
    group_sender: mpsc::Sender<QueuedScanGroup>,
    cancellation_flag: Arc<AtomicBool>,
    trusted_artwork_bases: Arc<Vec<reqwest::Url>>,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
}

async fn analyze_pending_scan_groups(
    context: LocalScanPipelineContext<'_>,
    pending_groups: Vec<PendingScanGroup>,
) -> ApplicationResult<LocalScanPipelineOutcome> {
    let started_at = Instant::now();
    let LocalScanPipelineContext {
        pool,
        library,
        scan_job_id,
        fence,
        group_sender,
        cancellation_flag,
        trusted_artwork_bases,
        event_listener,
    } = context;
    let total_items = i32::try_from(pending_groups.len()).unwrap_or(i32::MAX);
    let mut processed_items = 0_i32;
    let mut sync_outcome = mova_db::SyncLibraryMediaBestEffortOutcome::default();
    let mut completed_all_local_groups = true;
    let subtitle_video_paths = pending_groups
        .iter()
        .flat_map(|group| group.files.iter())
        .map(|file| file.inventory.file_path.clone())
        .collect::<Vec<_>>();
    let subtitle_index = tokio::task::spawn_blocking(move || {
        Arc::new(mova_scan::SubtitleDirectoryIndex::build(
            subtitle_video_paths.iter().map(PathBuf::as_path),
        ))
    })
    .await
    .map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "The subtitle directory index worker exited unexpectedly: {error}"
        ))
    })?;
    let subtitle_directories = subtitle_index.directory_count();

    'pending_groups: for pending_group in pending_groups {
        if is_cancelled(&cancellation_flag) {
            completed_all_local_groups = false;
            break;
        }

        let PendingScanGroup {
            files,
            metadata_lookup_hint,
            metadata_binding_conflict,
            eligible_generic_movie_nfo_sources,
            eligible_series_nfo_sources,
        } = pending_group;
        let metadata_owner_by_path = files
            .iter()
            .filter_map(|file| {
                file.existing_metadata.as_ref().map(|summary| {
                    (
                        file.inventory.file_path.to_string_lossy().to_string(),
                        summary.logical_metadata_owner_id,
                    )
                })
            })
            .collect::<HashMap<_, _>>();
        let discovered_files = match inspect_incremental_scan_files_within_root(
            files,
            PathBuf::from(&library.root_path),
            eligible_generic_movie_nfo_sources,
            eligible_series_nfo_sources,
            subtitle_index.clone(),
            cancellation_flag.clone(),
        )
        .await?
        {
            InspectIncrementalScanFilesOutcome::Completed(files) => files,
            InspectIncrementalScanFilesOutcome::Cancelled => {
                completed_all_local_groups = false;
                break;
            }
        };
        let groups = group_discovered_files_for_scan_with_root(
            discovered_files,
            std::path::Path::new(&library.root_path),
        );
        let mut groups = coalesce_scan_groups_by_metadata_owner(groups, &metadata_owner_by_path);
        for group in &mut groups {
            merge_pending_group_lookup_state(
                group,
                metadata_lookup_hint.as_deref(),
                metadata_binding_conflict,
            );
        }

        prepare_scan_groups_for_metadata_lookup(&mut groups);
        for group in &mut groups {
            for file in &mut group.files {
                sanitize_file_artwork_sources(file, &trusted_artwork_bases);
            }
        }

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
                fence,
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
                false,
                None,
                false,
                fence,
            )
            .await?;
            merge_sync_outcome(&mut sync_outcome, group_outcome.sync);

            event_listener(ScanJobEvent::ItemUpdated(build_scan_group_progress_update(
                scan_job_id,
                library.id,
                &group.presentation,
                group.files.first(),
                item_index,
                effective_total_items,
                ScanItemStage::PendingCommitted,
            )));

            event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                group_outcome.scan_job,
                SCAN_PHASE_PROCESSING,
            )));
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

    Ok(LocalScanPipelineOutcome {
        sync: sync_outcome,
        elapsed_ms: elapsed_millis(started_at),
        processed_groups: usize::try_from(processed_items).unwrap_or(usize::MAX),
        subtitle_directories,
    })
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

        let lookup_type = scan_presentation_lookup_type(&group.presentation);
        let local_selection =
            crate::local_metadata::apply_group_local_metadata(&mut group.files, lookup_type);
        if local_selection.identity_conflict {
            group.metadata_lookup_hint = None;
            group.metadata_binding_conflict = true;
        } else {
            merge_metadata_lookup_hint(group, local_selection.tmdb_id_hint);
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn enrich_discovered_groups(
    pool: &PgPool,
    library: &Library,
    scan_job_id: i64,
    fence: &BackgroundJobFence,
    group_receiver: mpsc::Receiver<QueuedScanGroup>,
    total_items: i32,
    cancellation_flag: Arc<AtomicBool>,
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> ApplicationResult<RemoteScanPipelineOutcome> {
    let started_at = Instant::now();
    let enrichment = MetadataEnrichmentContext::new(
        artwork_cache_dir.clone(),
        library.id,
        metadata_provider.clone(),
        library.metadata_language.clone(),
    );
    // Remote network and artwork work is bounded, while publication stays in
    // one coordinator below. This provides the measured network overlap
    // without multiplying database writers or making authoritative progress
    // events race each other. The second bounded channel keeps polling remote
    // futures while the coordinator awaits a database commit; without it,
    // buffer_unordered would pause the other in-flight HTTP requests whenever
    // one completed group was being committed.
    let (prepared_sender, mut prepared_receiver) = mpsc::channel(REMOTE_ENRICHMENT_QUEUE_CAPACITY);
    let preparation_cancellation = cancellation_flag.clone();
    let preparation_event_listener = event_listener.clone();
    let preparation_artwork_cache_dir = artwork_cache_dir.clone();
    let preparation = async move {
        let result = stream::unfold(group_receiver, |mut receiver| async move {
            let next = receiver.recv().await;
            next.map(|group| (group, receiver))
        })
        .map(|queued_group| {
            let enrichment = enrichment.clone();
            let cancellation_flag = preparation_cancellation.clone();
            let event_listener = preparation_event_listener.clone();
            let metadata_provider = metadata_provider.clone();
            let artwork_cache_dir = preparation_artwork_cache_dir.clone();
            async move {
                prepare_remote_scan_group(
                    library,
                    scan_job_id,
                    queued_group,
                    total_items,
                    cancellation_flag,
                    enrichment,
                    metadata_provider,
                    artwork_cache_dir,
                    pool,
                    event_listener,
                )
                .await
            }
        })
        .buffer_unordered(REMOTE_ENRICHMENT_CONCURRENCY)
        .try_for_each(|prepared_group| {
            let prepared_sender = prepared_sender.clone();
            let cancellation_flag = preparation_cancellation.clone();
            async move {
                match prepared_sender.send(prepared_group).await {
                    Ok(()) => Ok(()),
                    // Cancellation is an expected terminal state. The commit
                    // coordinator closes its receiver first, so preparation
                    // must not convert that orderly shutdown into a failed
                    // scan merely because a completed worker can no longer
                    // enqueue its result.
                    Err(_) if is_cancelled(&cancellation_flag) => Ok(()),
                    Err(_) => Err(ApplicationError::Unexpected(anyhow::anyhow!(
                        "remote scan commit coordinator stopped before preparation completed"
                    ))),
                }
            }
        })
        .await;
        drop(prepared_sender);
        result
    };

    let commit_cancellation = cancellation_flag.clone();
    let commit_event_listener = event_listener.clone();
    let commit_artwork_cache_dir = artwork_cache_dir.clone();
    let commit = async move {
        let mut sync_outcome = mova_db::SyncLibraryMediaBestEffortOutcome::default();
        let mut notification_summary = ScanNotificationSummary::default();
        let mut processed_groups = 0_usize;

        while let Some(prepared_group) = prepared_receiver.recv().await {
            if is_cancelled(&commit_cancellation) {
                break;
            }

            let PreparedRemoteScanGroup {
                group,
                item_index,
                total_items,
                artwork_publication,
                enrichment_outcome,
                failure_detail,
                materialized_artwork,
                allow_artwork_clear,
                replace_remote_data,
                retain_materialized_artwork,
            } = prepared_group;
            if group.files.is_empty() {
                continue;
            }
            processed_groups = processed_groups.saturating_add(1);
            let group_result = sync_scan_group_media_entries(
                pool,
                scan_job_id,
                library,
                &group,
                mova_db::ScanGroupCommitStage::Remote,
                allow_artwork_clear,
                replace_remote_data,
                enrichment_outcome
                    .as_ref()
                    .and_then(|outcome| outcome.tmdb_remote_snapshot_json.as_deref()),
                enrichment_outcome
                    .as_ref()
                    .is_some_and(|outcome| outcome.tmdb_remote_snapshot_renews_retention),
                fence,
            )
            .await;
            let group_outcome = match artwork_publication {
                Some(artwork_publication) => {
                    crate::tmdb_revalidation::finish_tmdb_artwork_publication(
                        artwork_publication,
                        group_result,
                        pool,
                        &commit_artwork_cache_dir,
                        library.id,
                        materialized_artwork,
                        retain_materialized_artwork,
                    )
                    .await?
                }
                None => group_result?,
            };
            merge_sync_outcome(&mut sync_outcome, group_outcome.sync);
            record_scan_notification_group(
                &mut notification_summary,
                &group,
                failure_detail.as_deref(),
            );
            commit_event_listener(ScanJobEvent::ItemUpdated(build_scan_group_progress_update(
                scan_job_id,
                library.id,
                &group.presentation,
                group.files.first(),
                item_index,
                total_items,
                ScanItemStage::Completed,
            )));
            commit_event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                group_outcome.scan_job,
                SCAN_PHASE_PROCESSING,
            )));
        }

        Ok::<_, ApplicationError>(RemoteScanPipelineOutcome {
            sync: sync_outcome,
            notification_summary,
            elapsed_ms: elapsed_millis(started_at),
            processed_groups,
        })
    };

    let (_, outcome) = tokio::try_join!(preparation, commit)?;
    Ok(outcome)
}

#[allow(clippy::too_many_arguments)]
async fn prepare_remote_scan_group(
    library: &Library,
    scan_job_id: i64,
    queued_group: QueuedScanGroup,
    total_items: i32,
    cancellation_flag: Arc<AtomicBool>,
    mut enrichment: MetadataEnrichmentContext,
    metadata_provider: Arc<dyn MetadataProvider>,
    artwork_cache_dir: PathBuf,
    pool: &PgPool,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> ApplicationResult<PreparedRemoteScanGroup> {
    let QueuedScanGroup {
        mut group,
        item_index,
        total_items: queued_total_items,
    } = queued_group;
    let total_items = total_items.max(queued_total_items).max(item_index);
    if group.files.is_empty() || is_cancelled(&cancellation_flag) {
        return Ok(PreparedRemoteScanGroup {
            group,
            item_index,
            total_items,
            artwork_publication: None,
            enrichment_outcome: None,
            failure_detail: None,
            materialized_artwork: std::collections::BTreeSet::new(),
            allow_artwork_clear: false,
            replace_remote_data: false,
            retain_materialized_artwork: false,
        });
    }

    let metadata_decision = resolve_group_metadata_lookup_type(metadata_provider.as_ref(), &group);
    let Some(lookup_type) = metadata_decision.lookup_type else {
        for file in &mut group.files {
            clear_remote_metadata_for_review(
                file,
                metadata_decision.metadata_status,
                metadata_decision.metadata_failure_reason,
                metadata_decision.remote_media_type,
            );
        }
        crate::local_metadata::apply_group_local_metadata(
            &mut group.files,
            scan_presentation_lookup_type(&group.presentation),
        );
        return Ok(PreparedRemoteScanGroup {
            group,
            item_index,
            total_items,
            artwork_publication: None,
            enrichment_outcome: None,
            failure_detail: None,
            materialized_artwork: std::collections::BTreeSet::new(),
            allow_artwork_clear: true,
            replace_remote_data: true,
            retain_materialized_artwork: true,
        });
    };

    // Remote enrichment can update several fields before a later provider
    // request fails. Keep the locally committed state so a transient provider
    // failure cannot partially replace trusted metadata or NFO values.
    let mut files_before_enrichment = group.files.clone();
    for file in &mut files_before_enrichment {
        enrichment.sanitize_file_artwork_sources(file);
    }
    // The guard starts before any artwork may be materialized and travels
    // with the prepared result until the single coordinator has committed all
    // references. It uses a standalone connection, so four concurrent remote
    // preparations cannot exhaust the application database pool.
    let artwork_publication = mova_db::TmdbArtworkPublicationGuard::acquire(pool, library.id)
        .await
        .map_err(ApplicationError::from)?;
    let mut presentation = group.presentation.clone();
    let enrichment_progress_listener = event_listener.clone();
    let enrichment_result = enrichment
        .enrich_group_with_lookup_hint_and_progress(
            lookup_type,
            &mut group.files,
            group.presentation.season_air_year,
            group.metadata_lookup_hint.as_deref(),
            move |stage, file| {
                if stage != MetadataEnrichmentStage::Metadata && !file.title.trim().is_empty() {
                    presentation.title = file.title.clone();
                }
                if stage != MetadataEnrichmentStage::Completed {
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
                }
            },
        )
        .await;
    let materialized_artwork = crate::tmdb_revalidation::materialized_tmdb_artwork_paths_from_files(
        &group.files,
        &artwork_cache_dir,
        library.id,
    );

    let mut failure_detail = None;
    let enrichment_outcome = match enrichment_result {
        Ok(outcome) => {
            for file in &mut group.files {
                finalize_file_metadata_status(
                    file,
                    metadata_provider.is_enabled(),
                    remote_media_type_for_lookup_type(lookup_type),
                );
            }
            crate::local_metadata::apply_group_local_metadata(&mut group.files, lookup_type);
            Some(outcome)
        }
        Err(error) => {
            let detail = compact_scan_failure_detail(error.root_cause().to_string());
            tracing::warn!(
                library_id = library.id,
                scan_job_id,
                title = %group.presentation.lookup_title,
                year = group.presentation.year,
                media_type = %group.presentation.media_type,
                error = ?error,
                "metadata enrichment failed for scan group"
            );
            restore_group_after_provider_error(
                &mut group.files,
                files_before_enrichment,
                metadata_decision.remote_media_type,
            );
            failure_detail = Some(detail);
            None
        }
    };
    if let Some(primary_file) = group.files.first() {
        if !primary_file.title.trim().is_empty() {
            group.presentation.title = primary_file.title.clone();
        }
    }

    Ok(PreparedRemoteScanGroup {
        group,
        item_index,
        total_items,
        artwork_publication: Some(artwork_publication),
        allow_artwork_clear: enrichment_outcome
            .as_ref()
            .is_some_and(|outcome| outcome.remote_metadata_applied),
        replace_remote_data: enrichment_outcome
            .as_ref()
            .is_some_and(|outcome| outcome.remote_metadata_applied),
        retain_materialized_artwork: failure_detail.is_none(),
        enrichment_outcome,
        failure_detail,
        materialized_artwork,
    })
}

fn elapsed_millis(started_at: Instant) -> u64 {
    u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[allow(clippy::too_many_arguments)]
async fn sync_scan_group_media_entries(
    pool: &PgPool,
    scan_job_id: i64,
    library: &Library,
    group: &ScanDiscoveredGroup,
    stage: mova_db::ScanGroupCommitStage,
    allow_artwork_clear: bool,
    replace_remote_data: bool,
    tmdb_remote_snapshot_json: Option<&str>,
    tmdb_remote_snapshot_renews_retention: bool,
    fence: &BackgroundJobFence,
) -> ApplicationResult<ScanGroupSyncOutcome> {
    let entries = build_media_entries(
        library,
        group.files.clone(),
        allow_artwork_clear,
        replace_remote_data,
        tmdb_remote_snapshot_json,
        tmdb_remote_snapshot_renews_retention,
    )?;
    let commit_outcome = match stage {
        mova_db::ScanGroupCommitStage::Local => {
            mova_db::upsert_library_media_entries_by_file_path_with_progress(
                pool,
                scan_job_id,
                library.id,
                &group.presentation.item_key,
                stage,
                &entries,
                fence,
            )
            .await
        }
        mova_db::ScanGroupCommitStage::Remote => {
            mova_db::patch_library_media_entries_remote_by_file_path_with_progress(
                pool,
                scan_job_id,
                library.id,
                &group.presentation.item_key,
                &entries,
                fence,
            )
            .await
        }
    }
    .map_err(ApplicationError::Unexpected)?;

    Ok(ScanGroupSyncOutcome {
        sync: mova_db::SyncLibraryMediaBestEffortOutcome {
            upserted_count: commit_outcome.upserted_count,
            ..Default::default()
        },
        scan_job: commit_outcome.scan_job,
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

    let file_count = i32::try_from(group.files.len()).unwrap_or(i32::MAX);
    let mut has_matched = false;
    let mut has_unmatched = false;
    let mut has_skipped = false;
    let mut has_failed = group.files.is_empty();
    for file in &group.files {
        match file.metadata_status.as_deref() {
            Some(METADATA_STATUS_MATCHED) => {
                has_matched = true;
                summary.matched_files = summary.matched_files.saturating_add(1);
            }
            Some(METADATA_STATUS_UNMATCHED) => {
                has_unmatched = true;
                summary.unmatched_files = summary.unmatched_files.saturating_add(1);
            }
            Some(METADATA_STATUS_SKIPPED) => {
                has_skipped = true;
                summary.skipped_files = summary.skipped_files.saturating_add(1);
            }
            _ => {
                has_failed = true;
                summary.failed_files = summary.failed_files.saturating_add(1);
            }
        }
    }
    let metadata_status = if has_failed {
        METADATA_STATUS_FAILED
    } else if has_unmatched {
        METADATA_STATUS_UNMATCHED
    } else if has_skipped {
        METADATA_STATUS_SKIPPED
    } else if has_matched {
        METADATA_STATUS_MATCHED
    } else {
        METADATA_STATUS_FAILED
    }
    .to_string();

    let probe_warning_count = i32::try_from(probe_warnings.len()).unwrap_or(i32::MAX);
    summary.probe_warning_count = summary
        .probe_warning_count
        .saturating_add(probe_warning_count);
    let has_provider_error = failure_detail.is_some();
    let has_issue = has_provider_error
        || matches!(
            metadata_status.as_str(),
            METADATA_STATUS_UNMATCHED | METADATA_STATUS_FAILED
        )
        || probe_warning_count > 0;
    if !has_issue {
        return;
    }

    summary.issue_count = summary.issue_count.saturating_add(1);
    if summary.issues.len() >= MAX_SCAN_NOTIFICATION_ISSUES {
        return;
    }

    let metadata_failure_reason =
        primary_file.and_then(|file| file.metadata_failure_reason.clone());
    let reason_code = if has_provider_error {
        METADATA_FAILURE_PROVIDER_ERROR.to_string()
    } else {
        metadata_failure_reason.unwrap_or_else(|| {
            if metadata_status == METADATA_STATUS_UNMATCHED {
                METADATA_FAILURE_NO_REMOTE_MATCH.to_string()
            } else if metadata_status == METADATA_STATUS_FAILED {
                "metadata_processing_failed".to_string()
            } else {
                "media_probe_warning".to_string()
            }
        })
    };
    let probe_warning_params = if probe_warning_count > 0 {
        BTreeMap::from([(
            "count".to_string(),
            serde_json::Value::from(probe_warning_count),
        )])
    } else {
        BTreeMap::new()
    };

    summary.issues.push(ScanNotificationIssue {
        item_key: group.presentation.item_key.clone(),
        media_type: group.presentation.media_type.clone(),
        title: group.presentation.title.clone(),
        year: group.presentation.year,
        file_count,
        metadata_status,
        reason_code,
        reason_params: BTreeMap::new(),
        diagnostic_message: failure_detail.map(compact_scan_failure_detail),
        probe_warning_count,
        probe_warning_file_path: first_probe_warning
            .as_ref()
            .map(|(file_path, _)| file_path.clone()),
        probe_warning_code: (probe_warning_count > 0).then(|| "media_probe_warning".to_string()),
        probe_warning_params,
        probe_warning_diagnostic: first_probe_warning.map(|(_, detail)| detail),
    });
}

fn record_media_discovery_issues(
    summary: &mut ScanNotificationSummary,
    issues: &[MediaDiscoveryIssue],
) {
    for issue in issues {
        summary.failed_files = summary.failed_files.saturating_add(1);
        summary.issue_count = summary.issue_count.saturating_add(1);
        if summary.issues.len() >= MAX_SCAN_NOTIFICATION_ISSUES {
            continue;
        }

        let file_path = issue.file_path.to_string_lossy().to_string();
        let title = issue
            .file_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "STRM".to_string());
        summary.issues.push(ScanNotificationIssue {
            item_key: format!("discovery:{file_path}"),
            media_type: "strm".to_string(),
            title,
            year: None,
            file_count: 1,
            metadata_status: METADATA_STATUS_FAILED.to_string(),
            reason_code: issue.reason_code.clone(),
            reason_params: BTreeMap::from([(
                "file_path".to_string(),
                serde_json::Value::String(file_path),
            )]),
            // Keep notification diagnostics derived exclusively from the
            // stable reason code. The discovery layer already redacts its
            // text, but this boundary must never trust carrier content.
            diagnostic_message: Some(format!("STRM discovery issue: {}", issue.reason_code)),
            probe_warning_count: 0,
            probe_warning_file_path: None,
            probe_warning_code: None,
            probe_warning_params: BTreeMap::new(),
            probe_warning_diagnostic: None,
        });
    }
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

    if group.metadata_binding_conflict {
        tracing::warn!(
            item_key = %presentation.item_key,
            title = %presentation.lookup_title,
            media_type = %presentation.media_type,
            "metadata lookup skipped because the scan container has conflicting TMDB bindings"
        );
        return GroupMetadataLookupDecision {
            lookup_type: None,
            metadata_status: METADATA_STATUS_UNMATCHED,
            metadata_failure_reason: Some(METADATA_FAILURE_NO_REMOTE_MATCH),
            remote_media_type: remote_media_type_for_lookup_type(local_lookup_type),
        };
    }

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
        if file.metadata_provider_item_id.is_some() {
            // A temporarily missing provider configuration must not demote a
            // previously matched item or discard its remote identity. Keep a
            // previous transient error marker so the refresh is retried once
            // the provider becomes available again.
            file.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
            return;
        }

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

pub(crate) fn restore_group_after_provider_error(
    files: &mut Vec<DiscoveredMediaFile>,
    files_before_enrichment: Vec<DiscoveredMediaFile>,
    remote_media_type: Option<&str>,
) {
    *files = files_before_enrichment;

    if let Some(authoritative_index) = files.iter().position(has_accepted_remote_binding) {
        let accepted_bindings = files
            .iter()
            .map(has_accepted_remote_binding)
            .collect::<Vec<_>>();
        let provider = files[authoritative_index].metadata_provider.clone();
        let provider_item_id = files[authoritative_index].metadata_provider_item_id.clone();
        let accepted_remote_media_type = files[authoritative_index]
            .remote_media_type
            .clone()
            .or_else(|| remote_media_type.map(str::to_string));

        for (file, had_accepted_binding) in files.iter_mut().zip(accepted_bindings) {
            // Every file in one scan group targets the same movie or series
            // parent. Propagating the accepted identity keeps a newly added
            // version/episode from demoting that shared parent when the
            // provider fails, while retaining each file's own local fields.
            // The accepted identity does not prove that a newly discovered
            // episode was enriched successfully, so only files that already
            // carried that binding remain matched.
            file.metadata_provider.clone_from(&provider);
            file.metadata_provider_item_id.clone_from(&provider_item_id);
            file.metadata_status = Some(
                if had_accepted_binding {
                    METADATA_STATUS_MATCHED
                } else {
                    METADATA_STATUS_FAILED
                }
                .to_string(),
            );
            file.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
            file.remote_media_type
                .clone_from(&accepted_remote_media_type);
        }

        // Group persistence updates the shared movie/series parent once per
        // file. Commit the authoritative snapshot last so local fields from a
        // newly added file cannot become the final parent presentation.
        if authoritative_index + 1 != files.len() {
            let authoritative_file = files.remove(authoritative_index);
            files.push(authoritative_file);
        }
        return;
    }

    for file in files {
        file.metadata_status = Some(METADATA_STATUS_FAILED.to_string());
        file.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
        if file.remote_media_type.is_none() {
            file.remote_media_type = remote_media_type.map(str::to_string);
        }
    }
}

fn has_accepted_remote_binding(file: &DiscoveredMediaFile) -> bool {
    file.metadata_provider
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && file
            .metadata_provider_item_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
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

fn build_media_entries(
    library: &Library,
    discovered_files: Vec<DiscoveredMediaFile>,
    allow_artwork_clear: bool,
    replace_remote_data: bool,
    tmdb_remote_snapshot_json: Option<&str>,
    tmdb_remote_snapshot_renews_retention: bool,
) -> ApplicationResult<Vec<mova_db::CreateMediaEntryParams>> {
    let discovered_files = normalize_discovered_files_for_local_structure(discovered_files);
    let mut entries = Vec::new();
    let mut remote_data_pending = replace_remote_data;

    for file in discovered_files {
        let media_type = effective_media_type(&file).to_string();
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
        let metadata_status = file.metadata_status.clone().ok_or_else(|| {
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
        let tmdb_remote_snapshot_json = replace_remote_data
            .then(|| tmdb_remote_snapshot_json.map(str::to_owned))
            .flatten();
        let tmdb_remote_snapshot_renews_retention =
            replace_remote_data && tmdb_remote_snapshot_renews_retention;
        let premiere_date = crate::local_metadata::parse_nfo_date(file.premiere_date.as_deref());
        let episode_premiere_date =
            crate::local_metadata::parse_nfo_date(file.episode_premiere_date.as_deref());
        let local_nfo = file.local_nfo.as_ref().and_then(|metadata| {
            crate::local_metadata::build_local_metadata_snapshot_for_file(
                metadata,
                file.local_nfo_is_selected,
                &file,
            )
        });
        let series_local_nfo = file.series_local_nfo.as_ref().and_then(|metadata| {
            crate::local_metadata::build_local_metadata_snapshot_for_file(
                metadata,
                file.series_local_nfo_is_selected,
                &file,
            )
        });

        entries.push(mova_db::CreateMediaEntryParams {
            library_id: library.id,
            media_type,
            metadata_provider: file.metadata_provider,
            metadata_provider_item_id: file.metadata_provider_item_id,
            metadata_status,
            metadata_failure_reason: file.metadata_failure_reason,
            allow_artwork_clear: entry_allow_artwork_clear,
            replace_remote_data,
            tmdb_remote_snapshot_json,
            tmdb_remote_snapshot_renews_retention,
            remote_media_type: file.remote_media_type,
            title: file.title,
            source_title: file.source_title,
            original_title: file.original_title,
            sort_title: file.sort_title,
            year: file.year,
            tagline: file.tagline,
            premiere_date,
            content_rating: file.content_rating,
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
            episode_original_title: file.episode_original_title,
            episode_sort_title: file.episode_sort_title,
            episode_year: file.episode_year,
            episode_overview: file.episode_overview,
            episode_tagline: file.episode_tagline,
            episode_premiere_date,
            episode_content_rating: file.episode_content_rating,
            overview: file.overview,
            series_poster_path: file.series_poster_path,
            series_backdrop_path: file.series_backdrop_path,
            series_logo_path: file.series_logo_path,
            local_nfo,
            series_local_nfo,
            removed_local_nfo_source_path: file.removed_local_nfo_source_path,
            removed_series_local_nfo_source_path: file.removed_series_local_nfo_source_path,
            poster_path: file.poster_path,
            backdrop_path: file.backdrop_path,
            logo_path: file.logo_path,
            file_path,
            source_kind: file.source_kind,
            stream_reference_hash: file.stream_reference_hash,
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

async fn emit_scan_job_phase(
    pool: &PgPool,
    scan_job_id: i64,
    phase: &str,
    progress_percent: i32,
    fence: &BackgroundJobFence,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> ApplicationResult<()> {
    match mova_db::update_scan_job_phase(pool, scan_job_id, phase, progress_percent, fence).await {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                scan_job, phase,
            )));
        }
        Ok(None) => {}
        Err(error) => return Err(ApplicationError::Unexpected(error)),
    }

    Ok(())
}

async fn record_failed_scan_attempt(
    pool: &PgPool,
    scan_job_id: i64,
    total_files: i32,
    scanned_files: i32,
    error_message: &str,
    fence: &BackgroundJobFence,
) {
    if let Err(error) = mova_db::record_scan_job_attempt_failure(
        pool,
        scan_job_id,
        total_files,
        scanned_files,
        error_message,
        fence,
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
    library_id: i64,
    scan_job_id: i64,
    total_files: i32,
    scanned_files: i32,
    fence: &BackgroundJobFence,
) -> ApplicationResult<Option<ScanJob>> {
    mova_db::finalize_scan_job(
        pool,
        library_id,
        scan_job_id,
        "cancelled",
        total_files,
        scanned_files,
        Some("scan cancelled"),
        None,
        fence,
    )
    .await
    .map_err(ApplicationError::from)
}

#[cfg(test)]
#[path = "scan_jobs/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "scan_jobs/performance_tests.rs"]
mod performance_tests;
