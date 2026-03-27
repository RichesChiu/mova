use crate::{
    auth::require_admin,
    error::ApiError,
    response::{created, ok, ok_message, ApiJson, UserResponse},
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
    pub is_enabled: Option<bool>,
    pub library_ids: Option<Vec<i64>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserLibraryAccessRequest {
    pub library_ids: Vec<i64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub role: Option<String>,
    pub is_enabled: Option<bool>,
    pub library_ids: Option<Vec<i64>>,
}

#[derive(Debug, Deserialize)]
pub struct ResetUserPasswordRequest {
    pub new_password: String,
}

pub async fn list_users(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<ApiJson<Vec<UserResponse>>, ApiError> {
    require_admin(&state, &jar).await?;

    let users = mova_application::list_users(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(users
        .into_iter()
        .map(|user| UserResponse::from_domain(user, state.api_time_offset))
        .collect()))
}

pub async fn create_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(request): Json<CreateUserRequest>,
) -> Result<(StatusCode, ApiJson<UserResponse>), ApiError> {
    require_admin(&state, &jar).await?;

    let user = mova_application::create_user(
        &state.db,
        mova_application::CreateUserInput {
            username: request.username,
            password: request.password,
            role: request.role,
            is_enabled: request.is_enabled.unwrap_or(true),
            library_ids: request.library_ids.unwrap_or_default(),
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(created(UserResponse::from_domain(
        user,
        state.api_time_offset,
    )))
}

pub async fn update_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<i64>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<ApiJson<UserResponse>, ApiError> {
    let current_user = require_admin(&state, &jar).await?;

    let user = mova_application::update_user(
        &state.db,
        current_user.user.id,
        user_id,
        mova_application::UpdateUserInput {
            username: request.username,
            role: request.role,
            is_enabled: request.is_enabled,
            library_ids: request.library_ids,
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(UserResponse::from_domain(user, state.api_time_offset)))
}

pub async fn update_user_library_access(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<i64>,
    Json(request): Json<UpdateUserLibraryAccessRequest>,
) -> Result<ApiJson<UserResponse>, ApiError> {
    require_admin(&state, &jar).await?;

    let user = mova_application::replace_user_library_access(
        &state.db,
        user_id,
        mova_application::UpdateUserLibraryAccessInput {
            library_ids: request.library_ids,
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(UserResponse::from_domain(user, state.api_time_offset)))
}

pub async fn reset_user_password(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<i64>,
    Json(request): Json<ResetUserPasswordRequest>,
) -> Result<ApiJson<()>, ApiError> {
    let current_user = require_admin(&state, &jar).await?;

    mova_application::reset_user_password(
        &state.db,
        current_user.user.id,
        user_id,
        mova_application::ResetUserPasswordInput {
            new_password: request.new_password,
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok_message("password reset", ()))
}

pub async fn delete_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<i64>,
) -> Result<ApiJson<()>, ApiError> {
    let current_user = require_admin(&state, &jar).await?;

    mova_application::delete_user(&state.db, current_user.user.id, user_id)
        .await
        .map_err(ApiError::from)?;

    Ok(ok_message("user deleted", ()))
}
