use crate::{MediaItem, PlaybackProgress};

/// “继续观看”列表里的单个条目。
/// 这里把媒体条目摘要和最近播放进度聚合到一起，方便前端列表直接渲染。
#[derive(Debug, Clone)]
pub struct ContinueWatchingItem {
    pub media_item: MediaItem,
    pub playback_progress: PlaybackProgress,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub episode_title: Option<String>,
    pub episode_overview: Option<String>,
    pub episode_poster_path: Option<String>,
    pub episode_backdrop_path: Option<String>,
}
