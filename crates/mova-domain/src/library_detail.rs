use crate::{Library, ScanJob};

/// 媒体库详情聚合对象。
/// 用来承载库本身信息，以及详情页首屏常用的统计摘要。
#[derive(Debug, Clone)]
pub struct LibraryDetail {
    pub library: Library,
    pub media_count: i64,
    pub movie_count: i64,
    pub series_count: i64,
    pub last_scan: Option<ScanJob>,
}
