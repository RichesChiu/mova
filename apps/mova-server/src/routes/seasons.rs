use crate::handlers;
use axum::{routing::get, Router};

pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        .route(
            "/seasons/{id}/poster",
            get(handlers::seasons::get_season_poster),
        )
        .route(
            "/seasons/{id}/backdrop",
            get(handlers::seasons::get_season_backdrop),
        )
}
