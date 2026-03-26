use crate::handlers;
use axum::{routing::get, Router};

pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        .route(
            "/users",
            get(handlers::users::list_users).post(handlers::users::create_user),
        )
        .route(
            "/users/{id}",
            axum::routing::patch(handlers::users::update_user).delete(handlers::users::delete_user),
        )
        .route(
            "/users/{id}/library-access",
            axum::routing::put(handlers::users::update_user_library_access),
        )
        .route(
            "/users/{id}/password",
            axum::routing::put(handlers::users::reset_user_password),
        )
}
