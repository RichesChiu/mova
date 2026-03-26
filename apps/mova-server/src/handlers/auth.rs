use crate::{
    auth::{attach_session_cookie, clear_session_cookie, require_user, SESSION_TTL},
    error::ApiError,
    response::{BootstrapStatusResponse, UserResponse},
    state::AppState,
};
use axum::{extract::State, http::StatusCode, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct BootstrapAdminRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

pub async fn get_bootstrap_status(
    State(state): State<AppState>,
) -> Result<Json<BootstrapStatusResponse>, ApiError> {
    let bootstrap_required = mova_application::bootstrap_required(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(BootstrapStatusResponse { bootstrap_required }))
}

pub async fn bootstrap_admin(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(request): Json<BootstrapAdminRequest>,
) -> Result<(StatusCode, CookieJar, Json<UserResponse>), ApiError> {
    let session = mova_application::bootstrap_admin(
        &state.db,
        mova_application::BootstrapAdminInput {
            username: request.username,
            password: request.password,
        },
        SESSION_TTL,
    )
    .await
    .map_err(ApiError::from)?;

    let jar = attach_session_cookie(jar, &session.token, session.expires_at);

    Ok((
        StatusCode::CREATED,
        jar,
        Json(UserResponse::from_domain(
            session.user,
            state.api_time_offset,
        )),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(request): Json<LoginRequest>,
) -> Result<(CookieJar, Json<UserResponse>), ApiError> {
    let session = mova_application::login(
        &state.db,
        mova_application::LoginInput {
            username: request.username,
            password: request.password,
        },
        SESSION_TTL,
    )
    .await
    .map_err(ApiError::from)?;

    let jar = attach_session_cookie(jar, &session.token, session.expires_at);

    Ok((
        jar,
        Json(UserResponse::from_domain(
            session.user,
            state.api_time_offset,
        )),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), ApiError> {
    if let Some(cookie) = jar.get("mova_session") {
        mova_application::logout(&state.db, cookie.value_trimmed())
            .await
            .map_err(ApiError::from)?;
    }

    Ok((clear_session_cookie(jar), StatusCode::NO_CONTENT))
}

pub async fn current_user(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<UserResponse>, ApiError> {
    let user = require_user(&state, &jar).await?;

    Ok(Json(UserResponse::from_domain(user, state.api_time_offset)))
}

pub async fn change_password(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(request): Json<ChangePasswordRequest>,
) -> Result<(CookieJar, Json<UserResponse>), ApiError> {
    let current_user = require_user(&state, &jar).await?;
    let session = mova_application::change_own_password(
        &state.db,
        current_user.user.id,
        mova_application::ChangeOwnPasswordInput {
            current_password: request.current_password,
            new_password: request.new_password,
        },
        SESSION_TTL,
    )
    .await
    .map_err(ApiError::from)?;

    let jar = attach_session_cookie(jar, &session.token, session.expires_at);

    Ok((
        jar,
        Json(UserResponse::from_domain(
            session.user,
            state.api_time_offset,
        )),
    ))
}
