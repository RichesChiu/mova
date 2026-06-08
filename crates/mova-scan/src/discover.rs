use crate::{
    parse::{extension_lowercase, parse_media_metadata, parse_media_metadata_without_sidecar},
    probe::{probe_media_file, MediaProbe, ProbeAvailability},
    subtitle::discover_subtitle_tracks,
    DiscoveredAudioTrack, DiscoveredMediaFile, DiscoveredMediaFileInventory,
};
use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

/// 递归扫描目录，找出当前支持的视频文件。
pub fn discover_media_files(root_path: &Path) -> io::Result<Vec<DiscoveredMediaFile>> {
    discover_media_files_with_progress(root_path, |_| {})
}

/// 递归扫描目录，返回当前支持的视频文件路径，不做 sidecar 解析和 ffprobe 探测。
pub fn discover_media_paths(root_path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit_dir_paths(root_path, &mut files)?;
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
    visit_dir_inventory(root_path, &mut files, &mut on_progress, &mut should_cancel)?;
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
    visit_dir(
        root_path,
        &mut files,
        &mut on_progress,
        &mut on_item_discovered,
        &mut should_cancel,
        &mut probe_availability,
    )?;
    files.sort_by(|left, right| left.file_path.cmp(&right.file_path));

    Ok(files)
}

/// 读取单个媒体文件并返回与整库扫描一致的解析结果。
pub fn inspect_media_file(path: &Path) -> io::Result<DiscoveredMediaFile> {
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

    inspect_media_file_inventory(build_discovered_media_file_inventory(
        path.to_path_buf(),
        metadata.len(),
        metadata_modified_at_ms(&metadata),
    ))
}

/// 对已确认发生新增或变化的视频文件做完整解析和 ffprobe 探测。
pub fn inspect_media_file_inventory(
    inventory: DiscoveredMediaFileInventory,
) -> io::Result<DiscoveredMediaFile> {
    let mut probe_availability = ProbeAvailability::Unknown;
    Ok(build_discovered_media_file(
        inventory,
        &mut probe_availability,
    ))
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
) -> io::Result<()>
where
    F: FnMut(usize),
{
    if should_cancel() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "scan cancelled"));
    }

    for entry in fs::read_dir(dir)? {
        if should_cancel() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "scan cancelled"));
        }

        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            visit_dir(
                &path,
                files,
                on_progress,
                on_item_discovered,
                should_cancel,
                probe_availability,
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
        files.push(build_discovered_media_file(inventory, probe_availability));
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
) -> io::Result<()>
where
    F: FnMut(usize),
{
    if should_cancel() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "scan cancelled"));
    }

    for entry in fs::read_dir(dir)? {
        if should_cancel() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "scan cancelled"));
        }

        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            visit_dir_inventory(&path, files, on_progress, should_cancel)?;
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

fn visit_dir_paths(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            visit_dir_paths(&path, files)?;
            continue;
        }

        if !metadata.is_file() || !is_supported_video(&path) {
            continue;
        }

        files.push(path);
    }

    Ok(())
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
    }
}

fn build_discovered_media_file(
    inventory: DiscoveredMediaFileInventory,
    probe_availability: &mut ProbeAvailability,
) -> DiscoveredMediaFile {
    let path = inventory.file_path.clone();
    let parsed = parse_media_metadata(&path);
    let probe = probe_media_file(&path, probe_availability);

    build_discovered_media_file_from_parts(inventory, parsed, probe, true)
}

fn build_discovered_media_file_without_probe(
    inventory: DiscoveredMediaFileInventory,
) -> DiscoveredMediaFile {
    let path = inventory.file_path.clone();
    let parsed = parse_media_metadata_without_sidecar(&path);

    build_discovered_media_file_from_parts(inventory, parsed, MediaProbe::default(), false)
}

fn build_discovered_media_file_from_parts(
    inventory: DiscoveredMediaFileInventory,
    parsed: crate::parse::ParsedMediaMetadata,
    probe: MediaProbe,
    discover_sidecar_subtitles: bool,
) -> DiscoveredMediaFile {
    let path = inventory.file_path;
    let subtitle_tracks = if discover_sidecar_subtitles {
        discover_subtitle_tracks(&path, &probe.subtitle_streams)
    } else {
        Vec::new()
    };

    DiscoveredMediaFile {
        file_modified_at_ms: inventory.file_modified_at_ms,
        metadata_provider: None,
        metadata_provider_item_id: None,
        title: parsed.title,
        source_title: parsed.source_title,
        original_title: parsed.original_title,
        sort_title: parsed.sort_title,
        year: parsed.year,
        imdb_rating: None,
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
        overview: parsed.overview,
        series_poster_path: parsed.series_poster_path,
        series_backdrop_path: parsed.series_backdrop_path,
        poster_path: parsed.poster_path,
        backdrop_path: parsed.backdrop_path,
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
