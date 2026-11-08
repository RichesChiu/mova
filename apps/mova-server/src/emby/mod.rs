//! The Emby JSON wire adapter. All business use cases remain in application.
mod auth;
mod browse;
mod dto;
mod playback;
#[cfg(test)]
mod tests;

use crate::{error::ApiError, state::AppState};
use axum::{
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/System/Info/Public", get(auth::system_info))
        .route("/System/Info", get(auth::private_system_info))
        .route(
            "/System/Ping",
            get(|| async { "Mova" }).post(|| async { "Mova" }),
        )
        .route("/Users/Public", get(|| async { Json(json!([])) }))
        .route("/Users/AuthenticateByName", post(auth::login))
        .route("/Users/Me", get(auth::me))
        .route("/Users/{user}", get(auth::user))
        .route("/Sessions/Logout", post(auth::logout))
        .route("/Sessions/Capabilities/Full", post(auth::capabilities))
        .route("/Sessions/Capabilities", post(auth::capabilities))
        .route("/Users/{user}/Views", get(browse::views))
        .route("/Users/{user}/Items/Root", get(browse::root))
        .route("/Users/{user}/Items", get(browse::items))
        .route("/Items", get(browse::items))
        .route("/Users/{user}/Items/Latest", get(browse::latest))
        .route("/Users/{user}/Items/Resume", get(browse::resume))
        .route("/Users/{user}/Items/{item}", get(browse::item))
        .route("/Items/{item}", get(browse::item))
        .route("/Shows/{item}/Seasons", get(browse::seasons))
        .route("/Shows/{item}/Episodes", get(browse::episodes))
        .route("/Items/{item}/Images/{kind}", get(browse::image))
        .route("/Items/{item}/Images/{kind}/{index}", get(browse::image))
        .route(
            "/Items/{item}/PlaybackInfo",
            get(playback::info_get).post(playback::info_post),
        )
        .route("/Videos/{item}/stream", get(playback::stream))
        .route("/Videos/{item}/stream.{container}", get(playback::stream))
        .route("/Video/{item}/stream", get(playback::stream))
        .route(
            "/Videos/{item}/{source}/Subtitles/{index}/Stream.vtt",
            get(playback::subtitle),
        )
        .route("/Sessions/Playing", post(playback::report))
        .route("/Sessions/Playing/Progress", post(playback::report))
        .route("/Sessions/Playing/Stopped", post(playback::report))
        .fallback(|| async { ApiError::NotFound("unsupported Emby endpoint".into()) })
        .layer(middleware::from_fn(protocol_errors))
}

/// Keep unsupported compatibility aliases out of the web SPA fallback.
pub async fn guard_fallback(
    req: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let namespace = req.uri().path().split('/').nth(1).unwrap_or("");
    if matches!(
        namespace,
        "emby" | "System" | "Users" | "Items" | "Shows" | "Videos" | "Video" | "Sessions"
    ) && req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .is_none()
    {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({"ResponseStatus":{"ErrorCode":"404","Message":"Not Found"}})),
        )
            .into_response();
    }
    next.run(req).await
}

// Preserve HTTP status and transport headers (Retry-After, Content-Range), while
// keeping the native envelope and diagnostic internals out of this protocol.
async fn protocol_errors(
    req: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(req).await;
    if response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"application/json"))
    {
        response.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("private, no-store"),
        );
    }
    if !response.status().is_client_error() && !response.status().is_server_error() {
        return response;
    }
    let (mut parts, _) = response.into_parts();
    parts.headers.remove(axum::http::header::CONTENT_LENGTH);
    let body=Json(json!({"ResponseStatus":{"ErrorCode":parts.status.as_str(),"Message":parts.status.canonical_reason().unwrap_or("Request failed")}})).into_response().into_body();
    parts.headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    axum::response::Response::from_parts(parts, body)
}
