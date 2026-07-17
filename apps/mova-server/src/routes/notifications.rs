use crate::handlers;
use axum::{routing::get, Router};

pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        .route(
            "/notifications",
            get(handlers::notifications::list_notifications)
                .put(handlers::notifications::mark_all_notifications_read),
        )
        .route(
            "/notifications/{id}/read",
            axum::routing::put(handlers::notifications::mark_notification_read),
        )
}
