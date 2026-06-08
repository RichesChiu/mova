use crate::{MediaItem, WatchHistory};

/// 观看历史列表里的单个条目。
#[derive(Debug, Clone)]
pub struct WatchHistoryItem {
    pub media_item: MediaItem,
    pub watch_history: WatchHistory,
}
