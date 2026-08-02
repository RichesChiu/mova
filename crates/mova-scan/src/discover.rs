use crate::{
    parse::{
        episode_identity_for_path, extension_lowercase, parse_media_metadata_with_sidecar,
        parse_media_metadata_without_sidecar,
    },
    probe::{probe_media_file_with_cancel, MediaProbe, ProbeAvailability},
    subtitle::{
        discover_subtitle_tracks, discover_subtitle_tracks_with_index, SubtitleDirectoryIndex,
    },
    DiscoveredAudioTrack, DiscoveredMediaFile, DiscoveredMediaFileInventory, LocalNfoMetadata,
    LocalNfoObservation, MediaNfoKind,
};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{self, ErrorKind},
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

/// 递归扫描目录，找出当前支持的视频文件。
pub fn discover_media_files(root_path: &Path) -> io::Result<Vec<DiscoveredMediaFile>> {
    discover_media_files_with_progress(root_path, |_| {})
}

/// 递归扫描目录，返回当前支持的视频文件路径，不做 sidecar 解析和 ffprobe 探测。
pub fn discover_media_paths(root_path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut boundary = DiscoveryBoundary::new(root_path)?;
    visit_dir_paths(root_path, &mut files, &mut boundary)?;
    files.sort();
    Ok(files)
}

/// 递归扫描目录，并在每发现一个支持的视频文件时回调当前进度。
pub fn discover_media_files_with_progress<F>(
    root_path: &Path,
    mut on_progress: F,
) -> io::Result<Vec<DiscoveredMediaFile>>
where
    F: FnMut(usize),
{
    discover_media_files_with_progress_and_cancel(root_path, &mut on_progress, || false)
}

/// 递归扫描目录，支持进度回调和外部取消信号。
pub fn discover_media_files_with_progress_and_cancel<F, C>(
    root_path: &Path,
    mut on_progress: F,
    mut should_cancel: C,
) -> io::Result<Vec<DiscoveredMediaFile>>
where
    F: FnMut(usize),
    C: FnMut() -> bool,
{
    discover_media_files_with_progress_item_and_cancel(
        root_path,
        &mut on_progress,
        |_| {},
        &mut should_cancel,
    )
}

/// 递归扫描目录，只返回支持的视频文件清单，不做 ffprobe 探测。
pub fn discover_media_file_inventory_with_progress_and_cancel<F, C>(
    root_path: &Path,
    mut on_progress: F,
    mut should_cancel: C,
) -> io::Result<Vec<DiscoveredMediaFileInventory>>
where
    F: FnMut(usize),
    C: FnMut() -> bool,
{
    let mut files = Vec::new();
    let mut boundary = DiscoveryBoundary::new(root_path)?;
    visit_dir_inventory(
        root_path,
        &mut files,
        &mut on_progress,
        &mut should_cancel,
        &mut boundary,
    )?;
    populate_inventory_sidecar_fingerprints(&mut files, root_path);
    files.sort_by(|left, right| left.file_path.cmp(&right.file_path));

    Ok(files)
}

/// 递归扫描目录，支持在发现单个媒体文件时立即回调，便于上层做增量 UI。
pub fn discover_media_files_with_progress_item_and_cancel<F, I, C>(
    root_path: &Path,
    mut on_progress: F,
    mut on_item_discovered: I,
    mut should_cancel: C,
) -> io::Result<Vec<DiscoveredMediaFile>>
where
    F: FnMut(usize),
    I: FnMut(&DiscoveredMediaFile),
    C: FnMut() -> bool,
{
    let mut files = Vec::new();
    let mut probe_availability = ProbeAvailability::Unknown;
    let mut boundary = DiscoveryBoundary::new(root_path)?;
    visit_dir(
        root_path,
        &mut files,
        &mut on_progress,
        &mut on_item_discovered,
        &mut should_cancel,
        &mut probe_availability,
        &mut boundary,
    )?;
    populate_discovered_sidecar_fingerprints(&mut files, root_path);
    files.sort_by(|left, right| left.file_path.cmp(&right.file_path));

    Ok(files)
}

