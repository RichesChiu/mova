use crate::auth::require_user;
use crate::error::ApiError;
use crate::response::{ok, ApiJson, WatchHistoryItemResponse};
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::HeaderMap,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct WatchHistoryQuery {
    pub limit: Option<i64>,
}

pub async fn list_watch_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Query(query): Query<WatchHistoryQuery>,
) -> Result<ApiJson<Vec<WatchHistoryItemResponse>>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    let items = mova_application::list_watch_history(&state.db, user.user.id, query.limit)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(items
        .into_iter()
        .filter(|item| user.can_access_library(item.media_item.library_id))
        .map(|item| WatchHistoryItemResponse::from_domain(item, state.api_time_offset))
        .collect()))
}
