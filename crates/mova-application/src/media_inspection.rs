//! On-demand inspection of one item's files when a user opens it.
//!
//! The inspection only looks: it never deletes. A missing file is reported to
//! the client and handed to a scan limited to the directories that held the
//! item's files; that scan decides whether the item is gone, moved, or only
//! short of some versions, under the same rules as a whole-library scan.

use crate::{error::ApplicationResult, ApplicationError};
use mova_domain::{Library, MediaItem, MediaSourceKind};
use mova_scan::{catalog_path_presence, CatalogPathPresence};
use sqlx::PgPool;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};

/// How long the click-time look-up may take before the page opens without a
/// verdict; slow storage must not hold the detail page.
const INSPECTION_TIMEOUT: Duration = Duration::from_secs(8);

pub const MEDIA_SOURCE_STATUS_AVAILABLE: &str = "available";
pub const MEDIA_SOURCE_STATUS_MISSING: &str = "missing";
pub const MEDIA_SOURCE_STATUS_STORAGE_UNAVAILABLE: &str = "storage_unavailable";

/// Whether an item can be opened for playback right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaSourceStatus {
    /// At least one version is on disk, or nothing could be checked.
    Available,
    /// Every local version is gone; a scoped scan decides what follows.
    Missing,
    /// The library's storage is unavailable; nothing was looked up.
    StorageUnavailable,
}

impl MediaSourceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => MEDIA_SOURCE_STATUS_AVAILABLE,
            Self::Missing => MEDIA_SOURCE_STATUS_MISSING,
            Self::StorageUnavailable => MEDIA_SOURCE_STATUS_STORAGE_UNAVAILABLE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaItemSourceInspection {
    pub status: MediaSourceStatus,
    /// Local versions that are no longer on disk.
    pub missing_media_file_ids: Vec<i64>,
    /// Directories a scan should inspect because files went missing there.
    pub rescan_directories: Vec<String>,
}

impl MediaItemSourceInspection {
    fn available() -> Self {
        Self {
            status: MediaSourceStatus::Available,
            missing_media_file_ids: Vec::new(),
            rescan_directories: Vec::new(),
        }
    }
}

/// Checks the local files of a movie or an episode on disk.
///
/// Series are not checked: their availability is the sum of their episodes.
/// An item without any file, an episode kept while its series has media, is
/// reported missing without a look-up. STRM carriers are never checked. A file that cannot be looked up because
/// the storage does not answer, or when the look-up times out, counts as
/// present: only a confirmed absence is reported.
pub async fn inspect_media_item_sources(
    pool: &PgPool,
    library: &Library,
    media_item: &MediaItem,
) -> ApplicationResult<MediaItemSourceInspection> {
    if media_item.media_type == "series" {
        return Ok(MediaItemSourceInspection::available());
    }
    if !library.is_storage_available() {
        return Ok(MediaItemSourceInspection {
            status: MediaSourceStatus::StorageUnavailable,
            missing_media_file_ids: Vec::new(),
            rescan_directories: Vec::new(),
        });
    }

    let media_files = mova_db::list_media_files_for_media_item(pool, media_item.id)
        .await
        .map_err(ApplicationError::Unexpected)?;
    // An episode kept without any file (its series still has media) is
    // already known to be gone; nothing is left to look up or to rescan.
    if media_files.is_empty() {
        return Ok(MediaItemSourceInspection {
            status: MediaSourceStatus::Missing,
            missing_media_file_ids: Vec::new(),
            rescan_directories: Vec::new(),
        });
    }
    let local_files = media_files
        .iter()
        .filter(|file| file.source_kind == MediaSourceKind::LocalFile)
        .map(|file| (file.id, file.file_path.clone()))
        .collect::<Vec<_>>();
    if local_files.is_empty() {
        return Ok(MediaItemSourceInspection::available());
    }
    let local_file_count = local_files.len();
    let has_remote_versions = media_files.len() > local_file_count;

    let root_path = PathBuf::from(&library.root_path);
    let lookup = tokio::time::timeout(
        INSPECTION_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            let canonical_root =
                std::fs::canonicalize(&root_path).unwrap_or_else(|_| root_path.clone());
            local_files
                .into_iter()
                .filter_map(|(media_file_id, file_path)| {
                    match catalog_path_presence(Path::new(&file_path), &canonical_root) {
                        CatalogPathPresence::Present => None,
                        CatalogPathPresence::Absent => Some((media_file_id, file_path)),
                        CatalogPathPresence::Failed(error) => {
                            tracing::warn!(
                                media_file_id,
                                file_path,
                                error = %error,
                                "could not verify a media file on open; treating it as present"
                            );
                            None
                        }
                    }
                })
                .collect::<Vec<_>>()
        }),
    )
    .await;
    let missing = match lookup {
        Ok(Ok(missing)) => missing,
        Ok(Err(error)) => {
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(
                "The media inspection worker exited unexpectedly: {error}"
            )));
        }
        Err(_) => {
            tracing::warn!(
                media_item_id = media_item.id,
                "media file verification on open timed out; opening without a verdict"
            );
            return Ok(MediaItemSourceInspection::available());
        }
    };
    if missing.is_empty() {
        return Ok(MediaItemSourceInspection::available());
    }

    let all_local_missing = missing.len() == local_file_count;
    let status = if all_local_missing && !has_remote_versions {
        MediaSourceStatus::Missing
    } else {
        MediaSourceStatus::Available
    };
    let rescan_directories = missing
        .iter()
        .filter_map(|(_, file_path)| {
            Path::new(file_path)
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(MediaItemSourceInspection {
        status,
        missing_media_file_ids: missing.into_iter().map(|(id, _)| id).collect(),
        rescan_directories,
    })
}
