use crate::handlers;
use axum::{routing::get, Router};

pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        .route("/realtime/events", get(handlers::realtime::events))
        .route("/realtime/state", get(handlers::realtime::state))
}
