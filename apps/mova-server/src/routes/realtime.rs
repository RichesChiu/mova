use crate::handlers;
use axum::{routing::get, Router};

pub fn router() -> Router<crate::state::AppState> {
    Router::new().route("/events", get(handlers::realtime::events))
}
