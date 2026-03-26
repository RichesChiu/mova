use crate::{
    parse::{extension_lowercase, parse_media_metadata},
    probe::{probe_media_file, ProbeAvailability},
    subtitle::discover_subtitle_tracks,
    DiscoveredMediaFile,
};
use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
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
    let mut files = Vec::new();
    let mut probe_availability = ProbeAvailability::Unknown;
    visit_dir(
        root_path,
        &mut files,
        &mut on_progress,
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

    let mut probe_availability = ProbeAvailability::Unknown;
    Ok(build_discovered_media_file(
        path.to_path_buf(),
        metadata.len(),
        &mut probe_availability,
    ))
}

fn visit_dir<F>(
    dir: &Path,
    files: &mut Vec<DiscoveredMediaFile>,
    on_progress: &mut F,
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
            visit_dir(&path, files, on_progress, should_cancel, probe_availability)?;
            continue;
        }

        if !metadata.is_file() || !is_supported_video(&path) {
            continue;
        }

        files.push(build_discovered_media_file(
            path,
            metadata.len(),
            probe_availability,
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

fn build_discovered_media_file(
    path: PathBuf,
    file_size: u64,
    probe_availability: &mut ProbeAvailability,
) -> DiscoveredMediaFile {
    let parsed = parse_media_metadata(&path);
    let probe = probe_media_file(&path, probe_availability);

    DiscoveredMediaFile {
        title: parsed.title,
        source_title: parsed.source_title,
        original_title: parsed.original_title,
        sort_title: parsed.sort_title,
        year: parsed.year,
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
        video_codec: probe.video_codec,
        audio_codec: probe.audio_codec,
        width: probe.width,
        height: probe.height,
        bitrate: probe.bitrate,
        subtitle_tracks: discover_subtitle_tracks(&path, &probe.subtitle_streams),
        file_path: path,
        file_size,
    }
}

pub(crate) fn is_supported_video(path: &Path) -> bool {
    matches!(
        extension_lowercase(path).as_deref(),
        Some("mp4" | "mkv" | "avi" | "mov" | "m4v" | "wmv" | "flv" | "webm" | "mpg" | "mpeg")
    )
}
