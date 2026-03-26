use crate::handlers;
use axum::{routing::get, Router};

pub fn router() -> Router<crate::state::AppState> {
    Router::new().route(
        "/watch-history",
        get(handlers::watch_history::list_watch_history),
    )
}
