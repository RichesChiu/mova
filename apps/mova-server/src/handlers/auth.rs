use crate::{
    auth::{
        attach_session_cookie, clear_session_cookie, request_auth_credential, require_user,
        AuthCredential, NATIVE_ACCESS_TOKEN_TTL, NATIVE_REFRESH_TOKEN_TTL, SESSION_TTL,
    },
    error::ApiError,
    response::{
        created, ok, ok_message, ApiJson, BootstrapStatusResponse, TokenLoginResponse, UserResponse,
    },
    state::AppState,
};
use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub device_name: Option<String>,
    pub client_type: Option<String>,
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

#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
pub struct LogoutRequest {
    pub refresh_token: Option<String>,
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

    let jar = attach_session_cookie(
        jar,
        &session.token,
        session.expires_at,
        state.session_cookie_secure,
    );
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
    let rate_limit_key = account_rate_limit_key(&request.username);
    enforce_auth_rate_limit(&state, &rate_limit_key)?;
    let session = resolve_auth_attempt(
        &state,
        &rate_limit_key,
        true,
        mova_application::login(
            &state.db,
            mova_application::LoginInput {
                username: request.username,
                password: request.password,
            },
            SESSION_TTL,
        )
        .await,
    )?;

    let jar = attach_session_cookie(
        jar,
        &session.token,
        session.expires_at,
        state.session_cookie_secure,
    );

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
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<ApiJson<TokenLoginResponse>, ApiError> {
    let rate_limit_key = account_rate_limit_key(&request.username);
    enforce_auth_rate_limit(&state, &rate_limit_key)?;
    let session = resolve_auth_attempt(
        &state,
        &rate_limit_key,
        true,
        mova_application::login_native_client(
            &state.db,
            mova_application::NativeClientLoginInput {
                username: request.username,
                password: request.password,
                user_agent: request_user_agent(&headers),
                device_name: request.device_name,
                client_type: request.client_type,
            },
            NATIVE_ACCESS_TOKEN_TTL,
            NATIVE_REFRESH_TOKEN_TTL,
        )
        .await,
    )?;

    Ok(ok(TokenLoginResponse::from_native_session(
        session,
        state.api_time_offset,
    )))
}

pub async fn refresh_token(
    State(state): State<AppState>,
    Json(request): Json<RefreshTokenRequest>,
) -> Result<ApiJson<TokenLoginResponse>, ApiError> {
    let session = mova_application::refresh_native_client_session(
        &state.db,
        mova_application::RefreshNativeClientSessionInput {
            refresh_token: request.refresh_token,
        },
        NATIVE_ACCESS_TOKEN_TTL,
        NATIVE_REFRESH_TOKEN_TTL,
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(TokenLoginResponse::from_native_session(
        session,
        state.api_time_offset,
    )))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    body: Bytes,
) -> Result<(CookieJar, ApiJson<()>), ApiError> {
    if let Ok(credential) = request_auth_credential(&headers, &jar) {
        match credential {
            AuthCredential::Bearer(token) => {
                mova_application::logout_native_client_access_token(&state.db, &token)
                    .await
                    .map_err(ApiError::from)?;
            }
            AuthCredential::SessionCookie(token) => {
                mova_application::logout(&state.db, &token)
                    .await
                    .map_err(ApiError::from)?;
            }
        }
    }

    if let Some(request) = parse_logout_request(&body)? {
        if let Some(refresh_token) = request.refresh_token {
            mova_application::logout_native_client_refresh_token(&state.db, &refresh_token)
                .await
                .map_err(ApiError::from)?;
        }
    }

    Ok((clear_session_cookie(jar), ok_message("logged out", ())))
}

fn parse_logout_request(body: &[u8]) -> Result<Option<LogoutRequest>, ApiError> {
    if body.is_empty() {
        return Ok(None);
    }

    serde_json::from_slice(body)
        .map(Some)
        .map_err(|_| ApiError::BadRequest("invalid logout request body".to_string()))
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
    let rate_limit_key = format!("password:{}", current_user.user.id);
    enforce_auth_rate_limit(&state, &rate_limit_key)?;
    let session = resolve_auth_attempt(
        &state,
        &rate_limit_key,
        false,
        mova_application::change_own_password(
            &state.db,
            current_user.user.id,
            mova_application::ChangeOwnPasswordInput {
                current_password: request.current_password,
                new_password: request.new_password,
            },
            SESSION_TTL,
        )
        .await,
    )?;

    let jar = attach_session_cookie(
        jar,
        &session.token,
        session.expires_at,
        state.session_cookie_secure,
    );

    Ok((
        jar,
        ok_message(
            "password updated",
            UserResponse::from_domain(session.user, state.api_time_offset),
        ),
    ))
}

fn account_rate_limit_key(username: &str) -> String {
    let normalized = username.trim().to_lowercase();
    format!("account:{:x}", Sha256::digest(normalized.as_bytes()))
}

fn enforce_auth_rate_limit(state: &AppState, key: &str) -> Result<(), ApiError> {
    state
        .auth_rate_limiter
        .check(key)
        .map_err(auth_rate_limit_error)
}

fn resolve_auth_attempt<T>(
    state: &AppState,
    key: &str,
    count_validation_failure: bool,
    result: mova_application::ApplicationResult<T>,
) -> Result<T, ApiError> {
    match result {
        Ok(value) => {
            state.auth_rate_limiter.record_success(key);
            Ok(value)
        }
        Err(error) => {
            if is_counted_auth_failure(&error, count_validation_failure) {
                if let Some(retry_after_seconds) = state.auth_rate_limiter.record_failure(key) {
                    return Err(auth_rate_limit_error(retry_after_seconds));
                }
            }

            Err(ApiError::from(error))
        }
    }
}

fn is_counted_auth_failure(
    error: &mova_application::ApplicationError,
    count_validation_failure: bool,
) -> bool {
    match error {
        mova_application::ApplicationError::Unauthorized(_) => true,
        mova_application::ApplicationError::Validation(_) => count_validation_failure,
        mova_application::ApplicationError::Business(error) => match error.kind() {
            mova_application::BusinessErrorKind::Unauthorized => true,
            mova_application::BusinessErrorKind::Validation => count_validation_failure,
            _ => false,
        },
        _ => false,
    }
}

fn auth_rate_limit_error(retry_after_seconds: u64) -> ApiError {
    ApiError::TooManyRequests {
        message: "too many authentication attempts; try again later".to_string(),
        retry_after_seconds,
    }
}

fn request_user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::{account_rate_limit_key, is_counted_auth_failure, parse_logout_request};

