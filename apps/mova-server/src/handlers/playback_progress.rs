use crate::auth::{require_media_item_access, require_user};
use crate::error::ApiError;
use crate::response::{ok, ApiJson, ContinueWatchingItemResponse, PlaybackProgressResponse};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

/// 更新播放进度接口接收的请求体。
#[derive(Debug, Deserialize)]
pub struct UpdatePlaybackProgressRequest {
    pub media_file_id: i64,
    pub position_seconds: i32,
    pub duration_seconds: Option<i32>,
    pub is_finished: Option<bool>,
}

/// 查询“继续观看”列表时支持的可选参数。
#[derive(Debug, Deserialize)]
pub struct ContinueWatchingQuery {
    pub limit: Option<i64>,
}

/// 读取当前登录用户的“继续观看”列表。
pub async fn list_continue_watching(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<ContinueWatchingQuery>,
) -> Result<ApiJson<Vec<ContinueWatchingItemResponse>>, ApiError> {
    let user = require_user(&state, &jar).await?;
    let items = mova_application::list_continue_watching(&state.db, user.user.id, query.limit)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(items
        .into_iter()
        .filter(|item| user.can_access_library(item.media_item.library_id))
        .map(|item| ContinueWatchingItemResponse::from_domain(item, state.api_time_offset))
        .collect()))
}

/// 读取某个媒体条目的最近播放进度。
pub async fn get_media_item_playback_progress(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
) -> Result<ApiJson<Option<PlaybackProgressResponse>>, ApiError> {
    let user = require_user(&state, &jar).await?;
    require_media_item_access(&state, &user, media_item_id).await?;
    let progress = mova_application::get_playback_progress_for_media_item(
        &state.db,
        user.user.id,
        media_item_id,
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(progress.map(|value| {
        PlaybackProgressResponse::from_domain(value, state.api_time_offset)
    })))
}

/// 写入某个媒体条目的播放进度。
pub async fn update_media_item_playback_progress(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
    Json(request): Json<UpdatePlaybackProgressRequest>,
) -> Result<ApiJson<PlaybackProgressResponse>, ApiError> {
    let user = require_user(&state, &jar).await?;
    require_media_item_access(&state, &user, media_item_id).await?;
    let progress = mova_application::update_playback_progress_for_media_item(
        &state.db,
        user.user.id,
        media_item_id,
        mova_application::UpdatePlaybackProgressInput {
            media_file_id: request.media_file_id,
            position_seconds: request.position_seconds,
            duration_seconds: request.duration_seconds,
            is_finished: request.is_finished.unwrap_or(false),
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(PlaybackProgressResponse::from_domain(
        progress,
        state.api_time_offset,
    )))
}
