use crate::{
    auth::require_user,
    error::ApiError,
    response::{ok, ok_message, ApiJson, NotificationFeedResponse},
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct ListNotificationsQuery {
    pub category: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct MarkAllNotificationsReadRequest {
    pub category: Option<String>,
}

pub async fn list_notifications(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Query(query): Query<ListNotificationsQuery>,
) -> Result<ApiJson<NotificationFeedResponse>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    let feed = mova_application::list_notifications(
        &state.db,
        &user,
        query.category.as_deref(),
        query.limit,
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(NotificationFeedResponse::from_domain(
        feed,
        state.api_time_offset,
    )))
}

pub async fn mark_notification_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(notification_id): Path<i64>,
) -> Result<ApiJson<()>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    mova_application::mark_notification_read(&state.db, &user, notification_id)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_message("notification marked as read", ()))
}

pub async fn mark_all_notifications_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(request): Json<MarkAllNotificationsReadRequest>,
) -> Result<ApiJson<u64>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    let marked_count = mova_application::mark_all_notifications_read(
        &state.db,
        &user,
        request.category.as_deref(),
    )
    .await
    .map_err(ApiError::from)?;
    Ok(ok_message("notifications marked as read", marked_count))
}
