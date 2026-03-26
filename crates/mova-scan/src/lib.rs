mod discover;
mod parse;
mod probe;
mod sidecar;

pub use discover::{
    discover_media_files, discover_media_files_with_progress,
    discover_media_files_with_progress_and_cancel, discover_media_paths, inspect_media_file,
};
pub use parse::is_likely_episode_path;

use std::path::PathBuf;

/// 扫描目录时发现的单个视频文件。
#[derive(Debug, Clone)]
pub struct DiscoveredMediaFile {
    pub file_path: PathBuf,
    pub title: String,
    pub source_title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub year: Option<i32>,
    pub season_number: Option<i32>,
    pub season_title: Option<String>,
    pub season_overview: Option<String>,
    pub season_poster_path: Option<String>,
    pub season_backdrop_path: Option<String>,
    pub episode_number: Option<i32>,
    pub episode_title: Option<String>,
    pub overview: Option<String>,
    pub series_poster_path: Option<String>,
    pub series_backdrop_path: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub file_size: u64,
    pub container: Option<String>,
    pub duration_seconds: Option<i32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bitrate: Option<i64>,
}

#[cfg(test)]
mod tests;