    #[test]
    fn logout_request_accepts_an_empty_body() {
        assert!(parse_logout_request(&[]).unwrap().is_none());
    }

    #[test]
    fn logout_request_accepts_an_optional_refresh_token() {
        let request = parse_logout_request(br#"{"refresh_token":"refresh-token"}"#)
            .unwrap()
            .unwrap();

        assert_eq!(request.refresh_token.as_deref(), Some("refresh-token"));
    }

    #[test]
    fn logout_request_rejects_invalid_json() {
        assert!(parse_logout_request(b"not-json").is_err());
    }

    #[test]
    fn account_rate_limit_key_normalizes_case_and_whitespace() {
        assert_eq!(
            account_rate_limit_key("  Viewer@Example.COM "),
            account_rate_limit_key("viewer@example.com")
        );
        assert_eq!(account_rate_limit_key("viewer").len(), 72);
    }

    #[test]
    fn password_change_only_counts_current_password_failures() {
        let validation = mova_application::ApplicationError::validation(
            "field_too_short",
            std::collections::BTreeMap::new(),
            "invalid new password",
        );
        let unauthorized = mova_application::ApplicationError::unauthorized(
            "invalid_current_password",
            std::collections::BTreeMap::new(),
            "wrong password",
        );

        assert!(!is_counted_auth_failure(&validation, false));
        assert!(is_counted_auth_failure(&unauthorized, false));
        assert!(is_counted_auth_failure(&validation, true));
    }
}
