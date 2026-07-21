mod discover;
mod parse;
mod probe;
mod sidecar;
mod subtitle;

pub use discover::{
    discover_media_file_inventory_with_progress_and_cancel, discover_media_files,
    discover_media_files_with_progress, discover_media_files_with_progress_and_cancel,
    discover_media_files_with_progress_item_and_cancel, discover_media_paths, inspect_media_file,
    inspect_media_file_inventory, inspect_media_file_inventory_shallow,
};
pub use parse::{
    infer_series_file_metadata, infer_series_sidecar_metadata, is_likely_episode_path,
    SeriesFileMetadata, SeriesSidecarMetadata,
};

use std::path::PathBuf;

use mova_domain::{MediaExternalId, MediaRating};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredMediaFileInventory {
    pub file_path: PathBuf,
    pub file_size: u64,
    pub file_modified_at_ms: Option<i64>,
}

/// 扫描目录时发现的单个视频文件。
#[derive(Debug, Clone)]
pub struct DiscoveredMediaFile {
    pub file_path: PathBuf,
    pub file_modified_at_ms: Option<i64>,
    pub probe_error: Option<String>,
    pub metadata_provider: Option<String>,
    pub metadata_provider_item_id: Option<i64>,
    pub title: String,
    pub source_title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub series_sidecar_title: Option<String>,
    pub series_sidecar_year: Option<i32>,
    pub year: Option<i32>,
    pub external_ids: Vec<MediaExternalId>,
    pub ratings: Vec<MediaRating>,
    pub metadata_status: Option<String>,
    pub metadata_failure_reason: Option<String>,
    pub remote_media_type: Option<String>,
    pub country: Option<String>,
    pub genres: Option<String>,
    pub studio: Option<String>,
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
    pub technical_tags: Vec<String>,
    pub audio_tracks: Vec<DiscoveredAudioTrack>,
    pub subtitle_tracks: Vec<DiscoveredSubtitleTrack>,
}

/// 为扫描增量同步生成稳定指纹。
/// 指纹只描述本地文件事实，不包含远端 metadata 或 ffprobe 结果，避免旧文件重复进入重探测链路。
pub fn discovered_media_file_scan_hash(file: &DiscoveredMediaFile) -> String {
    discovered_media_file_inventory_scan_hash(&DiscoveredMediaFileInventory {
        file_path: file.file_path.clone(),
        file_size: file.file_size,
        file_modified_at_ms: file.file_modified_at_ms,
    })
}

pub fn discovered_media_file_inventory_scan_hash(file: &DiscoveredMediaFileInventory) -> String {
    let mut hasher = StableScanHasher::new();

    hasher.write_u64("file_size", file.file_size);
    hasher.write_opt_i64("file_modified_at_ms", file.file_modified_at_ms);

    format!("{:016x}", hasher.finish())
}

struct StableScanHasher {
    hash: u64,
}

impl StableScanHasher {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    fn new() -> Self {
        Self {
            hash: Self::FNV_OFFSET,
        }
    }

    fn finish(self) -> u64 {
        self.hash
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(Self::FNV_PRIME);
        }
    }

    fn write_marker(&mut self, key: &str) {
        self.write_bytes(&(key.len() as u64).to_le_bytes());
        self.write_bytes(key.as_bytes());
    }

    fn write_u64(&mut self, key: &str, value: u64) {
        self.write_marker(key);
        self.write_bytes(&[1]);
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_opt_i64(&mut self, key: &str, value: Option<i64>) {
        self.write_opt_number(key, value.map(i64::to_le_bytes));
    }

    fn write_opt_number<const N: usize>(&mut self, key: &str, value: Option<[u8; N]>) {
        self.write_marker(key);
        match value {
            Some(value) => {
                self.write_bytes(&[1]);
                self.write_bytes(&value);
            }
            None => self.write_bytes(&[0]),
        }
    }
}

#[cfg(test)]
mod tests;
