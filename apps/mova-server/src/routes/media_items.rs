use crate::handlers;
use axum::{
    routing::{get, post},
    Router,
};

/// 把媒体条目相关接口统一挂到 `/media-items` 路径下。
pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        // `GET /media-items/{id}`：查询单个媒体条目详情。
        .route(
            "/media-items/{id}",
            get(handlers::media_items::get_media_item),
        )
        .route(
            "/media-items/{id}/cast",
            get(handlers::media_items::list_media_item_cast),
        )
        .route(
            "/media-items/{id}/playback-header",
            get(handlers::media_items::get_media_item_playback_header),
        )
        // `GET /media-items/{id}/files`：查询该媒体条目关联的物理文件列表。
        .route(
            "/media-items/{id}/files",
            get(handlers::media_items::list_media_item_files),
        )
        .route(
            "/media-items/{id}/seasons",
            get(handlers::media_items::list_media_item_seasons),
        )
        .route(
            "/media-items/{id}/episode-outline",
            get(handlers::media_items::get_media_item_episode_outline),
        )
        .route(
            "/media-items/{id}/metadata-search",
            get(handlers::media_items::search_media_item_metadata),
        )
        .route(
            "/media-items/{id}/metadata-match",
            post(handlers::media_items::apply_media_item_metadata_match),
        )
        // `POST /media-items/{id}/refresh-metadata`：手动重拉单个媒体条目的 metadata。
        .route(
            "/media-items/{id}/refresh-metadata",
            post(handlers::media_items::refresh_media_item_metadata),
        )
        .route(
            "/media-items/{id}/poster",
            get(handlers::media_items::get_media_item_poster),
        )
        .route(
            "/media-items/{id}/backdrop",
            get(handlers::media_items::get_media_item_backdrop),
        )
}
