use crate::handlers;
use axum::{routing::get, Router};

/// 服务器侧运行时信息（如可选媒体根目录）接口。
pub fn router() -> Router<crate::state::AppState> {
    Router::new().route("/server/root-paths", get(handlers::server::list_root_paths))
}
