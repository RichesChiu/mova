mod query;
mod series;
mod sync;

pub use query::{
    count_media_items_for_library, delete_series_episode_outline_cache,
    get_library_media_type_counts, get_media_file, get_media_item, get_media_item_playback_header,
    get_season, get_series_episode_outline_cache, list_episodes_for_season,
    list_library_media_file_paths, list_media_files_for_media_item, list_media_items_for_library,
    list_seasons_for_series, update_media_file_metadata, update_media_item_metadata,
    upsert_series_episode_outline_cache,
};
pub use sync::{
    delete_library_media_by_file_path, delete_library_media_by_path_prefix, sync_library_media,
    upsert_library_media_entry_by_file_path,
};
use time::OffsetDateTime;

/// 重建某个媒体库内容时，每个视频文件对应的一组入库参数。
#[derive(Debug, Clone)]
pub struct CreateMediaEntryParams {
    pub library_id: i64,
    pub media_type: String,
    pub title: String,
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
    pub file_path: String,
    pub container: Option<String>,
    pub file_size: i64,
    pub duration_seconds: Option<i32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bitrate: Option<i64>,
}

/// 手动刷新单个媒体条目时允许更新的 metadata 字段。
#[derive(Debug, Clone)]
pub struct UpdateMediaItemMetadataParams {
    pub title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub metadata_provider: Option<String>,
    pub metadata_provider_item_id: Option<i64>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

/// 刷新单个媒体文件时允许更新的源文件和探测字段。
#[derive(Debug, Clone)]
pub struct UpdateMediaFileMetadataParams {
    pub file_path: String,
    pub container: Option<String>,
    pub file_size: i64,
    pub duration_seconds: Option<i32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bitrate: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ListMediaItemsForLibraryParams {
    pub library_id: i64,
    pub query: Option<String>,
    pub year: Option<i32>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone)]
pub struct ListMediaItemsForLibraryResult {
    pub items: Vec<mova_domain::MediaItem>,
    pub total: i64,
}

#[derive(Debug, Clone)]
pub struct LibraryMediaTypeCounts {
    pub movie_count: i64,
    pub series_count: i64,
}

#[derive(Debug, Clone)]
pub struct MediaItemPlaybackHeader {
    pub media_item_id: i64,
    pub library_id: i64,
    pub media_type: String,
    pub series_media_item_id: Option<i64>,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub episode_title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SeriesEpisodeOutlineCacheEntry {
    pub series_media_item_id: i64,
    pub outline_json: String,
    pub fetched_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct UpsertSeriesEpisodeOutlineCacheParams {
    pub series_media_item_id: i64,
    pub outline_json: String,
    pub fetched_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}