/// 读取单个媒体文件并返回与整库扫描一致的解析结果。
pub fn inspect_media_file(path: &Path) -> io::Result<DiscoveredMediaFile> {
    let inventory = inventory_for_single_media_file(path)?;
    inspect_media_file_inventory(inventory)
}

pub fn inspect_media_file_within_root(
    path: &Path,
    root_path: &Path,
) -> io::Result<DiscoveredMediaFile> {
    inspect_media_file_within_root_and_nfo_policy(path, root_path, true)
}

pub fn inspect_media_file_within_root_and_nfo_policy(
    path: &Path,
    root_path: &Path,
    allow_generic_movie_nfo: bool,
) -> io::Result<DiscoveredMediaFile> {
    let mut inventory = inventory_for_single_media_file(path)?;
    inventory.sidecar_fingerprint =
        sidecar_fingerprint_for_directory(path.parent().unwrap_or(root_path), Some(root_path));
    let mut probe_availability = ProbeAvailability::Unknown;
    build_discovered_media_file_with_cancel(
        inventory,
        &mut probe_availability,
        &mut || false,
        None,
        Some(root_path),
        allow_generic_movie_nfo,
    )
}

/// Reads filename metadata, local NFO and local artwork for one media file
/// without invoking ffprobe or discovering audio/subtitle streams.
///
/// This is intended for sidecar-only metadata refreshes where technical stream
/// facts already stored by a full scan must remain authoritative.
pub fn inspect_media_file_sidecar_only(path: &Path) -> io::Result<DiscoveredMediaFile> {
    let inventory = inventory_for_single_media_file(path)?;
    Ok(build_discovered_media_file_sidecar_only(
        inventory, None, true,
    ))
}

pub fn inspect_media_file_sidecar_only_within_root(
    path: &Path,
    root_path: &Path,
) -> io::Result<DiscoveredMediaFile> {
    inspect_media_file_sidecar_only_within_root_and_nfo_policy(path, root_path, true)
}

pub fn inspect_media_file_sidecar_only_within_root_and_nfo_policy(
    path: &Path,
    root_path: &Path,
    allow_generic_movie_nfo: bool,
) -> io::Result<DiscoveredMediaFile> {
    let mut inventory = inventory_for_single_media_file(path)?;
    inventory.sidecar_fingerprint =
        sidecar_fingerprint_for_directory(path.parent().unwrap_or(root_path), Some(root_path));
    Ok(build_discovered_media_file_sidecar_only(
        inventory,
        Some(root_path),
        allow_generic_movie_nfo,
    ))
}

fn inventory_for_single_media_file(path: &Path) -> io::Result<DiscoveredMediaFileInventory> {
    let metadata = fs::metadata(path)?;

    if !metadata.is_file() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("path is not a regular file: {}", path.display()),
        ));
    }

    if !is_supported_video(path) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("unsupported media file extension: {}", path.display()),
        ));
    }

    let mut inventory = build_discovered_media_file_inventory(
        path.to_path_buf(),
        metadata.len(),
        metadata_modified_at_ms(&metadata),
    );
    inventory.sidecar_fingerprint =
        sidecar_fingerprint_for_directory(path.parent().unwrap_or_else(|| Path::new(".")), None);
    Ok(inventory)
}

/// 对已确认发生新增或变化的视频文件做完整解析和 ffprobe 探测。
pub fn inspect_media_file_inventory(
    inventory: DiscoveredMediaFileInventory,
) -> io::Result<DiscoveredMediaFile> {
    inspect_media_file_inventory_with_cancel(inventory, || false)
}

/// 对已确认发生新增或变化的视频文件做完整解析，并允许调用方中断正在运行的 ffprobe。
pub fn inspect_media_file_inventory_with_cancel<C>(
    inventory: DiscoveredMediaFileInventory,
    mut should_cancel: C,
) -> io::Result<DiscoveredMediaFile>
where
    C: FnMut() -> bool,
{
    let mut probe_availability = ProbeAvailability::Unknown;
    build_discovered_media_file_with_cancel(
        inventory,
        &mut probe_availability,
        &mut should_cancel,
        None,
        None,
        true,
    )
}

