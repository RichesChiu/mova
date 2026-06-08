use crate::handlers;
use axum::{routing::get, Router};

/// 把播放进度相关接口统一挂到媒体条目路径下。
pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        .route(
            "/playback-progress/continue-watching",
            get(handlers::playback_progress::list_continue_watching),
        )
        .route(
            "/media-items/{id}/playback-progress",
            get(handlers::playback_progress::get_media_item_playback_progress)
                .put(handlers::playback_progress::update_media_item_playback_progress),
        )
}
