use crate::{
    error::{ApplicationError, ApplicationResult},
    libraries::get_library,
    media_classification::{classify_media_type, metadata_lookup_type_for_media_type},
    media_enrichment::MetadataEnrichmentContext,
    media_enrichment::MetadataEnrichmentStage,
    metadata::MetadataProvider,
};
use mova_domain::{Library, ScanJob};
use mova_scan::{infer_series_folder_metadata, DiscoveredMediaFile};
use sqlx::postgres::PgPool;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicI32, Ordering},
        Arc,
    },
};

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
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub item_index: i32,
    pub total_items: i32,
    pub stage: String,
    pub progress_percent: i32,
}

#[derive(Debug, Clone, Copy)]
enum ScanItemStage {
    Discovered,
    Metadata,
    Artwork,
    Completed,
}

#[derive(Debug)]
enum DiscoverMediaFilesOutcome {
    Completed(Vec<DiscoveredMediaFile>),
    Cancelled(i32),
}

#[derive(Debug, Clone)]
struct ScanPresentationGroup {
    item_key: String,
    media_type: String,
    title: String,
    lookup_title: String,
    year: Option<i32>,
}

#[derive(Debug)]
struct ScanDiscoveredGroup {
    presentation: ScanPresentationGroup,
    files: Vec<DiscoveredMediaFile>,
}

const SCAN_PHASE_DISCOVERING: &str = "discovering";
const SCAN_PHASE_ENRICHING: &str = "enriching";
const SCAN_PHASE_SYNCING: &str = "syncing";
const SCAN_PHASE_FINISHED: &str = "finished";

const SCAN_ITEM_STAGE_DISCOVERED: &str = "discovered";
const SCAN_ITEM_STAGE_METADATA: &str = "metadata";
const SCAN_ITEM_STAGE_ARTWORK: &str = "artwork";
const SCAN_ITEM_STAGE_COMPLETED: &str = "completed";

const SCAN_PHASE_INITIALIZING: &str = "initializing";

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

    match execute_scan_job_with_cancellation(
        pool,
        library_id,
        scan_job_id,
        cancellation_flag,
        artwork_cache_dir,
        metadata_provider,
        event_listener,
    )
    .await?
    {
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
            if let Some(scan_job) = finalize_failed_scan(
                pool,
                scan_job_id,
                0,
                0,
                &format_scan_phase_error(
                    SCAN_PHASE_INITIALIZING,
                    format!("Failed to load library configuration: {}", error),
                ),
            )
            .await
            {
                event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                    scan_job,
                    SCAN_PHASE_FINISHED,
                )));
            }
            return Err(error);
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
            if let Some(scan_job) = finalize_failed_scan(
                pool,
                scan_job_id,
                0,
                0,
                &format_scan_phase_error(
                    SCAN_PHASE_INITIALIZING,
                    format!("Failed to start the scan job: {}", error),
                ),
            )
            .await
            {
                event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                    scan_job,
                    SCAN_PHASE_FINISHED,
                )));
            }
            return Err(error);
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
            if let Some(scan_job) = finalize_failed_scan(
                pool,
                scan_job_id,
                0,
                0,
                &format_scan_phase_error(
                    SCAN_PHASE_DISCOVERING,
                    format!("Failed to scan library files: {}", error),
                ),
            )
            .await
            {
                event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                    scan_job,
                    SCAN_PHASE_FINISHED,
                )));
            }
            return Err(error);
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

    emit_scan_job_phase(
        pool,
        scan_job_id,
        SCAN_PHASE_ENRICHING,
        event_listener.clone(),
    )
    .await;

    let discovered_files = enrich_discovered_files(
        &library,
        scan_job_id,
        discovered_files,
        cancellation_flag.clone(),
        artwork_cache_dir,
        metadata_provider,
        event_listener.clone(),
    )
    .await;

    let media_entries = match build_media_entries(&library, discovered_files) {
        Ok(entries) => entries,
        Err(error) => {
            if let Some(scan_job) = finalize_failed_scan(
                pool,
                scan_job_id,
                total_files,
                0,
                &format_scan_phase_error(
                    SCAN_PHASE_ENRICHING,
                    format!("Failed to build media entries: {}", error),
                ),
            )
            .await
            {
                event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                    scan_job,
                    SCAN_PHASE_FINISHED,
                )));
            }
            return Err(error);
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
        SCAN_PHASE_SYNCING,
        event_listener.clone(),
    )
    .await;

    if let Err(error) = mova_db::sync_library_media(pool, library.id, &media_entries).await {
        if let Some(scan_job) = finalize_failed_scan(
            pool,
            scan_job_id,
            total_files,
            0,
            &format_scan_phase_error(
                SCAN_PHASE_SYNCING,
                format!("Failed to save library data: {}", error),
            ),
        )
        .await
        {
            event_listener(ScanJobEvent::Finished(build_scan_job_progress_update(
                scan_job,
                SCAN_PHASE_FINISHED,
            )));
        }
        return Err(ApplicationError::Unexpected(error));
    }

    match mova_db::finalize_scan_job(pool, scan_job_id, "success", total_files, total_files, None)
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

