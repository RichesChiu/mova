use crate::{
    error::{ApplicationError, ApplicationResult},
    libraries::get_library,
    media_classification::{classify_media_type, metadata_lookup_type_for_media_type},
    media_enrichment::MetadataEnrichmentContext,
    metadata::MetadataProvider,
};
use mova_scan::DiscoveredMediaFile;
use sqlx::postgres::PgPool;
use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

/// 处理某个媒体库下一批文件系统变动。
/// `existing_paths` 是当前仍存在的路径，`removed_paths` 是当前已经不存在的路径。
pub async fn sync_library_filesystem_changes(
    pool: &PgPool,
    library_id: i64,
    existing_paths: Vec<PathBuf>,
    removed_paths: Vec<PathBuf>,
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> ApplicationResult<()> {
    let library = get_library(pool, library_id).await?;
    let mut enrichment = MetadataEnrichmentContext::new(
        artwork_cache_dir,
        metadata_provider,
        library.metadata_language.clone(),
    );
    let mut upserted_video_paths = HashSet::new();
    let mut removed_video_paths = HashSet::new();
    let mut removed_directory_prefixes = HashSet::new();

    for path in existing_paths {
        collect_upsert_targets(&path, &mut upserted_video_paths)?;
    }

    for path in removed_paths {
        if is_supported_video_path(&path) {
            removed_video_paths.insert(path_to_string(&path));
            continue;
        }

        if is_sidecar_or_artwork_path(&path) {
            collect_upsert_targets_for_parent(&path, &mut upserted_video_paths)?;
            continue;
        }

        if looks_like_directory_path(&path) {
            removed_directory_prefixes.insert(path_to_string(&path));
        }
    }

    let mut upserted_video_paths = upserted_video_paths.into_iter().collect::<Vec<_>>();
    upserted_video_paths.sort();

    for path in upserted_video_paths {
        if !path.is_file() {
            continue;
        }

        let media_type = classify_media_type(&library.library_type, &path);
        let lookup_type = metadata_lookup_type_for_media_type(media_type);
        let mut discovered_file = inspect_media_file(&path).await?;
        enrichment
            .enrich_file(lookup_type, &mut discovered_file)
            .await;
        let Some(entry) = build_media_entry(&library, media_type, discovered_file)? else {
            continue;
        };

        mova_db::upsert_library_media_entry_by_file_path(pool, library.id, &entry)
            .await
            .map_err(ApplicationError::from)?;
    }

    for directory_prefix in removed_directory_prefixes {
        mova_db::delete_library_media_by_path_prefix(pool, library.id, &directory_prefix)
            .await
            .map_err(ApplicationError::from)?;
    }

    for file_path in removed_video_paths {
        mova_db::delete_library_media_by_file_path(pool, library.id, &file_path)
            .await
            .map_err(ApplicationError::from)?;
    }

    Ok(())
}

/// 轻量级校准媒体库文件清单，只处理新增和删除路径，不重跑全量 metadata 解析。
pub async fn reconcile_library_inventory(
    pool: &PgPool,
    library_id: i64,
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> ApplicationResult<()> {
    let library = get_library(pool, library_id).await?;
    let root_path = library.root_path.clone();
    let db_paths = mova_db::list_library_media_file_paths(pool, library_id)
        .await
        .map_err(ApplicationError::from)?;
    let discovered_paths = discover_media_paths(PathBuf::from(&root_path)).await?;

    let discovered_strings = discovered_paths
        .iter()
        .map(|path| path_to_string(path))
        .collect::<HashSet<_>>();
    let db_path_set = db_paths.iter().cloned().collect::<HashSet<_>>();

    let existing_paths = discovered_paths
        .into_iter()
        .filter(|path| !db_path_set.contains(&path_to_string(path)))
        .collect::<Vec<_>>();
    let removed_paths = db_paths
        .into_iter()
        .filter(|path| !discovered_strings.contains(path))
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    if existing_paths.is_empty() && removed_paths.is_empty() {
        return Ok(());
    }

    sync_library_filesystem_changes(
        pool,
        library_id,
        existing_paths,
        removed_paths,
        artwork_cache_dir,
        metadata_provider,
    )
    .await
}

fn collect_upsert_targets(path: &Path, targets: &mut HashSet<PathBuf>) -> ApplicationResult<()> {
    if path.is_dir() {
        for discovered in mova_scan::discover_media_files(path).map_err(map_discover_io_error)? {
            targets.insert(discovered.file_path);
        }
        return Ok(());
    }

    if is_supported_video_path(path) {
        targets.insert(path.to_path_buf());
        return Ok(());
    }

    if is_sidecar_or_artwork_path(path) {
        collect_upsert_targets_for_parent(path, targets)?;
    }

    Ok(())
}

fn collect_upsert_targets_for_parent(
    path: &Path,
    targets: &mut HashSet<PathBuf>,
) -> ApplicationResult<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    for discovered in mova_scan::discover_media_files(parent).map_err(map_discover_io_error)? {
        targets.insert(discovered.file_path);
    }

    Ok(())
}

async fn inspect_media_file(path: &Path) -> ApplicationResult<DiscoveredMediaFile> {
    let path = path.to_path_buf();
    let join_path = path.display().to_string();
    tokio::task::spawn_blocking(move || mova_scan::inspect_media_file(&path))
        .await
        .map_err(|error| {
            ApplicationError::Unexpected(anyhow::anyhow!(
                "file sync worker failed to join for {}: {}",
                join_path,
                error
            ))
        })?
        .map_err(map_discover_io_error)
}

async fn discover_media_paths(root_path: PathBuf) -> ApplicationResult<Vec<PathBuf>> {
    let join_path = root_path.display().to_string();
    tokio::task::spawn_blocking(move || mova_scan::discover_media_paths(&root_path))
        .await
        .map_err(|error| {
            ApplicationError::Unexpected(anyhow::anyhow!(
                "inventory reconcile worker failed to join for {}: {}",
                join_path,
                error
            ))
        })?
        .map_err(map_discover_io_error)
}

fn build_media_entry(
    library: &mova_domain::Library,
    media_type: &str,
    file: DiscoveredMediaFile,
) -> ApplicationResult<Option<mova_db::CreateMediaEntryParams>> {
    if media_type == "episode" && (file.season_number.is_none() || file.episode_number.is_none()) {
        tracing::warn!(
            file_path = %file.file_path.display(),
            library_id = library.id,
            "skipping episode-like filesystem change because season/episode number could not be parsed"
        );
        return Ok(None);
    }

    let file_path = file.file_path.to_string_lossy().to_string();
    let file_size = i64::try_from(file.file_size).map_err(|_| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "file is too large to store in database: {}",
            file_path
        ))
    })?;

    Ok(Some(mova_db::CreateMediaEntryParams {
        library_id: library.id,
        media_type: media_type.to_string(),
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
    }))
}

fn is_supported_video_path(path: &Path) -> bool {
    matches!(
        extension_lowercase(path).as_deref(),
        Some("mp4" | "mkv" | "avi" | "mov" | "m4v" | "wmv" | "flv" | "webm" | "mpg" | "mpeg")
    )
}

fn is_sidecar_or_artwork_path(path: &Path) -> bool {
    matches!(
        extension_lowercase(path).as_deref(),
        Some("nfo" | "jpg" | "jpeg" | "png" | "webp" | "avif")
    )
}

fn looks_like_directory_path(path: &Path) -> bool {
    path.extension().is_none()
}

fn extension_lowercase(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn map_discover_io_error(error: io::Error) -> ApplicationError {
    ApplicationError::Unexpected(anyhow::anyhow!(
        "failed to inspect media filesystem change: {}",
        error
    ))
}
