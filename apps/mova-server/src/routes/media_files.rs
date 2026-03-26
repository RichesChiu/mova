use crate::handlers;
use axum::{routing::get, Router};

/// 把媒体文件播放相关接口统一挂到 `/media-files` 路径下。
pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        .route(
            "/media-files/{id}/stream",
            get(handlers::media_files::stream_media_file)
                .head(handlers::media_files::head_media_file),
        )
        .route(
            "/media-files/{id}/subtitles",
            get(handlers::subtitle_files::list_media_file_subtitles),
        )
}
