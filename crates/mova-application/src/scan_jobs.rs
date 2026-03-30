use crate::{
    error::{ApplicationError, ApplicationResult},
    libraries::get_library,
    media_classification::{classify_media_type, metadata_lookup_type_for_media_type},
    media_enrichment::MetadataEnrichmentContext,
    metadata::MetadataProvider,
};
use mova_domain::{Library, ScanJob};
use mova_scan::DiscoveredMediaFile;
use sqlx::postgres::PgPool;
use std::{
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
    Updated(ScanJob),
    Finished(ScanJob),
}

#[derive(Debug)]
enum DiscoverMediaFilesOutcome {
    Completed(Vec<DiscoveredMediaFile>),
    Cancelled(i32),
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
            event_listener(ScanJobEvent::Finished(scan_job));
        }
        return Ok(ExecuteScanJobOutcome::Cancelled);
    }

    let library = match get_library(pool, library_id).await {
        Ok(library) => library,
        Err(ApplicationError::NotFound(_)) => {
            return Ok(ExecuteScanJobOutcome::Cancelled);
        }
        Err(error) => {
            if let Some(scan_job) =
                finalize_failed_scan(pool, scan_job_id, 0, 0, &error.to_string()).await
            {
                event_listener(ScanJobEvent::Finished(scan_job));
            }
            return Err(error);
        }
    };

    if is_cancelled(&cancellation_flag) {
        if let Some(scan_job) = finalize_cancelled_scan(pool, scan_job_id, 0, 0).await {
            event_listener(ScanJobEvent::Finished(scan_job));
        }
        return Ok(ExecuteScanJobOutcome::Cancelled);
    }

    match mova_db::mark_scan_job_running(pool, scan_job_id).await {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Updated(scan_job));
        }
        Ok(None) => {
            return Ok(ExecuteScanJobOutcome::Cancelled);
        }
        Err(error) => {
            let error = ApplicationError::from(error);
            if let Some(scan_job) =
                finalize_failed_scan(pool, scan_job_id, 0, 0, &error.to_string()).await
            {
                event_listener(ScanJobEvent::Finished(scan_job));
            }
            return Err(error);
        }
    }

    if is_cancelled(&cancellation_flag) {
        if let Some(scan_job) = finalize_cancelled_scan(pool, scan_job_id, 0, 0).await {
            event_listener(ScanJobEvent::Finished(scan_job));
        }
        return Ok(ExecuteScanJobOutcome::Cancelled);
    }

    let discovered_files = match discover_media_files(
        pool,
        scan_job_id,
        &library.root_path,
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
                event_listener(ScanJobEvent::Finished(scan_job));
            }
            return Ok(ExecuteScanJobOutcome::Cancelled);
        }
        Err(error) => {
            if let Some(scan_job) =
                finalize_failed_scan(pool, scan_job_id, 0, 0, &error.to_string()).await
            {
                event_listener(ScanJobEvent::Finished(scan_job));
            }
            return Err(error);
        }
    };

    let total_files = discovered_files.len() as i32;
    match mova_db::update_scan_job_progress(pool, scan_job_id, Some(total_files), total_files).await
    {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Updated(scan_job));
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

    let discovered_files = enrich_discovered_files(
        &library,
        discovered_files,
        cancellation_flag.clone(),
        artwork_cache_dir,
        metadata_provider,
    )
    .await;

    let media_entries = match build_media_entries(&library, discovered_files) {
        Ok(entries) => entries,
        Err(error) => {
            if let Some(scan_job) =
                finalize_failed_scan(pool, scan_job_id, total_files, 0, &error.to_string()).await
            {
                event_listener(ScanJobEvent::Finished(scan_job));
            }
            return Err(error);
        }
    };

    if is_cancelled(&cancellation_flag) {
        if let Some(scan_job) =
            finalize_cancelled_scan(pool, scan_job_id, total_files, total_files).await
        {
            event_listener(ScanJobEvent::Finished(scan_job));
        }
        return Ok(ExecuteScanJobOutcome::Cancelled);
    }

    if let Err(error) = mova_db::sync_library_media(pool, library.id, &media_entries).await {
        if let Some(scan_job) =
            finalize_failed_scan(pool, scan_job_id, total_files, 0, &error.to_string()).await
        {
            event_listener(ScanJobEvent::Finished(scan_job));
        }
        return Err(ApplicationError::Unexpected(error));
    }

    match mova_db::finalize_scan_job(pool, scan_job_id, "success", total_files, total_files, None)
        .await
    {
        Ok(Some(scan_job)) => {
            event_listener(ScanJobEvent::Finished(scan_job.clone()));
            Ok(ExecuteScanJobOutcome::Completed(scan_job))
        }
        Ok(None) => Ok(ExecuteScanJobOutcome::Cancelled),
        Err(error) => Err(ApplicationError::from(error)),
    }
}

async fn enrich_discovered_files(
    library: &Library,
    mut discovered_files: Vec<DiscoveredMediaFile>,
    cancellation_flag: Arc<AtomicBool>,
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> Vec<DiscoveredMediaFile> {
    let mut enrichment = MetadataEnrichmentContext::new(
        artwork_cache_dir,
        metadata_provider,
        library.metadata_language.clone(),
    );

    for file in &mut discovered_files {
        if is_cancelled(&cancellation_flag) {
            break;
        }

        let media_type = classify_media_type(&library.library_type, &file.file_path);
        let lookup_type = metadata_lookup_type_for_media_type(media_type);
        enrichment.enrich_file(lookup_type, file).await;
    }

    discovered_files
}

async fn discover_media_files(
    pool: &PgPool,
    scan_job_id: i64,
    root_path: &str,
    cancellation_flag: Arc<AtomicBool>,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> ApplicationResult<DiscoverMediaFilesOutcome> {
    let root_path_string = root_path.to_string();
    let root_path_for_task = root_path_string.clone();
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<i32>();
    let progress_pool = pool.clone();
    let last_progress = Arc::new(AtomicI32::new(0));
    let last_progress_for_task = last_progress.clone();
    let progress_event_listener = event_listener.clone();

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
                    progress_event_listener(ScanJobEvent::Updated(scan_job));
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
        mova_scan::discover_media_files_with_progress_and_cancel(
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
            "scan worker failed to join for {}: {}",
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
            "failed to scan library directory {}: {}",
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
            video_codec: file.video_codec,
            audio_codec: file.audio_codec,
            width: file.width,
            height: file.height,
            bitrate: file.bitrate,
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
                })
                .collect(),
        });
    }

    Ok(entries)
}

fn is_cancelled(cancellation_flag: &Arc<AtomicBool>) -> bool {
    cancellation_flag.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use crate::media_classification::{LIBRARY_TYPE_MIXED, LIBRARY_TYPE_SERIES};
    use std::path::Path;

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