/// Fully inspects one inventory entry while reusing a scan-local directory
/// subtitle index.
pub fn inspect_media_file_inventory_with_cancel_and_subtitle_index<C>(
    inventory: DiscoveredMediaFileInventory,
    subtitle_index: &SubtitleDirectoryIndex,
    mut should_cancel: C,
) -> io::Result<DiscoveredMediaFile>
where
    C: FnMut() -> bool,
{
    let mut probe_availability = ProbeAvailability::Unknown;
    build_discovered_media_file_with_cancel(
        inventory,
        &mut probe_availability,
        &mut should_cancel,
        Some(subtitle_index),
        None,
        true,
    )
}

pub fn inspect_media_file_inventory_within_root_with_cancel_and_subtitle_index<C>(
    inventory: DiscoveredMediaFileInventory,
    root_path: &Path,
    subtitle_index: &SubtitleDirectoryIndex,
    mut should_cancel: C,
) -> io::Result<DiscoveredMediaFile>
where
    C: FnMut() -> bool,
{
    let mut probe_availability = ProbeAvailability::Unknown;
    build_discovered_media_file_with_cancel(
        inventory,
        &mut probe_availability,
        &mut should_cancel,
        Some(subtitle_index),
        Some(root_path),
        true,
    )
}

/// Fully inspects one inventory entry while enforcing the scan planner's
/// decision about whether this carrier may consume a directory-level
/// `movie.nfo`. File-specific movie and episode NFO files remain eligible.
pub fn inspect_media_file_inventory_within_root_with_cancel_and_subtitle_index_and_nfo_policy<C>(
    inventory: DiscoveredMediaFileInventory,
    root_path: &Path,
    subtitle_index: &SubtitleDirectoryIndex,
    allow_generic_movie_nfo: bool,
    mut should_cancel: C,
) -> io::Result<DiscoveredMediaFile>
where
    C: FnMut() -> bool,
{
    let mut probe_availability = ProbeAvailability::Unknown;
    build_discovered_media_file_with_cancel(
        inventory,
        &mut probe_availability,
        &mut should_cancel,
        Some(subtitle_index),
        Some(root_path),
        allow_generic_movie_nfo,
    )
}

/// 只做文件名/路径轻量解析，不读取 sidecar、不调用 ffprobe。
pub fn inspect_media_file_inventory_shallow(
    inventory: DiscoveredMediaFileInventory,
) -> io::Result<DiscoveredMediaFile> {
    Ok(build_discovered_media_file_without_probe(inventory))
}

fn visit_dir<F>(
    dir: &Path,
    files: &mut Vec<DiscoveredMediaFile>,
    on_progress: &mut F,
    on_item_discovered: &mut impl FnMut(&DiscoveredMediaFile),
    should_cancel: &mut impl FnMut() -> bool,
    probe_availability: &mut ProbeAvailability,
    boundary: &mut DiscoveryBoundary,
) -> io::Result<()>
where
    F: FnMut(usize),
{
    if should_cancel() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "scan cancelled"));
    }

    if !boundary.enter_directory(dir)? {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        if should_cancel() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "scan cancelled"));
        }

        let entry = entry?;
        let path = entry.path();
        let Some(canonical_path) = boundary.canonical_path_if_allowed(&path)? else {
            continue;
        };
        let metadata = fs::metadata(canonical_path)?;

        if metadata.is_dir() {
            visit_dir(
                &path,
                files,
                on_progress,
                on_item_discovered,
                should_cancel,
                probe_availability,
                boundary,
            )?;
            continue;
        }

        if !metadata.is_file() || !is_supported_video(&path) {
            continue;
        }

        let inventory = build_discovered_media_file_inventory(
            path,
            metadata.len(),
            metadata_modified_at_ms(&metadata),
        );
        files.push(build_discovered_media_file_with_cancel(
            inventory,
            probe_availability,
            should_cancel,
            None,
            None,
            true,
        )?);
        if let Some(file) = files.last() {
            on_item_discovered(file);
        }
        on_progress(files.len());
    }

    Ok(())
}

