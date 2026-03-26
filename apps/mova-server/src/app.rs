use crate::{routes, state::AppState};
use axum::Router;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};

/// 组装顶层路由，把不同业务模块的子路由合并到一个应用入口上。
pub fn build_router(state: AppState, web_dist_dir: Option<PathBuf>) -> Router {
    let api_router = Router::new()
        .merge(routes::auth())
        .merge(routes::health())
        .merge(routes::libraries())
        .merge(routes::server())
        .merge(routes::media_files())
        .merge(routes::subtitle_files())
        .merge(routes::media_items())
        .merge(routes::seasons())
        .merge(routes::playback_progress())
        .merge(routes::users())
        .merge(routes::watch_history());

    let app = Router::new().nest("/api", api_router);

    let app = if let Some(web_dist_dir) = web_dist_dir {
        let index_file = web_dist_dir.join("index.html");
        app.fallback_service(ServeDir::new(web_dist_dir).fallback(ServeFile::new(index_file)))
    } else {
        app
    };

    app.with_state(state)
}
