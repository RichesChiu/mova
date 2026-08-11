use crate::artwork::{read_trusted_local_artwork, LocalArtworkError};
use crate::auth::{require_season_with_library_access, AuthenticatedUser};
use crate::error::ApiError;
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        header::{self, HeaderValue},
        Response, StatusCode,
    },
};
const ARTWORK_CACHE_CONTROL: &str = "private, max-age=31536000, immutable";

/// 返回某一季的封面图内容。
pub async fn get_season_poster(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Path(season_id): Path<i64>,
) -> Result<Response<Body>, ApiError> {
    serve_season_artwork(state, &user, season_id, SeasonArtworkKind::Poster).await
}

/// 返回某一季的背景图内容。
pub async fn get_season_backdrop(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Path(season_id): Path<i64>,
) -> Result<Response<Body>, ApiError> {
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
    let (season, library) = require_season_with_library_access(&state, user, season_id).await?;

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

    let artwork_cache_root =
        mova_application::library_artwork_cache_dir(&state.cache_dir, library.id);
    let artwork = read_trusted_local_artwork(
        artwork_path,
        std::path::Path::new(&library.root_path),
        &artwork_cache_root,
    )
    .await
    .map_err(|error| map_season_artwork_error(kind, season_id, artwork_path, error))?;
    let content_length = artwork.bytes.len();

    let mut response = Response::new(Body::from(artwork.bytes));
    *response.status_mut() = StatusCode::OK;
    let response_headers = response.headers_mut();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(artwork.content_type),
    );
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

fn map_season_artwork_error(
    kind: SeasonArtworkKind,
    season_id: i64,
    artwork_path: &str,
    error: LocalArtworkError,
) -> ApiError {
    match error {
        LocalArtworkError::NotFound => ApiError::NotFound(format!(
            "{} not available for season {}",
            kind.field_name(),
            season_id,
        )),
        LocalArtworkError::Untrusted => {
            tracing::warn!(
                season_id,
                artwork_path,
                artwork_kind = kind.field_name(),
                "rejected untrusted season artwork path or payload"
            );
            ApiError::NotFound(format!(
                "{} not available for season {}",
                kind.field_name(),
                season_id,
            ))
        }
        LocalArtworkError::Io(error) => {
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

fn is_external_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}
