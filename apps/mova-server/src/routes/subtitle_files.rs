use crate::handlers;
use axum::{routing::get, Router};

/// 把字幕轨道输出统一挂到 `/subtitle-files` 路径下，前端播放器总是取 WebVTT。
pub fn router() -> Router<crate::state::AppState> {
    Router::new().route(
        "/subtitle-files/{id}/stream",
        get(handlers::subtitle_files::stream_subtitle_file),
    )
}
