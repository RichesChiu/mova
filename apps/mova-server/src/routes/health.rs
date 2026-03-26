use crate::handlers;
use axum::{routing::get, Router};

/// 暴露 `/health` GET 接口，供本地检查和部署探针使用。
pub fn router() -> Router<crate::state::AppState> {
    Router::new().route("/health", get(handlers::health::health))
}
