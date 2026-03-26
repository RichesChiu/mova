use crate::{routes, state::AppState};
use axum::Router;

/// 组装顶层路由，把不同业务模块的子路由合并到一个应用入口上。
pub fn build_router(state: AppState) -> Router {
    let api_router = Router::new()
        .merge(routes::auth())
        .merge(routes::health())
        .merge(routes::libraries())
        .merge(routes::server())
        .merge(routes::media_files())
        .merge(routes::media_items())
        .merge(routes::seasons())
        .merge(routes::playback_progress())
        .merge(routes::users())
        .merge(routes::watch_history());

    Router::new().nest("/api", api_router).with_state(state)
}
