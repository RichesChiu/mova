use crate::{
    auth::{
        attach_session_cookie, clear_session_cookie, request_auth_token, require_user, SESSION_TTL,
    },
    error::ApiError,
    response::{
        created, ok, ok_message, ApiJson, BootstrapStatusResponse, TokenLoginResponse, UserResponse,
    },
    state::AppState,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
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

#[derive(Debug, Deserialize)]
pub struct UpdateOwnProfileRequest {
    pub nickname: String,
}

pub async fn get_bootstrap_status(
    State(state): State<AppState>,
) -> Result<ApiJson<BootstrapStatusResponse>, ApiError> {
    let bootstrap_required = mova_application::bootstrap_required(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(BootstrapStatusResponse { bootstrap_required }))
}

pub async fn bootstrap_admin(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(request): Json<BootstrapAdminRequest>,
) -> Result<(StatusCode, CookieJar, ApiJson<UserResponse>), ApiError> {
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
    let (status, payload) = created(UserResponse::from_domain(
        session.user,
        state.api_time_offset,
    ));

    Ok((status, jar, payload))
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(request): Json<LoginRequest>,
) -> Result<(CookieJar, ApiJson<UserResponse>), ApiError> {
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
        ok(UserResponse::from_domain(
            session.user,
            state.api_time_offset,
        )),
    ))
}

pub async fn token_login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<ApiJson<TokenLoginResponse>, ApiError> {
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

    Ok(ok(TokenLoginResponse::from_session(
        session,
        state.api_time_offset,
    )))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<(CookieJar, ApiJson<()>), ApiError> {
    if let Ok(token) = request_auth_token(&headers, &jar) {
        mova_application::logout(&state.db, &token)
            .await
            .map_err(ApiError::from)?;
    }

    Ok((clear_session_cookie(jar), ok_message("logged out", ())))
}

pub async fn current_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<ApiJson<UserResponse>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;

    Ok(ok(UserResponse::from_domain(user, state.api_time_offset)))
}

pub async fn update_own_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(request): Json<UpdateOwnProfileRequest>,
) -> Result<ApiJson<UserResponse>, ApiError> {
    let current_user = require_user(&state, &headers, &jar).await?;
    let user = mova_application::update_own_profile(
        &state.db,
        current_user.user.id,
        mova_application::UpdateOwnProfileInput {
            nickname: request.nickname,
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(UserResponse::from_domain(user, state.api_time_offset)))
}

pub async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(request): Json<ChangePasswordRequest>,
) -> Result<(CookieJar, ApiJson<UserResponse>), ApiError> {
    let current_user = require_user(&state, &headers, &jar).await?;
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
        ok_message(
            "password updated",
            UserResponse::from_domain(session.user, state.api_time_offset),
        ),
    ))
}