async fn enrich_discovered_files(
    library: &Library,
    scan_job_id: i64,
    discovered_files: Vec<DiscoveredMediaFile>,
    cancellation_flag: Arc<AtomicBool>,
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> Vec<DiscoveredMediaFile> {
    let mut enrichment = MetadataEnrichmentContext::new(
        artwork_cache_dir,
        metadata_provider,
        library.metadata_language.clone(),
    );
    let mut groups = group_discovered_files_for_scan(library, discovered_files);
    let total_items = i32::try_from(groups.len()).unwrap_or(i32::MAX);

    for (index, group) in groups.iter_mut().enumerate() {
        if is_cancelled(&cancellation_flag) {
            break;
        }

        if group.presentation.media_type.eq_ignore_ascii_case("series") {
            for file in &mut group.files {
                file.source_title = group.presentation.lookup_title.clone();

                if file.year.is_none() {
                    file.year = group.presentation.year;
                }
            }
        }

        let item_index = i32::try_from(index + 1).unwrap_or(i32::MAX);
        let Some((primary_file, remaining_files)) = group.files.split_first_mut() else {
            continue;
        };

        let lookup_type = metadata_lookup_type_for_media_type(&group.presentation.media_type);
        let progress_listener = event_listener.clone();
        let mut presentation = group.presentation.clone();

        enrichment
            .enrich_file_with_progress(lookup_type, primary_file, move |stage, file| {
                if stage != MetadataEnrichmentStage::Metadata {
                    if !file.title.trim().is_empty() {
                        presentation.title = file.title.clone();
                    }
                }

                progress_listener(ScanJobEvent::ItemUpdated(build_scan_group_progress_update(
                    scan_job_id,
                    library.id,
                    &presentation,
                    item_index,
                    total_items,
                    stage.into(),
                )));
            })
            .await;

        if !primary_file.title.trim().is_empty() {
            group.presentation.title = primary_file.title.clone();
        }

        for file in remaining_files.iter_mut() {
            enrichment.enrich_file(lookup_type, file).await;
        }
    }

    groups.into_iter().flat_map(|group| group.files).collect()
}

async fn discover_media_files(
    pool: &PgPool,
    scan_job_id: i64,
    library: &Library,
    cancellation_flag: Arc<AtomicBool>,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> ApplicationResult<DiscoverMediaFilesOutcome> {
    let library_id = library.id;
    let root_path = library.root_path.as_str();
    let root_path_string = root_path.to_string();
    let root_path_for_task = root_path_string.clone();
    let library_type_for_task = library.library_type.clone();
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<i32>();
    let progress_pool = pool.clone();
    let last_progress = Arc::new(AtomicI32::new(0));
    let last_progress_for_task = last_progress.clone();
    let progress_event_listener = event_listener.clone();
    let item_event_listener = event_listener.clone();

    let progress_task = tokio::spawn(async move {
        let mut persisted_progress = 0;

        while let Some(scanned_files) = progress_rx.recv().await {
            if scanned_files <= persisted_progress {
                continue;
            }

            match mova_db::update_scan_job_progress(
                &progress_pool,
                scan_job_id,
                None,
                scanned_files,
            )
            .await
            {
                Ok(Some(scan_job)) => {
                    progress_event_listener(ScanJobEvent::Updated(build_scan_job_progress_update(
                        scan_job,
                        SCAN_PHASE_DISCOVERING,
                    )));
                }
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(
                        scan_job_id,
                        scanned_files,
                        error = ?error,
                        "failed to update scan progress"
                    );
                    continue;
                }
            }

            persisted_progress = scanned_files;
            last_progress_for_task.store(scanned_files, Ordering::SeqCst);
        }
    });

    let cancellation_for_task = cancellation_flag.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut discovered_group_count = 0_i32;
        let mut discovered_groups = HashMap::<String, i32>::new();

        mova_scan::discover_media_files_with_progress_item_and_cancel(
            std::path::Path::new(&root_path_for_task),
            |count| {
                let _ = progress_tx.send(count as i32);
            },
            |file| {
                let presentation = build_scan_presentation_group(&library_type_for_task, file);

                if discovered_groups.contains_key(&presentation.item_key) {
                    return;
                }

                discovered_group_count = discovered_group_count.saturating_add(1);
                discovered_groups.insert(presentation.item_key.clone(), discovered_group_count);

                item_event_listener(ScanJobEvent::ItemUpdated(build_scan_group_progress_update(
                    scan_job_id,
                    library_id,
                    &presentation,
                    discovered_group_count,
                    discovered_group_count,
                    ScanItemStage::Discovered,
                )));
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
) -> ApplicationResult<Vec<mova_db::CreateMediaEntryParams>> {
    let mut entries = Vec::new();

    for file in discovered_files {
        let media_type = classify_media_type(&library.library_type, &file.file_path).to_string();
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

        entries.push(mova_db::CreateMediaEntryParams {
            library_id: library.id,
            media_type,
            title: file.title,
            source_title: file.source_title,
            original_title: file.original_title,
            sort_title: file.sort_title,
            year: file.year,
            imdb_rating: file.imdb_rating,
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
            poster_path: file.poster_path,
            backdrop_path: file.backdrop_path,
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
        });
    }

    Ok(entries)
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
        SCAN_PHASE_ENRICHING => "Metadata enrichment failed",
        SCAN_PHASE_SYNCING => "Library write failed",
        SCAN_PHASE_FINISHED => "Finalization failed",
        _ => "Scan job failed",
    }
}

fn build_scan_presentation_group(
    library_type: &str,
    file: &DiscoveredMediaFile,
) -> ScanPresentationGroup {
    let media_type = classify_media_type(library_type, &file.file_path);

    if media_type == "episode" {
        if let Some(folder_metadata) = infer_series_folder_metadata(&file.file_path) {
            return ScanPresentationGroup {
                item_key: folder_metadata.folder_path.to_string_lossy().to_string(),
                media_type: "series".to_string(),
                title: folder_metadata.display_title,
                lookup_title: folder_metadata.title,
                year: folder_metadata.year.or(file.year),
            };
        }

        return ScanPresentationGroup {
            item_key: file.file_path.to_string_lossy().to_string(),
            media_type: "series".to_string(),
            title: file.source_title.clone(),
            lookup_title: file.source_title.clone(),
            year: file.year,
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
    }
}

fn group_discovered_files_for_scan(
    library: &Library,
    discovered_files: Vec<DiscoveredMediaFile>,
) -> Vec<ScanDiscoveredGroup> {
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
    item_index: i32,
    total_items: i32,
    stage: ScanItemStage,
) -> ScanJobItemProgressUpdate {
    let (stage_name, progress_percent) = match stage {
        ScanItemStage::Discovered => (SCAN_ITEM_STAGE_DISCOVERED, 6),
        ScanItemStage::Metadata => (SCAN_ITEM_STAGE_METADATA, 36),
        ScanItemStage::Artwork => (SCAN_ITEM_STAGE_ARTWORK, 76),
        ScanItemStage::Completed => (SCAN_ITEM_STAGE_COMPLETED, 100),
    };
    ScanJobItemProgressUpdate {
        scan_job_id,
        library_id,
        item_key: presentation.item_key.clone(),
        media_type: presentation.media_type.clone(),
        title: presentation.title.clone(),
        season_number: None,
        episode_number: None,
        item_index,
        total_items,
        stage: stage_name.to_string(),
        progress_percent,
    }
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
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) {
    match mova_db::get_scan_job(pool, scan_job_id).await {
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

#[cfg(test)]
mod tests {
    use crate::media_classification::{LIBRARY_TYPE_MIXED, LIBRARY_TYPE_SERIES};
    use mova_domain::Library;
    use mova_scan::DiscoveredMediaFile;
    use std::path::{Path, PathBuf};
    use time::OffsetDateTime;

    fn build_discovered_file() -> DiscoveredMediaFile {
        DiscoveredMediaFile {
            file_path: PathBuf::from("/media/series/Arcane/Arcane.S01E01.mkv"),
            title: "Arcane".to_string(),
            source_title: "Arcane.S01E01".to_string(),
            original_title: None,
            sort_title: None,
            year: Some(2021),
            imdb_rating: None,
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
            poster_path: None,
            backdrop_path: None,
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
            audio_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
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
            is_enabled: true,
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
        }
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
    fn series_library_still_forces_episode_media_type() {
        assert_eq!(
            super::classify_media_type(LIBRARY_TYPE_SERIES, Path::new("Movie.2025.mkv")),
            "episode"
        );
    }

    #[test]
    fn scan_phase_label_returns_user_facing_stage_name() {
        assert_eq!(
            super::scan_phase_label(super::SCAN_PHASE_DISCOVERING),
            "Directory scan failed"
        );
        assert_eq!(
            super::scan_phase_label(super::SCAN_PHASE_ENRICHING),
            "Metadata enrichment failed"
        );
        assert_eq!(
            super::scan_phase_label(super::SCAN_PHASE_SYNCING),
            "Library write failed"
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
    fn build_scan_item_progress_update_emits_group_level_series_payload() {
        let presentation =
            super::build_scan_presentation_group(LIBRARY_TYPE_SERIES, &build_discovered_file());
        let progress = super::build_scan_group_progress_update(
            41,
            7,
            &presentation,
            1,
            3,
            super::ScanItemStage::Discovered,
        );

        assert_eq!(progress.scan_job_id, 41);
        assert_eq!(progress.library_id, 7);
        assert_eq!(progress.media_type, "series");
        assert_eq!(progress.title, "Arcane");
        assert_eq!(progress.season_number, None);
        assert_eq!(progress.episode_number, None);
        assert_eq!(progress.stage, "discovered");
        assert_eq!(progress.progress_percent, 6);
        assert_eq!(progress.item_index, 1);
        assert_eq!(progress.total_items, 3);
        assert!(progress.item_key.ends_with("Arcane"));
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
        assert!(groups[0].presentation.item_key.ends_with("Arcane"));
    }
}

async fn finalize_failed_scan(
    pool: &PgPool,
    scan_job_id: i64,
    total_files: i32,
    scanned_files: i32,
    error_message: &str,
) -> Option<ScanJob> {
    mova_db::finalize_scan_job(
        pool,
        scan_job_id,
        "failed",
        total_files,
        scanned_files,
        Some(error_message),
    )
    .await
    .ok()
    .flatten()
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
    )
    .await
    .ok()
    .flatten()
}
