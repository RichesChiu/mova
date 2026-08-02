use super::parse::{episode_identity_for_path, parse_year_token};
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

const MAX_MEDIA_NFO_BYTES: usize = 2 * 1024 * 1024;
const MAX_TVSHOW_NFO_BYTES: usize = 4 * 1024 * 1024;
const MAX_NFO_ELEMENTS: usize = 100_000;
const MAX_NFO_FIELD_BYTES: usize = 256 * 1024;
const MAX_NFO_ACTORS: usize = 5_000;
const MAX_NFO_CREDITS: usize = 10_000;
const MAX_NFO_IMAGES: usize = 4_096;
const MAX_NFO_UNIQUE_IDS: usize = 16_384;
const MAX_NFO_RATINGS: usize = 1_024;
const MAX_NFO_NAMED_SEASONS: usize = 1_024;
const MAX_NFO_MULTI_VALUE_ITEMS: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalNfoKind {
    Movie,
    TvShow,
    Episode,
}

/// File-level NFO scopes accepted by [`observe_media_nfo_for_kind`].
///
/// A dedicated type prevents a caller from accidentally treating a
/// `tvshow.nfo` as a movie/episode sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaNfoKind {
    Movie,
    Episode,
}

impl From<MediaNfoKind> for LocalNfoKind {
    fn from(value: MediaNfoKind) -> Self {
        match value {
            MediaNfoKind::Movie => Self::Movie,
            MediaNfoKind::Episode => Self::Episode,
        }
    }
}

/// Stable reason codes for an NFO candidate that exists but cannot be used.
///
/// The code intentionally excludes operating-system and XML parser messages so
/// callers can persist or localize it without coupling to dependency wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalNfoErrorCode {
    OpenFailed,
    InspectFailed,
    NotRegularFile,
    TooLarge,
    ReadFailed,
    GrewBeyondLimit,
    InvalidUtf8,
    ForbiddenXmlDeclaration,
    MalformedXml,
    UnsupportedRoot,
    UnexpectedRootKind,
    OutsideLibraryRoot,
    SymlinkNotAllowed,
    SecureOpenUnavailable,
    ResourceLimitExceeded,
}

/// Result of observing the ordered NFO candidates for one metadata scope.
///
/// `Invalid` always refers to the first existing candidate. A lower-priority
/// candidate is never used to hide a broken higher-priority NFO.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalNfoObservation {
    Valid(Box<LocalNfoMetadata>),
    Invalid {
        candidate_path: PathBuf,
        error_code: LocalNfoErrorCode,
    },
    Absent {
        candidate_paths: Vec<PathBuf>,
    },
}

