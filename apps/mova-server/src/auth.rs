use crate::{error::ApiError, state::AppState};
use axum::http::{header, HeaderMap};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use mova_domain::{Library, MediaFile, MediaItem, Season, UserProfile};
use time::{Duration, OffsetDateTime};

pub const SESSION_TTL: Duration = Duration::days(30);
const SESSION_COOKIE_NAME: &str = "mova_session";

pub async fn require_user(
    state: &AppState,
    headers: &HeaderMap,
    jar: &CookieJar,
) -> Result<UserProfile, ApiError> {
    let token = request_auth_token(headers, jar)?;

    mova_application::get_user_by_session_token(&state.db, &token)
        .await
        .map_err(ApiError::from)
}

pub async fn require_admin(
    state: &AppState,
    headers: &HeaderMap,
    jar: &CookieJar,
) -> Result<UserProfile, ApiError> {
    let user = require_user(state, headers, jar).await?;
    if !user.is_admin() {
        return Err(ApiError::Forbidden("admin permission required".to_string()));
    }

    Ok(user)
}

pub fn attach_session_cookie(jar: CookieJar, token: &str, expires_at: OffsetDateTime) -> CookieJar {
    jar.add(build_session_cookie(token, expires_at))
}

pub fn clear_session_cookie(jar: CookieJar) -> CookieJar {
    jar.remove(Cookie::build((SESSION_COOKIE_NAME, "")).path("/").build())
}

pub fn request_auth_token(headers: &HeaderMap, jar: &CookieJar) -> Result<String, ApiError> {
    bearer_token(headers)
        .or_else(|| session_token(jar))
        .ok_or_else(|| ApiError::Unauthorized("authentication required".to_string()))
}

pub async fn require_library_access(
    state: &AppState,
    user: &UserProfile,
    library_id: i64,
) -> Result<Library, ApiError> {
    let library = mova_application::get_library(&state.db, library_id)
        .await
        .map_err(ApiError::from)?;

    if !user.can_access_library(library.id) {
        return Err(ApiError::Forbidden(format!(
            "user {} cannot access library {}",
            user.user.username, library.id
        )));
    }

    Ok(library)
}

pub async fn require_media_item_access(
    state: &AppState,
    user: &UserProfile,
    media_item_id: i64,
) -> Result<MediaItem, ApiError> {
    let media_item = mova_application::get_media_item(&state.db, media_item_id)
        .await
        .map_err(ApiError::from)?;
    require_library_access(state, user, media_item.library_id).await?;

    Ok(media_item)
}

pub async fn require_media_file_access(
    state: &AppState,
    user: &UserProfile,
    media_file_id: i64,
) -> Result<MediaFile, ApiError> {
    let media_file = mova_application::get_media_file(&state.db, media_file_id)
        .await
        .map_err(ApiError::from)?;
    let media_item = mova_application::get_media_item(&state.db, media_file.media_item_id)
        .await
        .map_err(ApiError::from)?;
    require_library_access(state, user, media_item.library_id).await?;

    Ok(media_file)
}

pub async fn require_season_access(
    state: &AppState,
    user: &UserProfile,
    season_id: i64,
) -> Result<Season, ApiError> {
    let season = mova_application::get_season(&state.db, season_id)
        .await
        .map_err(ApiError::from)?;
    let series = mova_application::get_media_item(&state.db, season.series_id)
        .await
        .map_err(ApiError::from)?;
    require_library_access(state, user, series.library_id).await?;

    Ok(season)
}

fn session_token(jar: &CookieJar) -> Option<String> {
    jar.get(SESSION_COOKIE_NAME)
        .map(|cookie| cookie.value_trimmed())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let authorization = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let mut parts = authorization.splitn(2, char::is_whitespace);
    let scheme = parts.next()?;
    let token = parts.next()?.trim();

    if !scheme.eq_ignore_ascii_case("bearer") || token.is_empty() {
        return None;
    }

    Some(token.to_string())
}

fn build_session_cookie(token: &str, expires_at: OffsetDateTime) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, token.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .expires(expires_at)
        .build()
}

#[cfg(test)]
mod tests {
    use super::{request_auth_token, SESSION_COOKIE_NAME};
    use axum::http::{header, HeaderMap, HeaderValue};
    use axum_extra::extract::cookie::{Cookie, CookieJar};

    #[test]
    fn request_auth_token_reads_bearer_token_from_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer native-client-token"),
        );

        let token = request_auth_token(&headers, &CookieJar::new()).unwrap();

        assert_eq!(token, "native-client-token");
    }

    #[test]
    fn request_auth_token_falls_back_to_session_cookie() {
        let jar = CookieJar::new().add(Cookie::new(SESSION_COOKIE_NAME, "cookie-session-token"));

        let token = request_auth_token(&HeaderMap::new(), &jar).unwrap();

        assert_eq!(token, "cookie-session-token");
    }

    #[test]
    fn request_auth_token_prefers_bearer_over_cookie_when_both_exist() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer native-client-token"),
        );
        let jar = CookieJar::new().add(Cookie::new(SESSION_COOKIE_NAME, "cookie-session-token"));

        let token = request_auth_token(&headers, &jar).unwrap();

        assert_eq!(token, "native-client-token");
    }
}
