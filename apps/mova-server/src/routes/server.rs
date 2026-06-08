use crate::handlers;
use axum::{routing::get, Router};

/// 服务器侧运行时信息（如容器内媒体目录树）接口。
pub fn router() -> Router<crate::state::AppState> {
    Router::new().route("/server/media-tree", get(handlers::server::get_media_tree))
}