impl LocalNfoObservation {
    fn into_valid(self) -> Option<LocalNfoMetadata> {
        match self {
            Self::Valid(metadata) => Some(*metadata),
            Self::Invalid { .. } | Self::Absent { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNfoUniqueId {
    pub provider: String,
    pub value: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalNfoRating {
    pub source: String,
    pub kind: LocalNfoRatingKind,
    pub value: f64,
    pub scale: f64,
    #[serde(default)]
    pub votes: Option<u64>,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalNfoRatingKind {
    Audience,
    Critic,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNfoArtwork {
    pub posters: Vec<String>,
    pub backdrops: Vec<String>,
    pub logos: Vec<String>,
    pub thumbnails: Vec<String>,
    /// Type-preserving image inventory. The convenience arrays above remain
    /// the projection inputs used by current clients.
    #[serde(default)]
    pub images: Vec<LocalNfoImage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalNfoImageKind {
    Poster,
    Backdrop,
    Logo,
    Banner,
    Landscape,
    ClearArt,
    DiscArt,
    KeyArt,
    Thumbnail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNfoImage {
    pub kind: LocalNfoImageKind,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNfoActor {
    pub name: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub order: Option<u32>,
    #[serde(default)]
    pub thumb_path: Option<String>,
    /// Optional person kind emitted by Emby-family NFO writers.
    #[serde(default)]
    pub person_type: Option<String>,
    /// Person-scoped provider identifiers. They remain scoped to this actor
    /// and are never mixed with the movie/show/episode identifiers.
    #[serde(default)]
    pub unique_ids: Vec<LocalNfoUniqueId>,
    /// Optional free-form profile/biography text emitted by some writers.
    #[serde(default)]
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNfoCredits {
    pub actors: Vec<LocalNfoActor>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    /// Emby/Kodi compatibility marker. It remains snapshot-only until MOVA
    /// defines ownership semantics for explicitly empty local fields.
    #[serde(default)]
    pub clear_actors: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNfoCollection {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub unique_ids: Vec<LocalNfoUniqueId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNfoNamedSeason {
    pub season_number: i32,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub artwork: LocalNfoArtwork,
}

/// Structured, user-owned metadata read from a Kodi/Emby compatible NFO file.
///
/// Playback state and ffprobe-owned stream facts intentionally are not imported:
/// NFO playback state has no MOVA user identity, while the media file is the
/// authoritative source for its technical streams.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalNfoMetadata {
    pub kind: LocalNfoKind,
    /// Canonical NFO file path used as persistence provenance. It is runtime
    /// provenance rather than normalized metadata and is not serialized.
    #[serde(skip)]
    pub source_path: PathBuf,
    /// Runtime-only guard used when otherwise valid NFO documents disagree on
    /// the TMDB identity of one logical media item. The full identifiers stay
    /// in the source snapshot, but the conflicting TMDB value is not projected
    /// into the aggregate identity table.
    #[serde(skip, default)]
    pub suppress_tmdb_identity_projection: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub original_title: Option<String>,
    #[serde(default)]
    pub sort_title: Option<String>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub outline: Option<String>,
    #[serde(default)]
    pub tagline: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub premiered: Option<String>,
    #[serde(default)]
    pub aired: Option<String>,
    /// Source-provided library timestamp retained for diagnostics and future
    /// export compatibility. It never replaces MOVA's own `created_at`.
    #[serde(default)]
    pub date_added: Option<String>,
    #[serde(default)]
    pub runtime_minutes: Option<u32>,
    #[serde(default)]
    pub content_rating: Option<String>,
    #[serde(default)]
    pub original_language: Option<String>,
    #[serde(default)]
    pub preferred_metadata_language: Option<String>,
    #[serde(default)]
    pub preferred_metadata_country_code: Option<String>,
    #[serde(default)]
    pub show_title: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub display_order: Option<String>,
    #[serde(default)]
    pub air_days: Vec<String>,
    #[serde(default)]
    pub air_time: Option<String>,
    #[serde(default)]
    pub custom_rating: Option<String>,
    #[serde(default)]
    pub trailers: Vec<String>,
    #[serde(default)]
    pub aspect_ratio: Option<String>,
    #[serde(default)]
    pub top_250: Option<i32>,
    #[serde(default)]
    pub season_number: Option<i32>,
    #[serde(default)]
    pub episode_number: Option<i32>,
    #[serde(default)]
    pub season_count: Option<u32>,
    #[serde(default)]
    pub episode_count: Option<u32>,
    #[serde(default)]
    pub display_episode_number: Option<i32>,
    #[serde(default)]
    pub display_season_number: Option<i32>,
    #[serde(default)]
    pub display_after_season_number: Option<i32>,
    #[serde(default)]
    pub show_link: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub countries: Vec<String>,
    #[serde(default)]
    pub studios: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default)]
    pub named_seasons: Vec<LocalNfoNamedSeason>,
    #[serde(default)]
    pub unique_ids: Vec<LocalNfoUniqueId>,
    #[serde(default)]
    pub episode_guide_ids: Vec<LocalNfoUniqueId>,
    #[serde(default)]
    pub ratings: Vec<LocalNfoRating>,
    #[serde(default)]
    pub credits: LocalNfoCredits,
    #[serde(default)]
    pub artwork: LocalNfoArtwork,
    #[serde(default)]
    pub collection: Option<LocalNfoCollection>,
    #[serde(default)]
    pub lock_data: bool,
    #[serde(default)]
    pub locked_fields: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ArtworkKind {
    Poster,
    Backdrop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArtworkScope {
    FileSpecific,
    Generic,
}

#[cfg(test)]
pub(crate) fn read_sidecar_metadata(path: &Path) -> Option<LocalNfoMetadata> {
    let expected_kind = if episode_identity_for_path(path).is_some() {
        MediaNfoKind::Episode
    } else {
        MediaNfoKind::Movie
    };
    observe_media_nfo_for_kind(path, expected_kind).into_valid()
}

pub(crate) fn read_series_sidecar_metadata(path: &Path) -> Option<LocalNfoMetadata> {
    observe_series_nfo(path).into_valid()
}

pub(crate) fn read_series_sidecar_metadata_within_root(
    path: &Path,
    root_path: &Path,
) -> Option<LocalNfoMetadata> {
    observe_series_nfo_within_root(path, root_path).into_valid()
}

/// Observe the file-specific movie/episode NFO and then the generic
/// `movie.nfo` candidate, without opening or probing the media file.
pub fn observe_media_nfo(path: &Path) -> LocalNfoObservation {
    let expected_kind = if episode_identity_for_path(path).is_some() {
        MediaNfoKind::Episode
    } else {
        MediaNfoKind::Movie
    };
    observe_media_nfo_for_kind(path, expected_kind)
}

/// Observe a movie or episode NFO using the caller-provided authoritative
/// media kind rather than inferring the scope from its filename.
///
/// Movies try `<stem>.nfo` and then `movie.nfo`. Episodes only try
/// `<stem>.nfo`, even when the filename itself has no season/episode marker.
pub fn observe_media_nfo_for_kind(path: &Path, expected_kind: MediaNfoKind) -> LocalNfoObservation {
    observe_nfo_candidates(
        media_nfo_candidate_paths(path, expected_kind),
        MAX_MEDIA_NFO_BYTES,
        expected_kind.into(),
    )
}

/// Root-bounded variant used by every library scan and manual refresh.
pub fn observe_media_nfo_for_kind_within_root(
    path: &Path,
    expected_kind: MediaNfoKind,
    root_path: &Path,
) -> LocalNfoObservation {
    observe_nfo_candidates_with_optional_root(
        media_nfo_candidate_paths(path, expected_kind),
        MAX_MEDIA_NFO_BYTES,
        expected_kind.into(),
        Some(root_path),
    )
}

/// Observe the nearest `tvshow.nfo` among the same five ancestors used by the
/// legacy series-sidecar lookup, without opening or probing the media file.
pub fn observe_series_nfo(path: &Path) -> LocalNfoObservation {
    let candidates = path
        .parent()
        .into_iter()
        .flat_map(|parent| parent.ancestors().take(5))
        .map(|directory| directory.join("tvshow.nfo"))
        .collect();
    observe_nfo_candidates(candidates, MAX_TVSHOW_NFO_BYTES, LocalNfoKind::TvShow)
}

/// Observe the nearest `tvshow.nfo` without allowing a candidate outside the
/// supplied media-library root. No media stream probing is performed.
pub fn observe_series_nfo_within_root(path: &Path, root_path: &Path) -> LocalNfoObservation {
    let candidates = series_nfo_candidate_paths_within_root(path, root_path);
    observe_nfo_candidates_with_optional_root(
        candidates,
        MAX_TVSHOW_NFO_BYTES,
        LocalNfoKind::TvShow,
        Some(root_path),
    )
}

/// Re-observe one already known NFO source path without applying candidate
/// fallback rules. This powers the on-demand metadata-source inspection API.
pub fn observe_nfo_file(path: &Path, expected_kind: LocalNfoKind) -> LocalNfoObservation {
    let max_bytes = match expected_kind {
        LocalNfoKind::TvShow => MAX_TVSHOW_NFO_BYTES,
        LocalNfoKind::Movie | LocalNfoKind::Episode => MAX_MEDIA_NFO_BYTES,
    };
    observe_nfo_candidates(vec![path.to_path_buf()], max_bytes, expected_kind)
}

/// Re-observe a persisted source while enforcing its owning library root.
pub fn observe_nfo_file_within_root(
    path: &Path,
    expected_kind: LocalNfoKind,
    root_path: &Path,
) -> LocalNfoObservation {
    let max_bytes = match expected_kind {
        LocalNfoKind::TvShow => MAX_TVSHOW_NFO_BYTES,
        LocalNfoKind::Movie | LocalNfoKind::Episode => MAX_MEDIA_NFO_BYTES,
    };
    observe_nfo_candidates_with_optional_root(
        vec![path.to_path_buf()],
        max_bytes,
        expected_kind,
        Some(root_path),
    )
}

fn observe_nfo_candidates(
    candidate_paths: Vec<PathBuf>,
    max_bytes: usize,
    expected_kind: LocalNfoKind,
) -> LocalNfoObservation {
    observe_nfo_candidates_with_optional_root(candidate_paths, max_bytes, expected_kind, None)
}

fn observe_nfo_candidates_with_optional_root(
    candidate_paths: Vec<PathBuf>,
    max_bytes: usize,
    expected_kind: LocalNfoKind,
    root_path: Option<&Path>,
) -> LocalNfoObservation {
    let canonical_root = match root_path.map(fs::canonicalize).transpose() {
        Ok(root) => root,
        Err(error) => {
            let candidate_path = candidate_paths
                .first()
                .cloned()
                .unwrap_or_else(|| root_path.unwrap_or_else(|| Path::new("/")).to_path_buf());
            tracing::warn!(
                library_root = %root_path.unwrap_or_else(|| Path::new("/")).display(),
                error = %error,
                "failed to resolve media-library root for NFO observation"
            );
            return LocalNfoObservation::Invalid {
                candidate_path,
                error_code: LocalNfoErrorCode::InspectFailed,
            };
        }
    };
    let mut selected_candidate = None;
    for candidate_path in &candidate_paths {
        if let Some(root_path) = root_path {
            if !candidate_path.starts_with(root_path) {
                return LocalNfoObservation::Invalid {
                    candidate_path: candidate_path.clone(),
                    error_code: LocalNfoErrorCode::OutsideLibraryRoot,
                };
            }
        }
        match fs::symlink_metadata(candidate_path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                tracing::warn!(
                    file_path = %candidate_path.display(),
                    "sidecar nfo symlinks are not allowed"
                );
                return LocalNfoObservation::Invalid {
                    candidate_path: candidate_path.clone(),
                    error_code: LocalNfoErrorCode::SymlinkNotAllowed,
                };
            }
            Ok(metadata) if metadata.is_file() => {
                if let Some(canonical_root) = canonical_root.as_deref() {
                    match fs::canonicalize(candidate_path) {
                        Ok(canonical_candidate)
                            if canonical_candidate.starts_with(canonical_root) => {}
                        Ok(_) => {
                            return LocalNfoObservation::Invalid {
                                candidate_path: candidate_path.clone(),
                                error_code: LocalNfoErrorCode::OutsideLibraryRoot,
                            };
                        }
                        Err(error) => {
                            tracing::warn!(
                                file_path = %candidate_path.display(),
                                error = %error,
                                "failed to resolve sidecar nfo candidate"
                            );
                            return LocalNfoObservation::Invalid {
                                candidate_path: candidate_path.clone(),
                                error_code: LocalNfoErrorCode::InspectFailed,
                            };
                        }
                    }
                }
                selected_candidate = Some(candidate_path.clone());
                break;
            }
            Ok(_) => {
                tracing::warn!(
                    file_path = %candidate_path.display(),
                    "sidecar nfo path is not a regular file"
                );
                return LocalNfoObservation::Invalid {
                    candidate_path: candidate_path.clone(),
                    error_code: LocalNfoErrorCode::NotRegularFile,
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(
                    file_path = %candidate_path.display(),
                    error = %error,
                    "failed to inspect sidecar nfo candidate"
                );
                return LocalNfoObservation::Invalid {
                    candidate_path: candidate_path.clone(),
                    error_code: LocalNfoErrorCode::InspectFailed,
                };
            }
        }
    }
    let Some(candidate_path) = selected_candidate else {
        return LocalNfoObservation::Absent { candidate_paths };
    };

    let (contents, resolved_source_path) =
        match read_nfo_file(&candidate_path, max_bytes, root_path) {
            Ok(result) => result,
            Err(error_code) => {
                return LocalNfoObservation::Invalid {
                    candidate_path,
                    error_code,
                };
            }
        };
    let mut metadata = match parse_nfo_metadata_result(
        &contents,
        candidate_path.parent().unwrap_or_else(|| Path::new("/")),
    ) {
        Ok(metadata) => metadata,
        Err(error_code) => {
            return LocalNfoObservation::Invalid {
                candidate_path,
                error_code,
            };
        }
    };
    if metadata.kind != expected_kind {
        tracing::warn!(
            file_path = %candidate_path.display(),
            actual_kind = ?metadata.kind,
            expected_kind = ?expected_kind,
            "sidecar nfo root type does not match its media use"
        );
        return LocalNfoObservation::Invalid {
            candidate_path,
            error_code: LocalNfoErrorCode::UnexpectedRootKind,
        };
    }

    metadata.source_path = resolved_source_path;
    LocalNfoObservation::Valid(Box::new(metadata))
}

fn read_nfo_file(
    path: &Path,
    max_bytes: usize,
    root_path: Option<&Path>,
) -> Result<(String, PathBuf), LocalNfoErrorCode> {
    let (mut file, resolved_source_path) = open_nfo_file(path, root_path)?;
    let metadata = match file.metadata() {
        Ok(metadata) => metadata,
        Err(error) => {
            tracing::warn!(
                file_path = %path.display(),
                error = %error,
                "failed to inspect sidecar nfo file"
            );
            return Err(LocalNfoErrorCode::InspectFailed);
        }
    };

    if !metadata.is_file() {
        tracing::warn!(
            file_path = %path.display(),
            "sidecar nfo path is not a regular file"
        );
        return Err(LocalNfoErrorCode::NotRegularFile);
    }
    if metadata.len() > max_bytes as u64 {
        tracing::warn!(
            file_path = %path.display(),
            file_bytes = metadata.len(),
            max_bytes,
            "sidecar nfo file exceeds the size limit"
        );
        return Err(LocalNfoErrorCode::TooLarge);
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let read_result = (&mut file)
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes);
    if let Err(error) = read_result {
        tracing::warn!(
            file_path = %path.display(),
            error = %error,
            "failed to read sidecar nfo file"
        );
        return Err(LocalNfoErrorCode::ReadFailed);
    }
    if bytes.len() > max_bytes {
        tracing::warn!(
            file_path = %path.display(),
            file_bytes = bytes.len(),
            max_bytes,
            "sidecar nfo file grew beyond the size limit while being read"
        );
        return Err(LocalNfoErrorCode::GrewBeyondLimit);
    }

    match String::from_utf8(bytes) {
        Ok(contents) => Ok((contents, resolved_source_path)),
        Err(error) => {
            tracing::warn!(
                file_path = %path.display(),
                error = %error,
                "sidecar nfo file is not valid UTF-8"
            );
            Err(LocalNfoErrorCode::InvalidUtf8)
        }
    }
}

#[cfg(unix)]
fn open_nfo_file(
    path: &Path,
    root_path: Option<&Path>,
) -> Result<(fs::File, PathBuf), LocalNfoErrorCode> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::OpenOptionsExt},
        },
    };

    let open_at = |directory: &fs::File, name: &std::ffi::OsStr, directory_only: bool| {
        let name = CString::new(name.as_bytes()).map_err(|_| LocalNfoErrorCode::OpenFailed)?;
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if directory_only { libc::O_DIRECTORY } else { 0 };
        // SAFETY: `directory` owns a valid descriptor, `name` is a live
        // NUL-terminated component, and a successful descriptor is
        // immediately transferred into `File` ownership exactly once.
        let descriptor = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if descriptor < 0 {
            let error = std::io::Error::last_os_error();
            return Err(if matches!(error.raw_os_error(), Some(libc::ELOOP)) {
                LocalNfoErrorCode::SymlinkNotAllowed
            } else {
                LocalNfoErrorCode::OpenFailed
            });
        }
        // SAFETY: `descriptor` was just returned by `openat` and has not been
        // wrapped or closed elsewhere.
        Ok(unsafe { fs::File::from_raw_fd(descriptor) })
    };

    let Some(root_path) = root_path else {
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(path)
            .map_err(|error| {
                tracing::warn!(
                    file_path = %path.display(),
                    error = %error,
                    "failed to open sidecar nfo file without following symlinks"
                );
                if matches!(error.raw_os_error(), Some(libc::ELOOP)) {
                    LocalNfoErrorCode::SymlinkNotAllowed
                } else {
                    LocalNfoErrorCode::OpenFailed
                }
            })?;
        let resolved_source_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        return Ok((file, resolved_source_path));
    };

    let relative_path = path
        .strip_prefix(root_path)
        .ok()
        .filter(|relative| {
            !relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        })
        .ok_or(LocalNfoErrorCode::OutsideLibraryRoot)?;
    let file_name = relative_path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or(LocalNfoErrorCode::OpenFailed)?;
    let canonical_root = fs::canonicalize(root_path).map_err(|error| {
        tracing::warn!(
            library_root = %root_path.display(),
            error = %error,
            "failed to resolve media-library root before opening NFO"
        );
        LocalNfoErrorCode::InspectFailed
    })?;
    let canonical_parent =
        fs::canonicalize(path.parent().unwrap_or(root_path)).map_err(|error| {
            tracing::warn!(
                file_path = %path.display(),
                error = %error,
                "failed to resolve sidecar NFO parent directory"
            );
            LocalNfoErrorCode::OpenFailed
        })?;
    let relative_parent = canonical_parent
        .strip_prefix(&canonical_root)
        .map_err(|_| LocalNfoErrorCode::OutsideLibraryRoot)?;

    let mut directory = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(&canonical_root)
        .map_err(|error| {
            tracing::warn!(
                library_root = %canonical_root.display(),
                error = %error,
                "failed to open media-library root for bounded NFO traversal"
            );
            LocalNfoErrorCode::InspectFailed
        })?;
    for component in relative_parent.components() {
        let Component::Normal(name) = component else {
            return Err(LocalNfoErrorCode::OutsideLibraryRoot);
        };
        directory = open_at(&directory, name, true)?;
    }
    let file = open_at(&directory, file_name, false)?;

    Ok((file, canonical_parent.join(file_name)))
}

#[cfg(not(unix))]
fn open_nfo_file(
    path: &Path,
    root_path: Option<&Path>,
) -> Result<(fs::File, PathBuf), LocalNfoErrorCode> {
    let _ = root_path;
    tracing::warn!(
        file_path = %path.display(),
        "secure descriptor-relative NFO opening is unavailable on this platform"
    );
    // A path-based open followed by canonicalization validates a different
    // filesystem lookup from the already opened handle and is vulnerable to a
    // rename/symlink race. Until this platform has a handle-relative traversal
    // implementation equivalent to Unix openat(O_NOFOLLOW), fail closed.
    Err(LocalNfoErrorCode::SecureOpenUnavailable)
}

fn media_nfo_candidate_paths(video_path: &Path, expected_kind: MediaNfoKind) -> Vec<PathBuf> {
    let mut candidates = vec![video_path.with_extension("nfo")];

    if expected_kind == MediaNfoKind::Movie {
        let Some(parent) = video_path.parent() else {
            return candidates;
        };
        let generic = parent.join("movie.nfo");
        if !candidates.contains(&generic) {
            candidates.push(generic);
        }
    }

    candidates
}

fn series_nfo_candidate_paths_within_root(path: &Path, root_path: &Path) -> Vec<PathBuf> {
    let Some(relative_path) = path.strip_prefix(root_path).ok().filter(|relative_path| {
        !relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    }) else {
        return Vec::new();
    };
    let Some(relative_parent) = relative_path.parent() else {
        return Vec::new();
    };

    relative_parent
        .ancestors()
        .take(5)
        .map(|directory| root_path.join(directory).join("tvshow.nfo"))
        .collect()
}

#[cfg(test)]
pub(crate) fn parse_nfo_metadata(contents: &str, base_dir: &Path) -> Option<LocalNfoMetadata> {
    parse_nfo_metadata_result(contents, base_dir).ok()
}

fn parse_nfo_metadata_result(
    contents: &str,
    base_dir: &Path,
) -> Result<LocalNfoMetadata, LocalNfoErrorCode> {
    let lowercase = contents.to_ascii_lowercase();
    if lowercase.contains("<!doctype") || lowercase.contains("<!entity") {
        tracing::warn!("sidecar nfo contains a forbidden DTD or entity declaration");
        return Err(LocalNfoErrorCode::ForbiddenXmlDeclaration);
    }

    let document = match Document::parse(contents) {
        Ok(document) => document,
        Err(error) => {
            tracing::warn!(error = %error, "failed to parse sidecar nfo XML");
            return Err(LocalNfoErrorCode::MalformedXml);
        }
    };
    let root = document.root_element();
    let kind = match root.tag_name().name().to_ascii_lowercase().as_str() {
        "movie" => LocalNfoKind::Movie,
        "tvshow" => LocalNfoKind::TvShow,
        "episodedetails" => LocalNfoKind::Episode,
        unsupported => {
            tracing::warn!(root = unsupported, "unsupported sidecar nfo root element");
            return Err(LocalNfoErrorCode::UnsupportedRoot);
        }
    };
    validate_nfo_resource_limits(&document)?;

    let mut unique_ids = parse_unique_ids(root);
    append_legacy_unique_ids(root, &mut unique_ids);
    append_legacy_id_element(root, &mut unique_ids);
    deduplicate_unique_ids(&mut unique_ids);

    Ok(LocalNfoMetadata {
        kind,
        source_path: PathBuf::new(),
        suppress_tmdb_identity_projection: false,
        title: first_child_text(root, &["title", "localtitle", "name"]),
        original_title: first_child_text(root, &["originaltitle"]),
        sort_title: first_child_text(root, &["sorttitle", "sortname"]),
        year: first_child_text(root, &["year"]).and_then(|value| parse_year_token(&value)),
        overview: first_child_text(root, &["plot", "biography", "review"]),
        outline: first_child_text(root, &["outline"]),
        tagline: first_child_text(root, &["tagline"]),
        status: first_child_text(root, &["status"]),
        premiered: first_child_text(root, &["premiered", "releasedate", "formed"]),
        aired: first_child_text(root, &["aired"]),
        date_added: first_child_text(root, &["dateadded"]),
        runtime_minutes: first_child_text(root, &["runtime"])
            .and_then(|value| parse_positive_u32(&value)),
        content_rating: first_child_text(root, &["mpaa", "contentrating", "certification"]),
        original_language: first_child_text(root, &["originallanguage", "original_language"]),
        preferred_metadata_language: first_child_text(root, &["language"]),
        preferred_metadata_country_code: first_child_text(root, &["countrycode"]),
        show_title: (kind == LocalNfoKind::Episode)
            .then(|| first_child_text(root, &["showtitle"]))
            .flatten(),
        end_date: first_child_text(root, &["enddate"]),
        display_order: first_child_text(root, &["displayorder"]),
        air_days: if kind == LocalNfoKind::TvShow {
            let mut values = child_texts(root, &["airs_dayofweek"])
                .into_iter()
                .flat_map(|value| {
                    value
                        .split(['/', ',', '|'])
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            deduplicate_case_insensitive(&mut values);
            values
        } else {
            Vec::new()
        },
        air_time: (kind == LocalNfoKind::TvShow)
            .then(|| first_child_text(root, &["airs_time"]))
            .flatten(),
        custom_rating: first_child_text(root, &["customrating"]),
        trailers: child_texts(root, &["trailer"]),
        aspect_ratio: first_child_text(root, &["aspectratio"]),
        top_250: first_child_text(root, &["top250"])
            .and_then(|value| value.parse::<i32>().ok())
            .filter(|value| *value >= 0),
        season_number: (kind == LocalNfoKind::Episode)
            .then(|| first_child_text(root, &["season"]))
            .flatten()
            .and_then(|value| value.parse::<i32>().ok()),
        episode_number: (kind == LocalNfoKind::Episode)
            .then(|| first_child_text(root, &["episode"]))
            .flatten()
            .and_then(|value| value.parse::<i32>().ok()),
        season_count: (kind == LocalNfoKind::TvShow)
            .then(|| first_child_text(root, &["season"]))
            .flatten()
            .and_then(|value| value.parse::<u32>().ok()),
        episode_count: (kind == LocalNfoKind::TvShow)
            .then(|| first_child_text(root, &["episode"]))
            .flatten()
            .and_then(|value| value.parse::<u32>().ok()),
        display_episode_number: (kind == LocalNfoKind::Episode)
            .then(|| first_child_text(root, &["displayepisode", "airsbefore_episode"]))
            .flatten()
            .and_then(|value| value.parse::<i32>().ok()),
        display_season_number: (kind == LocalNfoKind::Episode)
            .then(|| first_child_text(root, &["displayseason", "airsbefore_season"]))
            .flatten()
            .and_then(|value| value.parse::<i32>().ok()),
        display_after_season_number: (kind == LocalNfoKind::Episode)
            .then(|| first_child_text(root, &["airsafter_season"]))
            .flatten()
            .and_then(|value| value.parse::<i32>().ok()),
        show_link: (kind == LocalNfoKind::Movie)
            .then(|| first_child_text(root, &["showlink"]))
            .flatten(),
        genres: child_texts_split(root, &["genre"]),
        countries: child_texts_split(root, &["country"]),
        studios: child_texts(root, &["studio"]),
        tags: child_texts(root, &["tag"]),
        styles: child_texts(root, &["style"]),
        named_seasons: if kind == LocalNfoKind::TvShow {
            parse_named_seasons(root, base_dir)
        } else {
            Vec::new()
        },
        unique_ids,
        episode_guide_ids: if kind == LocalNfoKind::TvShow {
            parse_episode_guide_ids(root)?
        } else {
            Vec::new()
        },
        ratings: parse_ratings(root),
        credits: parse_credits(root, base_dir),
        artwork: parse_artwork(root, kind, base_dir),
        collection: parse_collection(root),
        lock_data: first_child_text(root, &["lockdata"]).is_some_and(|value| parse_bool(&value)),
        locked_fields: child_texts(root, &["lockedfield", "lockedfields"])
            .into_iter()
            .flat_map(|value| split_list_value(&value))
            .collect(),
    })
}

fn validate_nfo_resource_limits(document: &Document<'_>) -> Result<(), LocalNfoErrorCode> {
    let mut element_count = 0_usize;
    let mut actor_count = 0_usize;
    let mut credit_count = 0_usize;
    let mut image_count = 0_usize;
    let mut unique_id_count = 0_usize;
    let mut rating_count = 0_usize;
    let mut named_season_count = 0_usize;
    let mut multi_value_count = 0_usize;

    for node in document.descendants() {
        if node.is_text() {
            if node
                .text()
                .is_some_and(|value| value.len() > MAX_NFO_FIELD_BYTES)
            {
                return resource_limit_exceeded("field_bytes", MAX_NFO_FIELD_BYTES);
            }
            continue;
        }
        if !node.is_element() {
            continue;
        }

        element_count = element_count.saturating_add(1);
        if element_count > MAX_NFO_ELEMENTS {
            return resource_limit_exceeded("elements", MAX_NFO_ELEMENTS);
        }
        if node
            .attributes()
            .any(|attribute| attribute.value().len() > MAX_NFO_FIELD_BYTES)
        {
            return resource_limit_exceeded("field_bytes", MAX_NFO_FIELD_BYTES);
        }

        let name = node.tag_name().name().to_ascii_lowercase();
        match name.as_str() {
            "actor" => actor_count = actor_count.saturating_add(1),
            "director" => credit_count = credit_count.saturating_add(1),
            "credits" | "writer" => {
                credit_count = credit_count
                    .saturating_add(node.text().map(slash_list_item_count).unwrap_or(0));
            }
            "uniqueid" | "tmdbid" | "tmdb_id" | "imdbid" | "imdb_id" | "tvdbid" | "tvdb_id"
            | "id" => unique_id_count = unique_id_count.saturating_add(1),
            "rating" | "communityrating" | "criticrating" => {
                rating_count = rating_count.saturating_add(1)
            }
            "namedseason" | "seasonplot" => {
                named_season_count = named_season_count.saturating_add(1)
            }
            "thumb" | "fanart" | "logo" | "clearlogo" => {
                image_count = image_count.saturating_add(1)
            }
            "genre" | "country" | "studio" | "tag" | "style" | "trailer" | "airs_dayofweek"
            | "lockedfield" | "lockedfields" => {
                multi_value_count = multi_value_count
                    .saturating_add(node.text().map(multi_value_item_count).unwrap_or(0));
            }
            _ if node.parent().is_some_and(|parent| is_named(parent, "art")) => {
                image_count = image_count.saturating_add(1)
            }
            _ => {}
        }

        if actor_count > MAX_NFO_ACTORS {
            return resource_limit_exceeded("actors", MAX_NFO_ACTORS);
        }
        if credit_count > MAX_NFO_CREDITS {
            return resource_limit_exceeded("credits", MAX_NFO_CREDITS);
        }
        if image_count > MAX_NFO_IMAGES {
            return resource_limit_exceeded("images", MAX_NFO_IMAGES);
        }
        if unique_id_count > MAX_NFO_UNIQUE_IDS {
            return resource_limit_exceeded("unique_ids", MAX_NFO_UNIQUE_IDS);
        }
        if rating_count > MAX_NFO_RATINGS {
            return resource_limit_exceeded("ratings", MAX_NFO_RATINGS);
        }
        if named_season_count > MAX_NFO_NAMED_SEASONS {
            return resource_limit_exceeded("named_seasons", MAX_NFO_NAMED_SEASONS);
        }
        if multi_value_count > MAX_NFO_MULTI_VALUE_ITEMS {
            return resource_limit_exceeded("multi_value_items", MAX_NFO_MULTI_VALUE_ITEMS);
        }
    }

    Ok(())
}

fn multi_value_item_count(value: &str) -> usize {
    value
        .split(['/', ',', '|'])
        .filter(|part| !part.trim().is_empty())
        .count()
}

fn slash_list_item_count(value: &str) -> usize {
    value
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .count()
}

fn resource_limit_exceeded<T>(metric: &str, limit: usize) -> Result<T, LocalNfoErrorCode> {
    tracing::warn!(
        metric,
        limit,
        "sidecar nfo exceeds a structured resource limit"
    );
    Err(LocalNfoErrorCode::ResourceLimitExceeded)
}

fn is_named(node: Node<'_, '_>, expected: &str) -> bool {
    node.is_element() && node.tag_name().name().eq_ignore_ascii_case(expected)
}

fn node_text(node: Node<'_, '_>) -> Option<String> {
    node.text().and_then(normalize_xml_text)
}

fn first_child_text(node: Node<'_, '_>, names: &[&str]) -> Option<String> {
    for name in names {
        if let Some(value) = node
            .children()
            .find(|child| is_named(*child, name))
            .and_then(node_text)
        {
            return Some(value);
        }
    }
    None
}

fn child_texts(node: Node<'_, '_>, names: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    for child in node.children().filter(Node::is_element) {
        if names.iter().any(|name| is_named(child, name)) {
            if let Some(value) = node_text(child) {
                values.push(value);
            }
        }
    }
    deduplicate_case_insensitive(&mut values);
    values
}

fn child_texts_split(node: Node<'_, '_>, names: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    for value in child_texts(node, names) {
        for part in value.split('/') {
            if let Some(part) = normalize_xml_text(part) {
                values.push(part);
            }
        }
    }
    deduplicate_case_insensitive(&mut values);
    values
}

fn parse_named_seasons(root: Node<'_, '_>, base_dir: &Path) -> Vec<LocalNfoNamedSeason> {
    let mut seasons = BTreeMap::<i32, LocalNfoNamedSeason>::new();
    for node in root
        .children()
        .filter(|node| is_named(*node, "namedseason"))
    {
        let Some(season_number) = attribute_ignore_ascii_case(node, "number")
            .and_then(|value| value.parse::<i32>().ok())
            .filter(|value| *value >= 0)
        else {
            continue;
        };
        let Some(title) = node_text(node) else {
            continue;
        };
        season_metadata_entry(&mut seasons, season_number).title = Some(title);
    }

    for node in root.children().filter(|node| is_named(*node, "seasonplot")) {
        let Some(season_number) = attribute_ignore_ascii_case(node, "number")
            .and_then(|value| value.parse::<i32>().ok())
            .filter(|value| *value >= 0)
        else {
            continue;
        };
        let Some(overview) = node_text(node) else {
            continue;
        };
        season_metadata_entry(&mut seasons, season_number).overview = Some(overview);
    }

    for node in root.children().filter(|node| is_named(*node, "thumb")) {
        if !attribute_ignore_ascii_case(node, "type")
            .is_some_and(|value| value.eq_ignore_ascii_case("season"))
        {
            continue;
        }
        let Some(season_number) = attribute_ignore_ascii_case(node, "season")
            .and_then(|value| value.parse::<i32>().ok())
            .filter(|value| *value >= 0)
        else {
            continue;
        };
        let Some(value) = node_text(node) else {
            continue;
        };
        let aspect =
            attribute_ignore_ascii_case(node, "aspect").unwrap_or_else(|| "poster".to_string());
        add_artwork_reference(
            &mut season_metadata_entry(&mut seasons, season_number).artwork,
            &aspect,
            &value,
            base_dir,
        );
    }

    seasons.into_values().collect()
}

fn season_metadata_entry(
    seasons: &mut BTreeMap<i32, LocalNfoNamedSeason>,
    season_number: i32,
) -> &mut LocalNfoNamedSeason {
    seasons
        .entry(season_number)
        .or_insert_with(|| LocalNfoNamedSeason {
            season_number,
            title: None,
            overview: None,
            artwork: LocalNfoArtwork::default(),
        })
}

fn attribute_ignore_ascii_case(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.attributes()
        .find(|attribute| attribute.name().eq_ignore_ascii_case(name))
        .map(|attribute| attribute.value().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_unique_ids(node: Node<'_, '_>) -> Vec<LocalNfoUniqueId> {
    let mut ids = Vec::new();
    for child in node.children().filter(|child| is_named(*child, "uniqueid")) {
        let Some(value) = node_text(child) else {
            continue;
        };
        let Some(provider) = attribute_ignore_ascii_case(child, "type")
            .or_else(|| attribute_ignore_ascii_case(child, "provider"))
            .map(|provider| normalize_id_provider(&provider))
            .filter(|provider| !provider.is_empty())
        else {
            continue;
        };
        push_unique_id(
            &mut ids,
            LocalNfoUniqueId {
                provider,
                value,
                is_default: attribute_ignore_ascii_case(child, "default")
                    .is_some_and(|value| parse_bool(&value)),
            },
        );
    }
    ids
}

fn append_legacy_unique_ids(node: Node<'_, '_>, ids: &mut Vec<LocalNfoUniqueId>) {
    for (tag, provider) in [
        ("tmdbid", "tmdb"),
        ("tmdb_id", "tmdb"),
        ("imdbid", "imdb"),
        ("imdb_id", "imdb"),
        ("tvdbid", "tvdb"),
        ("tvdb_id", "tvdb"),
    ] {
        if let Some(value) = first_child_text(node, &[tag]) {
            push_unique_id(
                ids,
                LocalNfoUniqueId {
                    provider: provider.to_string(),
                    value,
                    is_default: false,
                },
            );
        }
    }
}

fn append_legacy_id_element(node: Node<'_, '_>, ids: &mut Vec<LocalNfoUniqueId>) {
    let Some(id_node) = node.children().find(|child| is_named(*child, "id")) else {
        return;
    };

    for (attribute, provider) in [("tmdb", "tmdb"), ("tvdb", "tvdb"), ("imdb", "imdb")] {
        if let Some(value) = attribute_ignore_ascii_case(id_node, attribute) {
            push_unique_id(
                ids,
                LocalNfoUniqueId {
                    provider: provider.to_string(),
                    value,
                    is_default: false,
                },
            );
        }
    }

    if let Some(value) = node_text(id_node).filter(|value| {
        value
            .get(..2)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("tt"))
            && value.get(2..).is_some_and(|suffix| {
                !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
            })
    }) {
        push_unique_id(
            ids,
            LocalNfoUniqueId {
                provider: "imdb".to_string(),
                value,
                is_default: false,
            },
        );
    }
}

fn parse_episode_guide_ids(root: Node<'_, '_>) -> Result<Vec<LocalNfoUniqueId>, LocalNfoErrorCode> {
    let Some(value) = first_child_text(root, &["episodeguide"]) else {
        return Ok(Vec::new());
    };
    let Ok(serde_json::Value::Object(values)) = serde_json::from_str(&value) else {
        return Ok(Vec::new());
    };
    if values.len() > MAX_NFO_UNIQUE_IDS {
        return resource_limit_exceeded("episode_guide_ids", MAX_NFO_UNIQUE_IDS);
    }

    let mut ids = Vec::new();
    for (provider, value) in values {
        let value = match value {
            serde_json::Value::String(value) => value,
            serde_json::Value::Number(value) => value.to_string(),
            _ => continue,
        };
        let provider = normalize_id_provider(&provider);
        let value = value.trim();
        if provider.is_empty() || value.is_empty() {
            continue;
        }
        push_unique_id(
            &mut ids,
            LocalNfoUniqueId {
                provider,
                value: value.to_string(),
                is_default: false,
            },
        );
    }
    deduplicate_unique_ids(&mut ids);
    Ok(ids)
}

fn normalize_id_provider(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "themoviedb" | "themoviedb.org" => "tmdb".to_string(),
        "thetvdb" | "thetvdb.com" => "tvdb".to_string(),
        normalized => normalized.to_string(),
    }
}

fn push_unique_id(ids: &mut Vec<LocalNfoUniqueId>, candidate: LocalNfoUniqueId) {
    ids.push(candidate);
}

fn deduplicate_unique_ids(ids: &mut Vec<LocalNfoUniqueId>) {
    let mut seen = HashSet::with_capacity(ids.len());
    ids.retain(|id| seen.insert((id.provider.to_ascii_lowercase(), id.value.clone())));
}

fn parse_ratings(root: Node<'_, '_>) -> Vec<LocalNfoRating> {
    let mut ratings = Vec::new();
    if let Some(container) = root.children().find(|node| is_named(*node, "ratings")) {
        for rating in container
            .children()
            .filter(|node| is_named(*node, "rating"))
        {
            let scale = match attribute_ignore_ascii_case(rating, "max") {
                Some(value) => match parse_positive_finite_number(&value) {
                    Some(value) => value,
                    None => continue,
                },
                None => 10.0,
            };
            let Some(value) = first_child_text(rating, &["value"])
                .and_then(|value| parse_rating_number(&value, scale))
            else {
                continue;
            };
            ratings.push(LocalNfoRating {
                source: attribute_ignore_ascii_case(rating, "name")
                    .map(|source| normalize_id_provider(&source))
                    .unwrap_or_else(|| "default".to_string()),
                kind: attribute_ignore_ascii_case(rating, "name")
                    .as_deref()
                    .map(classify_structured_rating_kind)
                    .unwrap_or(LocalNfoRatingKind::Audience),
                value,
                scale,
                votes: first_child_text(rating, &["votes"]).and_then(|value| parse_votes(&value)),
                is_default: attribute_ignore_ascii_case(rating, "default")
                    .is_some_and(|value| parse_bool(&value)),
            });
        }
    }

    if let Some(value) =
        first_child_text(root, &["rating"]).and_then(|value| parse_rating_number(&value, 10.0))
    {
        ratings.push(LocalNfoRating {
            source: "default".to_string(),
            kind: LocalNfoRatingKind::Audience,
            value,
            scale: 10.0,
            votes: first_child_text(root, &["votes"]).and_then(|value| parse_votes(&value)),
            is_default: true,
        });
    }
    if let Some(value) = first_child_text(root, &["communityrating"])
        .and_then(|value| parse_rating_number(&value, 10.0))
    {
        ratings.push(LocalNfoRating {
            source: "community".to_string(),
            kind: LocalNfoRatingKind::Audience,
            value,
            scale: 10.0,
            votes: None,
            is_default: ratings.is_empty(),
        });
    }
    if let Some(value) = first_child_text(root, &["criticrating"])
        .and_then(|value| parse_rating_number(&value, 100.0))
    {
        ratings.push(LocalNfoRating {
            source: "default".to_string(),
            kind: LocalNfoRatingKind::Critic,
            value,
            scale: 100.0,
            votes: None,
            is_default: false,
        });
    }

    ratings
}

fn classify_structured_rating_kind(source: &str) -> LocalNfoRatingKind {
    let source = source.trim().to_ascii_lowercase();
    if source == "metacritic"
        || source.contains("critic")
        || (source.contains("tomato") && !source.contains("audience"))
    {
        LocalNfoRatingKind::Critic
    } else {
        LocalNfoRatingKind::Audience
    }
}

fn parse_positive_finite_number(value: &str) -> Option<f64> {
    value
        .trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn parse_rating_number(value: &str, scale: f64) -> Option<f64> {
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    value
        .trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && (0.0..=scale).contains(value))
}

fn parse_votes(value: &str) -> Option<u64> {
    value
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>()
        .parse::<u64>()
        .ok()
}

fn parse_credits(root: Node<'_, '_>, base_dir: &Path) -> LocalNfoCredits {
    let mut credits = LocalNfoCredits {
        directors: child_texts(root, &["director"]),
        writers: child_texts_split(root, &["credits", "writer"]),
        clear_actors: root.children().any(|node| {
            is_named(node, "actor")
                && attribute_ignore_ascii_case(node, "clear")
                    .is_some_and(|value| parse_bool(&value))
        }),
        ..LocalNfoCredits::default()
    };

    for actor in root.children().filter(|node| is_named(*node, "actor")) {
        let Some(name) = first_child_text(actor, &["name"]) else {
            continue;
        };
        let thumb_path = first_child_text(actor, &["thumb"])
            .as_deref()
            .and_then(|value| resolve_sidecar_reference(value, base_dir));
        credits.actors.push(LocalNfoActor {
            name,
            role: first_child_text(actor, &["role"]),
            order: first_child_text(actor, &["order"]).and_then(|value| value.parse::<u32>().ok()),
            thumb_path,
            person_type: first_child_text(actor, &["type"]),
            unique_ids: {
                let mut ids = parse_unique_ids(actor);
                append_legacy_unique_ids(actor, &mut ids);
                deduplicate_unique_ids(&mut ids);
                ids
            },
            profile: first_child_text(actor, &["profile", "biography"]),
        });
    }

    credits
}

fn parse_artwork(root: Node<'_, '_>, kind: LocalNfoKind, base_dir: &Path) -> LocalNfoArtwork {
    let mut artwork = LocalNfoArtwork::default();

    for child in root.children().filter(Node::is_element) {
        if is_named(child, "thumb") {
            if attribute_ignore_ascii_case(child, "type")
                .is_some_and(|value| value.eq_ignore_ascii_case("season"))
            {
                continue;
            }
            let aspect =
                attribute_ignore_ascii_case(child, "aspect").unwrap_or_else(|| match kind {
                    LocalNfoKind::Episode => "thumb".to_string(),
                    LocalNfoKind::Movie | LocalNfoKind::TvShow => "poster".to_string(),
                });
            if let Some(value) = node_text(child) {
                add_artwork_reference(&mut artwork, &aspect, &value, base_dir);
            }
        } else if is_named(child, "fanart") {
            let nested = child
                .children()
                .filter(|node| is_named(*node, "thumb"))
                .filter_map(node_text)
                .collect::<Vec<_>>();
            if nested.is_empty() {
                if let Some(value) = node_text(child) {
                    add_artwork_reference(&mut artwork, "fanart", &value, base_dir);
                }
            } else {
                for value in nested {
                    add_artwork_reference(&mut artwork, "fanart", &value, base_dir);
                }
            }
        } else if is_named(child, "logo") || is_named(child, "clearlogo") {
            if let Some(value) = node_text(child) {
                add_artwork_reference(&mut artwork, child.tag_name().name(), &value, base_dir);
            }
        } else if is_named(child, "art") {
            for image in child.children().filter(Node::is_element) {
                if let Some(value) = node_text(image) {
                    add_artwork_reference(&mut artwork, image.tag_name().name(), &value, base_dir);
                }
            }
        }
    }

    artwork
}

fn add_artwork_reference(
    artwork: &mut LocalNfoArtwork,
    aspect: &str,
    value: &str,
    base_dir: &Path,
) {
    let Some(reference) = resolve_sidecar_reference(value, base_dir) else {
        return;
    };
    let kind = match aspect.trim().to_ascii_lowercase().as_str() {
        "poster" | "cover" | "folder" => LocalNfoImageKind::Poster,
        "fanart" | "backdrop" | "background" => LocalNfoImageKind::Backdrop,
        "logo" | "clearlogo" => LocalNfoImageKind::Logo,
        "banner" => LocalNfoImageKind::Banner,
        "landscape" => LocalNfoImageKind::Landscape,
        "clearart" => LocalNfoImageKind::ClearArt,
        "discart" | "disc" => LocalNfoImageKind::DiscArt,
        "keyart" => LocalNfoImageKind::KeyArt,
        _ => LocalNfoImageKind::Thumbnail,
    };
    match kind {
        LocalNfoImageKind::Poster => push_unique(&mut artwork.posters, reference.clone()),
        LocalNfoImageKind::Backdrop => push_unique(&mut artwork.backdrops, reference.clone()),
        LocalNfoImageKind::Logo => push_unique(&mut artwork.logos, reference.clone()),
        LocalNfoImageKind::Banner
        | LocalNfoImageKind::Landscape
        | LocalNfoImageKind::ClearArt
        | LocalNfoImageKind::DiscArt
        | LocalNfoImageKind::KeyArt
        | LocalNfoImageKind::Thumbnail => {
            push_unique(&mut artwork.thumbnails, reference.clone());
        }
    }
    if !artwork
        .images
        .iter()
        .any(|image| image.kind == kind && image.path == reference)
    {
        artwork.images.push(LocalNfoImage {
            kind,
            path: reference,
        });
    }
}

fn parse_collection(root: Node<'_, '_>) -> Option<LocalNfoCollection> {
    let set = root.children().find(|node| is_named(*node, "set"))?;
    let has_elements = set.children().any(|node| node.is_element());
    let name = if has_elements {
        first_child_text(set, &["name", "title"])
    } else {
        node_text(set)
    };
    let overview = first_child_text(set, &["overview", "plot"]);
    let mut unique_ids = parse_unique_ids(set);
    append_legacy_unique_ids(set, &mut unique_ids);
    if let Some(value) = attribute_ignore_ascii_case(set, "tmdbcolid") {
        push_unique_id(
            &mut unique_ids,
            LocalNfoUniqueId {
                provider: "tmdb_collection".to_string(),
                value,
                is_default: false,
            },
        );
    }
    deduplicate_unique_ids(&mut unique_ids);

    (name.is_some() || overview.is_some() || !unique_ids.is_empty()).then_some(LocalNfoCollection {
        name,
        overview,
        unique_ids,
    })
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "yes" | "1"
    )
}

fn parse_positive_u32(value: &str) -> Option<u32> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= u32::MAX as f64)
        .map(|value| value.round() as u32)
}

fn split_list_value(value: &str) -> Vec<String> {
    value
        .split([',', '|'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn push_unique(values: &mut Vec<String>, candidate: String) {
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&candidate))
    {
        values.push(candidate);
    }
}

fn deduplicate_case_insensitive(values: &mut Vec<String>) {
    let mut seen = HashSet::with_capacity(values.len());
    values.retain(|value| seen.insert(value.to_ascii_lowercase()));
}

fn normalize_xml_text(value: &str) -> Option<String> {
    // roxmltree already resolves XML entities and exposes CDATA as text. A
    // second decoding pass would corrupt intentional text such as `&amp;`.
    let normalized = value.trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

fn resolve_sidecar_reference(value: &str, base_dir: &Path) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if is_external_url(value) {
        return value
            .starts_with("https://image.tmdb.org/t/p/")
            .then(|| value.to_string());
    }

    let canonical_base = fs::canonicalize(base_dir).ok()?;
    let reference = Path::new(value);
    let resolved = if reference.is_absolute() {
        reference.to_path_buf()
    } else {
        base_dir.join(reference)
    };
    let canonical_reference = fs::canonicalize(resolved).ok()?;

    if !canonical_reference.starts_with(&canonical_base)
        || !is_supported_sidecar_image(&canonical_reference)
    {
        return None;
    }

    Some(canonical_reference.to_string_lossy().to_string())
}

fn is_supported_sidecar_image(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if !matches!(
        extension.as_deref(),
        Some("jpg" | "jpeg" | "png" | "webp" | "gif" | "avif")
    ) {
        return false;
    }

    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() == 0 {
        return false;
    }

    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 16];
    let Ok(read) = file.read(&mut header) else {
        return false;
    };
    let header = &header[..read];

    match extension.as_deref() {
        Some("jpg" | "jpeg") => header.starts_with(&[0xff, 0xd8, 0xff]),
        Some("png") => header.starts_with(b"\x89PNG\r\n\x1a\n"),
        Some("webp") => {
            header.len() >= 12 && header.starts_with(b"RIFF") && &header[8..12] == b"WEBP"
        }
        Some("gif") => header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a"),
        Some("avif") => {
            header.len() >= 12
                && &header[4..8] == b"ftyp"
                && matches!(&header[8..12], b"avif" | b"avis")
        }
        _ => false,
    }
}

pub(crate) fn find_local_artwork(video_path: &Path, kind: ArtworkKind) -> Option<String> {
    find_local_artwork_with_scope(video_path, kind, ArtworkScope::FileSpecific)
        .or_else(|| find_local_artwork_with_scope(video_path, kind, ArtworkScope::Generic))
}

/// Resolve an episode thumbnail by the exact video stem. The second candidate
/// is a narrowly-scoped compatibility alias used by some media organizers; it
/// deliberately does not perform prefix or season/episode-only matching.
pub(crate) fn find_local_episode_thumbnail(video_path: &Path) -> Option<String> {
    let parent = video_path.parent()?;
    let stem = video_path.file_stem()?.to_str()?;
    find_local_artwork_in_directory(
        parent,
        &[format!("{stem}-thumb"), format!("{stem} - thumb")],
    )
}

/// Resolve season artwork without searching beyond the immediate media
/// container. Numbered artwork is safe in a flat series directory. Generic
/// names are accepted only when the direct parent is an explicit matching
/// season directory.
pub(crate) fn find_local_season_artwork(
    video_path: &Path,
    season_number: i32,
    kind: ArtworkKind,
) -> Option<String> {
    if season_number < 0 || !matches!(kind, ArtworkKind::Poster) {
        return None;
    }

    let parent = video_path.parent()?;
    let explicit_parent_season = explicit_season_directory_number(parent);
    if explicit_parent_season.is_some_and(|parent_season| parent_season != season_number) {
        return None;
    }

    let padded = format!("{season_number:02}");
    let plain = season_number.to_string();
    let mut numbered_names = vec![
        format!("season{padded}-poster"),
        format!("season{plain}-poster"),
    ];
    numbered_names.dedup();

    if let Some(path) = find_local_artwork_in_directory(parent, &numbered_names) {
        return Some(path);
    }

    if explicit_parent_season != Some(season_number) {
        return None;
    }

    let generic_names = ["season-poster".to_string(), "poster".to_string()];
    find_local_artwork_in_directory(parent, &generic_names)
}

/// Resolve generic series artwork only from the video's direct directory.
/// An explicit season directory is never crossed: its parent has not been
/// proven to be the series container at this layer.
pub(crate) fn find_local_series_artwork(video_path: &Path, kind: ArtworkKind) -> Option<String> {
    let parent = video_path.parent()?;
    if explicit_season_directory_number(parent).is_some() {
        return None;
    }
    let names = match kind {
        ArtworkKind::Poster => vec!["poster", "folder", "cover"],
        ArtworkKind::Backdrop => vec!["fanart", "backdrop", "background"],
    };
    let names = names
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    find_local_artwork_in_directory(parent, &names)
}

pub(crate) fn find_local_artwork_with_scope(
    video_path: &Path,
    kind: ArtworkKind,
    scope: ArtworkScope,
) -> Option<String> {
    let parent = video_path.parent()?;
    let stem = video_path.file_stem()?.to_str()?;

    let name_candidates = match (kind, scope) {
        (ArtworkKind::Poster, ArtworkScope::FileSpecific) => {
            vec![
                stem.to_string(),
                format!("{stem}-poster"),
                format!("{stem}.poster"),
            ]
        }
        (ArtworkKind::Poster, ArtworkScope::Generic) => {
            vec![
                "poster".to_string(),
                "folder".to_string(),
                "cover".to_string(),
            ]
        }
        (ArtworkKind::Backdrop, ArtworkScope::FileSpecific) => vec![
            format!("{stem}-fanart"),
            format!("{stem}-backdrop"),
            format!("{stem}-background"),
        ],
        (ArtworkKind::Backdrop, ArtworkScope::Generic) => vec![
            "fanart".to_string(),
            "backdrop".to_string(),
            "background".to_string(),
        ],
    };

    find_local_artwork_in_directory(parent, &name_candidates)
}

fn find_local_artwork_in_directory(directory: &Path, name_candidates: &[String]) -> Option<String> {
    const IMAGE_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "avif"];

    for name in name_candidates {
        for extension in IMAGE_EXTENSIONS {
            let candidate = directory.join(format!("{name}.{extension}"));
            if is_non_empty_file(&candidate) {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }

    None
}

fn explicit_season_directory_number(directory: &Path) -> Option<i32> {
    let name = directory.file_name()?.to_str()?.trim();
    let normalized = name
        .replace(['.', '_', '-', '—', '–'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let normalized_lower = normalized.to_ascii_lowercase();
    let compact = normalized_lower.replace(' ', "");

    let digits = normalized_lower
        .strip_prefix("season ")
        .filter(|suffix| !suffix.contains(' '))
        .or_else(|| compact.strip_prefix('s'))
        .or_else(|| compact.strip_prefix('第')?.strip_suffix('季'))?;
    if digits.is_empty() || digits.len() > 3 || !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    digits.parse::<i32>().ok()
}

fn is_non_empty_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

fn is_external_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::{
        observe_nfo_file, parse_nfo_metadata, read_series_sidecar_metadata, read_sidecar_metadata,
        LocalNfoErrorCode, LocalNfoKind, LocalNfoObservation, MAX_MEDIA_NFO_BYTES, MAX_NFO_ACTORS,
        MAX_TVSHOW_NFO_BYTES,
    };
    use std::{env, fs, path::PathBuf};
    use uuid::Uuid;

    fn unique_temp_path(kind: &str) -> PathBuf {
        env::temp_dir().join(format!("mova-scan-sidecar-{kind}-{}", Uuid::new_v4()))
    }

    fn create_sparse_file(path: &std::path::Path, bytes: usize) {
        let file = fs::File::create(path).expect("oversized nfo should be created");
        file.set_len(bytes as u64)
            .expect("oversized nfo length should be set");
    }

    #[test]
    fn oversized_movie_nfo_safely_falls_back_to_empty_metadata() {
        let root = unique_temp_path("oversized-movie");
        let video_path = root.join("Movie.2026.mkv");
        fs::create_dir_all(&root).unwrap();
        create_sparse_file(&root.join("movie.nfo"), MAX_MEDIA_NFO_BYTES + 1);

        let metadata = read_sidecar_metadata(&video_path);

        let _ = fs::remove_dir_all(&root);
        assert_eq!(metadata, None);
    }

    #[test]
    fn oversized_episode_nfo_safely_falls_back_to_empty_metadata() {
        let root = unique_temp_path("oversized-episode");
        let video_path = root.join("Series.S01E01.mkv");
        fs::create_dir_all(&root).unwrap();
        create_sparse_file(&video_path.with_extension("nfo"), MAX_MEDIA_NFO_BYTES + 1);

        let metadata = read_sidecar_metadata(&video_path);

        let _ = fs::remove_dir_all(&root);
        assert_eq!(metadata, None);
    }

    #[test]
    fn oversized_tvshow_nfo_safely_falls_back_to_empty_metadata() {
        let root = unique_temp_path("oversized-tvshow");
        let video_path = root.join("Season 01").join("Series.S01E01.mkv");
        fs::create_dir_all(video_path.parent().unwrap()).unwrap();
        create_sparse_file(&root.join("tvshow.nfo"), MAX_TVSHOW_NFO_BYTES + 1);

        let metadata = read_series_sidecar_metadata(&video_path);

        let _ = fs::remove_dir_all(&root);
        assert_eq!(metadata, None);
    }

    #[test]
    fn invalid_utf8_nfo_safely_falls_back_to_empty_metadata() {
        let root = unique_temp_path("invalid-utf8");
        let video_path = root.join("Movie.2026.mkv");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("movie.nfo"), [0xff, 0xfe, 0xfd]).unwrap();

        let metadata = read_sidecar_metadata(&video_path);

        let _ = fs::remove_dir_all(&root);
        assert_eq!(metadata, None);
    }

    #[test]
    fn xml_entities_are_decoded_exactly_once() {
        let metadata = parse_nfo_metadata(
            "<movie><title>Rock &amp;amp; Roll</title><plot><![CDATA[A & B]]></plot></movie>",
            std::path::Path::new("/media"),
        )
        .expect("valid NFO should parse");

        assert_eq!(metadata.title.as_deref(), Some("Rock &amp; Roll"));
        assert_eq!(metadata.overview.as_deref(), Some("A & B"));
    }

    #[test]
    fn large_actor_collection_is_preserved_without_truncation() {
        const ACTOR_COUNT: usize = 2_048;
        let root = unique_temp_path("large-actor-collection");
        let nfo_path = root.join("movie.nfo");
        fs::create_dir_all(&root).unwrap();
        let actors = (0..ACTOR_COUNT)
            .map(|index| format!("<actor><name>Actor {index}</name></actor>"))
            .collect::<String>();
        fs::write(
            &nfo_path,
            format!("<movie><title>Cast</title>{actors}</movie>"),
        )
        .unwrap();

        let observation = observe_nfo_file(&nfo_path, LocalNfoKind::Movie);

        let _ = fs::remove_dir_all(&root);
        let LocalNfoObservation::Valid(metadata) = observation else {
            panic!("large valid actor collection should be accepted");
        };
        assert_eq!(metadata.credits.actors.len(), ACTOR_COUNT);
        assert_eq!(
            metadata
                .credits
                .actors
                .last()
                .map(|actor| actor.name.as_str()),
            Some("Actor 2047")
        );
    }

    #[test]
    fn actor_collection_over_limit_rejects_the_entire_nfo() {
        let root = unique_temp_path("actor-resource-limit");
        let nfo_path = root.join("movie.nfo");
        fs::create_dir_all(&root).unwrap();
        let actors = (0..=MAX_NFO_ACTORS)
            .map(|index| format!("<actor><name>Actor {index}</name></actor>"))
            .collect::<String>();
        fs::write(
            &nfo_path,
            format!("<movie><title>Cast</title>{actors}</movie>"),
        )
        .unwrap();

        let observation = observe_nfo_file(&nfo_path, LocalNfoKind::Movie);

        let _ = fs::remove_dir_all(&root);
        assert!(matches!(
            observation,
            LocalNfoObservation::Invalid {
                error_code: LocalNfoErrorCode::ResourceLimitExceeded,
                ..
            }
        ));
    }
}