fn visit_dir_inventory<F>(
    dir: &Path,
    files: &mut Vec<DiscoveredMediaFileInventory>,
    on_progress: &mut F,
    should_cancel: &mut impl FnMut() -> bool,
    boundary: &mut DiscoveryBoundary,
) -> io::Result<()>
where
    F: FnMut(usize),
{
    if should_cancel() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "scan cancelled"));
    }

    if !boundary.enter_directory(dir)? {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        if should_cancel() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "scan cancelled"));
        }

        let entry = entry?;
        let path = entry.path();
        let Some(canonical_path) = boundary.canonical_path_if_allowed(&path)? else {
            continue;
        };
        let metadata = fs::metadata(canonical_path)?;

        if metadata.is_dir() {
            visit_dir_inventory(&path, files, on_progress, should_cancel, boundary)?;
            continue;
        }

        if !metadata.is_file() || !is_supported_video(&path) {
            continue;
        }

        files.push(build_discovered_media_file_inventory(
            path,
            metadata.len(),
            metadata_modified_at_ms(&metadata),
        ));
        on_progress(files.len());
    }

    Ok(())
}

fn visit_dir_paths(
    dir: &Path,
    files: &mut Vec<PathBuf>,
    boundary: &mut DiscoveryBoundary,
) -> io::Result<()> {
    if !boundary.enter_directory(dir)? {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(canonical_path) = boundary.canonical_path_if_allowed(&path)? else {
            continue;
        };
        let metadata = fs::metadata(canonical_path)?;

        if metadata.is_dir() {
            visit_dir_paths(&path, files, boundary)?;
            continue;
        }

        if !metadata.is_file() || !is_supported_video(&path) {
            continue;
        }

        files.push(path);
    }

    Ok(())
}

struct DiscoveryBoundary {
    canonical_root: PathBuf,
    visited_directories: HashSet<PathBuf>,
}

impl DiscoveryBoundary {
    fn new(root_path: &Path) -> io::Result<Self> {
        let canonical_root = fs::canonicalize(root_path)?;
        if !fs::metadata(&canonical_root)?.is_dir() {
            return Err(io::Error::new(
                ErrorKind::NotADirectory,
                format!(
                    "media library root is not a directory: {}",
                    root_path.display()
                ),
            ));
        }

        Ok(Self {
            canonical_root,
            visited_directories: HashSet::new(),
        })
    }

    fn canonical_path_if_allowed(&self, path: &Path) -> io::Result<Option<PathBuf>> {
        let canonical_path = fs::canonicalize(path)?;
        Ok(canonical_path
            .starts_with(&self.canonical_root)
            .then_some(canonical_path))
    }

    fn enter_directory(&mut self, path: &Path) -> io::Result<bool> {
        let Some(canonical_path) = self.canonical_path_if_allowed(path)? else {
            return Ok(false);
        };

        Ok(self.visited_directories.insert(canonical_path))
    }
}

fn build_discovered_media_file_inventory(
    path: PathBuf,
    file_size: u64,
    file_modified_at_ms: Option<i64>,
) -> DiscoveredMediaFileInventory {
    DiscoveredMediaFileInventory {
        file_path: path,
        file_size,
        file_modified_at_ms,
        sidecar_fingerprint: String::new(),
    }
}

