use crate::{error::ApiError, state::AppState};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use mova_domain::{Library, MediaFile, MediaItem, Season, UserProfile};
use time::{Duration, OffsetDateTime};

pub const SESSION_TTL: Duration = Duration::days(30);
const SESSION_COOKIE_NAME: &str = "mova_session";

pub async fn require_user(state: &AppState, jar: &CookieJar) -> Result<UserProfile, ApiError> {
    let token = session_token(jar)?;

    mova_application::get_user_by_session_token(&state.db, token)
        .await
        .map_err(ApiError::from)
}

pub async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<UserProfile, ApiError> {
    let user = require_user(state, jar).await?;
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

fn session_token<'a>(jar: &'a CookieJar) -> Result<&'a str, ApiError> {
    jar.get(SESSION_COOKIE_NAME)
        .map(|cookie| cookie.value_trimmed())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::Unauthorized("authentication required".to_string()))
}

fn build_session_cookie(token: &str, expires_at: OffsetDateTime) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, token.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .expires(expires_at)
        .build()
}
