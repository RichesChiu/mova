use crate::{MediaItem, WatchHistory};

/// 观看历史列表里的单个条目。
#[derive(Debug, Clone)]
pub struct WatchHistoryItem {
    pub media_item: MediaItem,
    pub watch_history: WatchHistory,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub episode_title: Option<String>,
    pub episode_overview: Option<String>,
    pub episode_poster_path: Option<String>,
    pub episode_backdrop_path: Option<String>,
}