fn build_discovered_media_file_with_cancel(
    inventory: DiscoveredMediaFileInventory,
    probe_availability: &mut ProbeAvailability,
    should_cancel: &mut impl FnMut() -> bool,
    subtitle_index: Option<&SubtitleDirectoryIndex>,
    root_path: Option<&Path>,
    allow_generic_movie_nfo: bool,
) -> io::Result<DiscoveredMediaFile> {
    if should_cancel() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "scan cancelled"));
    }

    let path = inventory.file_path.clone();
    let (sidecar, invalid_local_nfo_source_path) =
        observe_file_nfo(&path, root_path, allow_generic_movie_nfo);
    let parsed = parse_media_metadata_with_sidecar(&path, sidecar);
    let probe = probe_media_file_with_cancel(&path, probe_availability, should_cancel)?;

    let mut file =
        build_discovered_media_file_from_parts(inventory, parsed, probe, true, subtitle_index);
    file.invalid_local_nfo_source_path = invalid_local_nfo_source_path;
    Ok(file)
}

fn build_discovered_media_file_without_probe(
    inventory: DiscoveredMediaFileInventory,
) -> DiscoveredMediaFile {
    let path = inventory.file_path.clone();
    let parsed = parse_media_metadata_without_sidecar(&path);

    build_discovered_media_file_from_parts(inventory, parsed, MediaProbe::default(), false, None)
}

fn build_discovered_media_file_sidecar_only(
    inventory: DiscoveredMediaFileInventory,
    root_path: Option<&Path>,
    allow_generic_movie_nfo: bool,
) -> DiscoveredMediaFile {
    let path = inventory.file_path.clone();
    let (sidecar, invalid_local_nfo_source_path) =
        observe_file_nfo(&path, root_path, allow_generic_movie_nfo);
    let parsed = parse_media_metadata_with_sidecar(&path, sidecar);

    let mut file = build_discovered_media_file_from_parts(
        inventory,
        parsed,
        MediaProbe::default(),
        false,
        None,
    );
    file.invalid_local_nfo_source_path = invalid_local_nfo_source_path;
    file
}

fn observe_file_nfo(
    path: &Path,
    root_path: Option<&Path>,
    allow_generic_movie_nfo: bool,
) -> (Option<LocalNfoMetadata>, Option<PathBuf>) {
    let expected_kind = if crate::parse::is_likely_episode_path(path) {
        MediaNfoKind::Episode
    } else {
        MediaNfoKind::Movie
    };
    let observation = match (root_path, expected_kind, allow_generic_movie_nfo) {
        (Some(root_path), MediaNfoKind::Movie, false) => crate::observe_nfo_file_within_root(
            &path.with_extension("nfo"),
            crate::LocalNfoKind::Movie,
            root_path,
        ),
        (Some(root_path), _, _) => {
            crate::observe_media_nfo_for_kind_within_root(path, expected_kind, root_path)
        }
        (None, _, _) => crate::observe_media_nfo_for_kind(path, expected_kind),
    };
    match observation {
        LocalNfoObservation::Valid(metadata) => (Some(*metadata), None),
        LocalNfoObservation::Invalid { candidate_path, .. } => (None, Some(candidate_path)),
        LocalNfoObservation::Absent { .. } => (None, None),
    }
}

