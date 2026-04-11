use crate::handlers;
use axum::{routing::get, Router};

pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        .route(
            "/auth/bootstrap-status",
            get(handlers::auth::get_bootstrap_status),
        )
        .route(
            "/auth/bootstrap-admin",
            axum::routing::post(handlers::auth::bootstrap_admin),
        )
        .route("/auth/login", axum::routing::post(handlers::auth::login))
        .route("/auth/logout", axum::routing::post(handlers::auth::logout))
        .route(
            "/auth/me",
            get(handlers::auth::current_user).patch(handlers::auth::update_own_profile),
        )
        .route(
            "/auth/password",
            axum::routing::put(handlers::auth::change_password),
        )
}
