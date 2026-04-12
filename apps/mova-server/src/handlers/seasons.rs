use crate::auth::{require_season_access, require_user};
use crate::error::ApiError;
use crate::response::{ok, ApiJson, EpisodeResponse};
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        header::{self, HeaderValue},
        Response, StatusCode,
    },
};
use axum_extra::extract::cookie::CookieJar;
use std::{io::ErrorKind, path::Path as FsPath};

const ARTWORK_CACHE_CONTROL: &str = "private, max-age=31536000, immutable";

/// 查询某一季下的集列表。
pub async fn list_season_episodes(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(season_id): Path<i64>,
) -> Result<ApiJson<Vec<EpisodeResponse>>, ApiError> {
    let user = require_user(&state, &jar).await?;
    require_season_access(&state, &user, season_id).await?;
    let episodes = mova_application::list_episodes_for_season(&state.db, season_id)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(episodes
        .into_iter()
        .map(|episode| EpisodeResponse::from_domain(episode, state.api_time_offset))
        .collect()))
}

/// 返回某一季的封面图内容。
pub async fn get_season_poster(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(season_id): Path<i64>,
) -> Result<Response<Body>, ApiError> {
    let user = require_user(&state, &jar).await?;
    serve_season_artwork(state, &user, season_id, SeasonArtworkKind::Poster).await
}

/// 返回某一季的背景图内容。
pub async fn get_season_backdrop(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(season_id): Path<i64>,
) -> Result<Response<Body>, ApiError> {
    let user = require_user(&state, &jar).await?;
    serve_season_artwork(state, &user, season_id, SeasonArtworkKind::Backdrop).await
}

#[derive(Debug, Clone, Copy)]
enum SeasonArtworkKind {
    Poster,
    Backdrop,
}

impl SeasonArtworkKind {
    fn field_name(self) -> &'static str {
        match self {
            Self::Poster => "poster",
            Self::Backdrop => "backdrop",
        }
    }
}

async fn serve_season_artwork(
    state: AppState,
    user: &mova_domain::UserProfile,
    season_id: i64,
    kind: SeasonArtworkKind,
) -> Result<Response<Body>, ApiError> {
    let season = require_season_access(&state, user, season_id).await?;

    let artwork_path = match kind {
        SeasonArtworkKind::Poster => season.poster_path.as_deref(),
        SeasonArtworkKind::Backdrop => season.backdrop_path.as_deref(),
    }
    .ok_or_else(|| {
        ApiError::NotFound(format!(
            "{} not available for season {}",
            kind.field_name(),
            season_id
        ))
    })?;

    if is_external_url(artwork_path) {
        return Err(ApiError::BadRequest(format!(
            "{} for season {} is stored as a remote URL and should be requested directly",
            kind.field_name(),
            season_id
        )));
    }

    let metadata = tokio::fs::metadata(artwork_path)
        .await
        .map_err(|error| map_season_artwork_io_error(kind, season_id, artwork_path, error))?;
    if !metadata.is_file() {
        return Err(ApiError::NotFound(format!(
            "{} path is not a regular file for season {}: {}",
            kind.field_name(),
            season_id,
            artwork_path
        )));
    }

    let file_bytes = tokio::fs::read(artwork_path)
        .await
        .map_err(|error| map_season_artwork_io_error(kind, season_id, artwork_path, error))?;
    let content_length = file_bytes.len();
    let content_type = content_type_for_artwork(artwork_path);

    let mut response = Response::new(Body::from(file_bytes));
    *response.status_mut() = StatusCode::OK;
    let response_headers = response.headers_mut();
    response_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response_headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&content_length.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(ARTWORK_CACHE_CONTROL),
    );

    Ok(response)
}

fn map_season_artwork_io_error(
    kind: SeasonArtworkKind,
    season_id: i64,
    artwork_path: &str,
    error: std::io::Error,
) -> ApiError {
    match error.kind() {
        ErrorKind::NotFound => ApiError::NotFound(format!(
            "{} file not found for season {}: {}",
            kind.field_name(),
            season_id,
            artwork_path
        )),
        _ => {
            tracing::error!(
                season_id,
                artwork_path,
                error = ?error,
                "failed to access season artwork on disk"
            );
            ApiError::Internal
        }
    }
}

fn content_type_for_artwork(path: &str) -> &'static str {
    match FsPath::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
}

fn is_external_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}