fn build_discovered_media_file_from_parts(
    inventory: DiscoveredMediaFileInventory,
    parsed: crate::parse::ParsedMediaMetadata,
    probe: MediaProbe,
    discover_local_sidecars: bool,
    subtitle_index: Option<&SubtitleDirectoryIndex>,
) -> DiscoveredMediaFile {
    let path = inventory.file_path;
    let subtitle_tracks = if discover_local_sidecars {
        match subtitle_index {
            Some(index) => {
                discover_subtitle_tracks_with_index(&path, &probe.subtitle_streams, index)
            }
            None => discover_subtitle_tracks(&path, &probe.subtitle_streams),
        }
    } else {
        Vec::new()
    };

    DiscoveredMediaFile {
        file_modified_at_ms: inventory.file_modified_at_ms,
        sidecar_fingerprint: inventory.sidecar_fingerprint,
        probe_error: probe.error,
        metadata_provider: None,
        metadata_provider_item_id: None,
        title: parsed.title,
        source_title: parsed.source_title,
        original_title: parsed.original_title,
        sort_title: parsed.sort_title,
        tagline: None,
        premiere_date: None,
        content_rating: None,
        series_sidecar_title: None,
        series_sidecar_year: None,
        local_nfo: parsed.local_nfo,
        series_local_nfo: None,
        invalid_local_nfo_source_path: None,
        invalid_series_local_nfo_source_path: None,
        local_nfo_is_selected: false,
        series_local_nfo_is_selected: false,
        removed_local_nfo_source_path: None,
        removed_series_local_nfo_source_path: None,
        year: parsed.year,
        external_ids: Vec::new(),
        ratings: Vec::new(),
        metadata_status: None,
        metadata_failure_reason: None,
        remote_media_type: None,
        country: None,
        genres: None,
        studio: None,
        season_number: parsed.season_number,
        season_title: parsed.season_title,
        season_overview: parsed.season_overview,
        season_poster_path: parsed.season_poster_path,
        season_backdrop_path: parsed.season_backdrop_path,
        episode_number: parsed.episode_number,
        episode_title: parsed.episode_title,
        episode_original_title: None,
        episode_sort_title: None,
        episode_year: None,
        episode_overview: parsed.episode_overview,
        episode_tagline: None,
        episode_premiere_date: None,
        episode_content_rating: None,
        overview: parsed.overview,
        series_poster_path: parsed.series_poster_path,
        series_backdrop_path: parsed.series_backdrop_path,
        series_logo_path: None,
        poster_path: parsed.poster_path,
        backdrop_path: parsed.backdrop_path,
        logo_path: None,
        container: extension_lowercase(&path),
        duration_seconds: probe.duration_seconds,
        video_title: probe.video_title,
        video_codec: probe.video_codec,
        video_profile: probe.video_profile,
        video_level: probe.video_level,
        audio_codec: probe.audio_codec,
        width: probe.width,
        height: probe.height,
        bitrate: probe.bitrate,
        video_bitrate: probe.video_bitrate,
        video_frame_rate: probe.video_frame_rate,
        video_aspect_ratio: probe.video_aspect_ratio,
        video_scan_type: probe.video_scan_type,
        video_color_primaries: probe.video_color_primaries,
        video_color_space: probe.video_color_space,
        video_color_transfer: probe.video_color_transfer,
        video_bit_depth: probe.video_bit_depth,
        video_pixel_format: probe.video_pixel_format,
        video_reference_frames: probe.video_reference_frames,
        technical_tags: probe.technical_tags,
        audio_tracks: probe
            .audio_streams
            .into_iter()
            .map(|audio| DiscoveredAudioTrack {
                stream_index: audio.stream_index,
                language: audio.language,
                audio_codec: audio.audio_codec,
                label: audio.label,
                channel_layout: audio.channel_layout,
                channels: audio.channels,
                bitrate: audio.bitrate,
                sample_rate: audio.sample_rate,
                is_default: audio.is_default,
            })
            .collect(),
        subtitle_tracks,
        file_path: path,
        file_size: inventory.file_size,
    }
}

fn populate_inventory_sidecar_fingerprints(
    files: &mut [DiscoveredMediaFileInventory],
    root_path: &Path,
) {
    let fingerprints = sidecar_fingerprints_by_directory(
        files
            .iter()
            .map(|file| file.file_path.as_path())
            .collect::<Vec<_>>()
            .as_slice(),
        Some(root_path),
    );
    for file in files {
        file.sidecar_fingerprint = file
            .file_path
            .parent()
            .and_then(|parent| fingerprints.get(parent))
            .cloned()
            .unwrap_or_default();
    }
}

fn populate_discovered_sidecar_fingerprints(files: &mut [DiscoveredMediaFile], root_path: &Path) {
    let fingerprints = sidecar_fingerprints_by_directory(
        files
            .iter()
            .map(|file| file.file_path.as_path())
            .collect::<Vec<_>>()
            .as_slice(),
        Some(root_path),
    );
    for file in files {
        file.sidecar_fingerprint = file
            .file_path
            .parent()
            .and_then(|parent| fingerprints.get(parent))
            .cloned()
            .unwrap_or_default();
    }
}

