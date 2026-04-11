mod discover;
mod parse;
mod probe;
mod sidecar;
mod subtitle;

pub use discover::{
    discover_media_files, discover_media_files_with_progress,
    discover_media_files_with_progress_and_cancel,
    discover_media_files_with_progress_item_and_cancel, discover_media_paths, inspect_media_file,
};
pub use parse::{infer_series_folder_metadata, is_likely_episode_path, SeriesFolderMetadata};

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSubtitleTrack {
    pub source_kind: String,
    pub file_path: Option<PathBuf>,
    pub stream_index: Option<i32>,
    pub language: Option<String>,
    pub subtitle_format: String,
    pub label: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredAudioTrack {
    pub stream_index: i32,
    pub language: Option<String>,
    pub audio_codec: Option<String>,
    pub label: Option<String>,
    pub channel_layout: Option<String>,
    pub channels: Option<i32>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i32>,
    pub is_default: bool,
}

/// 扫描目录时发现的单个视频文件。
#[derive(Debug, Clone)]
pub struct DiscoveredMediaFile {
    pub file_path: PathBuf,
    pub title: String,
    pub source_title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub year: Option<i32>,
    pub imdb_rating: Option<String>,
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
    pub video_title: Option<String>,
    pub video_codec: Option<String>,
    pub video_profile: Option<String>,
    pub video_level: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bitrate: Option<i64>,
    pub video_bitrate: Option<i64>,
    pub video_frame_rate: Option<f64>,
    pub video_aspect_ratio: Option<String>,
    pub video_scan_type: Option<String>,
    pub video_color_primaries: Option<String>,
    pub video_color_space: Option<String>,
    pub video_color_transfer: Option<String>,
    pub video_bit_depth: Option<i32>,
    pub video_pixel_format: Option<String>,
    pub video_reference_frames: Option<i32>,
    pub audio_tracks: Vec<DiscoveredAudioTrack>,
    pub subtitle_tracks: Vec<DiscoveredSubtitleTrack>,
}

#[cfg(test)]
mod tests;