fn sidecar_fingerprints_by_directory(
    video_paths: &[&Path],
    root_path: Option<&Path>,
) -> BTreeMap<PathBuf, String> {
    let directories = video_paths
        .iter()
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect::<HashSet<_>>();
    let mut fingerprints = BTreeMap::new();

    for directory in directories {
        fingerprints.insert(
            directory.clone(),
            sidecar_fingerprint_for_directory(&directory, root_path),
        );
    }

    fingerprints
}

fn sidecar_fingerprint_for_directory(directory: &Path, root_path: Option<&Path>) -> String {
    let mut facts = Vec::new();
    let mut episode_video_counts = BTreeMap::<(i32, i32), usize>::new();

    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            if is_supported_video(&path) && path.is_file() {
                if let Some(identity) = episode_identity_for_path(&path) {
                    *episode_video_counts
                        .entry((identity.season_number, identity.episode_number))
                        .or_default() += 1;
                }
                continue;
            }
            if !is_local_analysis_sidecar(&path) {
                continue;
            }
            if let Ok(metadata) = fs::metadata(&path) {
                if metadata.is_file() {
                    facts.push(sidecar_file_fact(&path, &metadata));
                }
            }
        }
    }

    // External subtitle matching falls back to SxxExx only when exactly one
    // video in the directory has that identity. Include the stable counts so
    // adding or removing a second version invalidates surviving siblings and
    // refreshes their subtitle associations.
    facts.extend(episode_video_counts.into_iter().map(
        |((season_number, episode_number), count)| {
            format!("episode-video-count\0{season_number}\0{episode_number}\0{count}")
        },
    ));

    let series_nfo = match root_path {
        Some(root_path) => directory
            .strip_prefix(root_path)
            .ok()
            .filter(|relative_directory| {
                !relative_directory.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            })
            .and_then(|relative_directory| {
                relative_directory
                    .ancestors()
                    .take(5)
                    .map(|ancestor| root_path.join(ancestor).join("tvshow.nfo"))
                    .find(|path| path.is_file())
            }),
        None => directory
            .ancestors()
            .take(5)
            .map(|ancestor| ancestor.join("tvshow.nfo"))
            .find(|path| path.is_file()),
    };
    if let Some(series_nfo) = series_nfo {
        if let Ok(metadata) = fs::metadata(&series_nfo) {
            facts.push(sidecar_file_fact(&series_nfo, &metadata));
        }
    }

    facts.sort();
    stable_sidecar_fingerprint(&facts)
}

fn is_local_analysis_sidecar(path: &Path) -> bool {
    matches!(
        extension_lowercase(path).as_deref(),
        Some(
            "nfo"
                | "srt"
                | "ass"
                | "ssa"
                | "vtt"
                | "jpg"
                | "jpeg"
                | "png"
                | "webp"
                | "gif"
                | "avif"
        )
    )
}

fn sidecar_file_fact(path: &Path, metadata: &fs::Metadata) -> String {
    format!(
        "{}\0{}\0{}",
        path.to_string_lossy(),
        metadata.len(),
        metadata_modified_at_ms(metadata)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    )
}

fn stable_sidecar_fingerprint(facts: &[String]) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;

    for fact in facts {
        for byte in (fact.len() as u64)
            .to_le_bytes()
            .into_iter()
            .chain(fact.bytes())
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }

    format!("{hash:016x}")
}

fn metadata_modified_at_ms(metadata: &fs::Metadata) -> Option<i64> {
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;

    i64::try_from(duration.as_millis()).ok()
}

pub(crate) fn is_supported_video(path: &Path) -> bool {
    matches!(
        extension_lowercase(path).as_deref(),
        Some("mp4" | "mkv" | "avi" | "mov" | "m4v" | "wmv" | "flv" | "webm" | "mpg" | "mpeg")
    )
}
