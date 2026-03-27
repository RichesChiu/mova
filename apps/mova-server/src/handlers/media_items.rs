use crate::auth::{require_admin, require_media_item_access, require_user};
use crate::error::ApiError;
use crate::response::{
    ok, ApiJson, MediaFileResponse, MediaItemDetailResponse, MediaItemPlaybackHeaderResponse,
    MediaItemResponse, MetadataMatchCandidateResponse, SeasonResponse,
    SeriesEpisodeOutlineResponse,
};
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{
        header::{self, HeaderValue},
        Response, StatusCode,
    },
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use std::{io::ErrorKind, path::Path as FsPath};

#[derive(Debug, Deserialize)]
pub struct SearchMediaItemMetadataQuery {
    pub query: String,
    pub year: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ApplyMediaItemMetadataMatchRequest {
    pub provider_item_id: i64,
}

/// 查询单个媒体条目详情。
pub async fn get_media_item(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
) -> Result<ApiJson<MediaItemDetailResponse>, ApiError> {
    let user = require_user(&state, &jar).await?;
    let media_item = require_media_item_access(&state, &user, media_item_id).await?;
    let cast = mova_application::list_media_item_cast(
        &state.db,
        &media_item,
        state.metadata_provider.clone(),
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(MediaItemDetailResponse::from_domain(
        media_item,
        cast,
        state.api_time_offset,
    )))
}

pub async fn get_media_item_playback_header(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
) -> Result<ApiJson<MediaItemPlaybackHeaderResponse>, ApiError> {
    let user = require_user(&state, &jar).await?;
    require_media_item_access(&state, &user, media_item_id).await?;
    let playback_header =
        mova_application::get_media_item_playback_header(&state.db, media_item_id)
            .await
            .map_err(ApiError::from)?;

    Ok(ok(MediaItemPlaybackHeaderResponse::from_domain(
        playback_header,
    )))
}

/// 查询某个媒体条目关联的文件列表。
pub async fn list_media_item_files(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
) -> Result<ApiJson<Vec<MediaFileResponse>>, ApiError> {
    let user = require_user(&state, &jar).await?;
    require_media_item_access(&state, &user, media_item_id).await?;
    let media_files = mova_application::list_media_files_for_media_item(&state.db, media_item_id)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(media_files
        .into_iter()
        .map(|media_file| MediaFileResponse::from_domain(media_file, state.api_time_offset))
        .collect()))
}

/// 查询某个剧集媒体条目下的季列表。
pub async fn list_media_item_seasons(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
) -> Result<ApiJson<Vec<SeasonResponse>>, ApiError> {
    let user = require_user(&state, &jar).await?;
    require_media_item_access(&state, &user, media_item_id).await?;
    let seasons = mova_application::list_seasons_for_series(&state.db, media_item_id)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(seasons
        .into_iter()
        .map(|season| SeasonResponse::from_domain(season, state.api_time_offset))
        .collect()))
}

/// 查询剧集媒体条目的“全集大纲 + 本地可用性”。
/// 远端元数据可用时返回全季全集，不可用时退化为本地已入库集数。
pub async fn get_media_item_episode_outline(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
) -> Result<ApiJson<SeriesEpisodeOutlineResponse>, ApiError> {
    let user = require_user(&state, &jar).await?;
    require_media_item_access(&state, &user, media_item_id).await?;
    let outline = mova_application::series_episode_outline_for_media_item(
        &state.db,
        user.user.id,
        media_item_id,
        state.metadata_provider.clone(),
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(SeriesEpisodeOutlineResponse::from_domain(outline)))
}

/// 管理员手动搜索单条媒体的候选元数据。
/// 这里返回的是候选列表，真正替换要走单独的 match 接口。
pub async fn search_media_item_metadata(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
    Query(query): Query<SearchMediaItemMetadataQuery>,
) -> Result<ApiJson<Vec<MetadataMatchCandidateResponse>>, ApiError> {
    let user = require_admin(&state, &jar).await?;
    require_media_item_access(&state, &user, media_item_id).await?;
    let results = mova_application::search_media_item_metadata_matches(
        &state.db,
        media_item_id,
        mova_application::SearchMetadataMatchesInput {
            query: query.query,
            year: query.year,
        },
        state.metadata_provider.clone(),
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(results
        .into_iter()
        .map(MetadataMatchCandidateResponse::from_domain)
        .collect()))
}

/// 管理员确认候选后，把选中的外部元数据绑定到当前媒体条目。
pub async fn apply_media_item_metadata_match(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
    Json(request): Json<ApplyMediaItemMetadataMatchRequest>,
) -> Result<ApiJson<MediaItemResponse>, ApiError> {
    let user = require_admin(&state, &jar).await?;
    let media_item = require_media_item_access(&state, &user, media_item_id).await?;
    ensure_metadata_mutation_allowed(&state, media_item.library_id)?;

    let matched = mova_application::apply_media_item_metadata_match(
        &state.db,
        media_item_id,
        mova_application::ApplyMetadataMatchInput {
            provider_item_id: request.provider_item_id,
        },
        state.metadata_provider.clone(),
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(MediaItemResponse::from_domain(
        matched,
        state.api_time_offset,
    )))
}

/// 手动刷新单个媒体条目的 metadata。
/// 当前会重新读取本地 sidecar，并在可用时补拉 TMDB 元数据。
pub async fn refresh_media_item_metadata(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
) -> Result<ApiJson<MediaItemResponse>, ApiError> {
    let user = require_admin(&state, &jar).await?;
    let media_item = require_media_item_access(&state, &user, media_item_id).await?;
    ensure_metadata_mutation_allowed(&state, media_item.library_id)?;

    let refreshed = mova_application::refresh_media_item_metadata(
        &state.db,
        media_item_id,
        state.artwork_cache_dir.clone(),
        state.metadata_provider.clone(),
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(MediaItemResponse::from_domain(
        refreshed,
        state.api_time_offset,
    )))
}

fn ensure_metadata_mutation_allowed(state: &AppState, library_id: i64) -> Result<(), ApiError> {
    if state.scan_registry.is_deleting(library_id) {
        return Err(ApiError::Conflict(format!(
            "library {} is being deleted",
            library_id
        )));
    }

    if let Some(active_scan) = state.scan_registry.active_scan(library_id) {
        return Err(ApiError::Conflict(format!(
            "library {} is being scanned by job {}",
            library_id,
            active_scan.scan_job_id()
        )));
    }

    Ok(())
}

/// 返回媒体条目的海报图片内容。
pub async fn get_media_item_poster(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
) -> Result<Response<Body>, ApiError> {
    let user = require_user(&state, &jar).await?;
    serve_media_item_artwork(state, &user, media_item_id, ArtworkKind::Poster).await
}

/// 返回媒体条目的背景图内容。
pub async fn get_media_item_backdrop(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
) -> Result<Response<Body>, ApiError> {
    let user = require_user(&state, &jar).await?;
    serve_media_item_artwork(state, &user, media_item_id, ArtworkKind::Backdrop).await
}

#[derive(Debug, Clone, Copy)]
enum ArtworkKind {
    Poster,
    Backdrop,
}

impl ArtworkKind {
    fn field_name(self) -> &'static str {
        match self {
            Self::Poster => "poster",
            Self::Backdrop => "backdrop",
        }
    }
}

async fn serve_media_item_artwork(
    state: AppState,
    user: &mova_domain::UserProfile,
    media_item_id: i64,
    kind: ArtworkKind,
) -> Result<Response<Body>, ApiError> {
    let media_item = require_media_item_access(&state, user, media_item_id).await?;

    let artwork_path = match kind {
        ArtworkKind::Poster => media_item.poster_path.as_deref(),
        ArtworkKind::Backdrop => media_item.backdrop_path.as_deref(),
    }
    .ok_or_else(|| {
        ApiError::NotFound(format!(
            "{} not available for media item {}",
            kind.field_name(),
            media_item_id
        ))
    })?;

    if is_external_url(artwork_path) {
        return Err(ApiError::BadRequest(format!(
            "{} for media item {} is stored as a remote URL and should be requested directly",
            kind.field_name(),
            media_item_id
        )));
    }

    let metadata = tokio::fs::metadata(artwork_path)
        .await
        .map_err(|error| map_media_artwork_io_error(kind, media_item_id, artwork_path, error))?;

    if !metadata.is_file() {
        return Err(ApiError::NotFound(format!(
            "{} path is not a regular file for media item {}: {}",
            kind.field_name(),
            media_item_id,
            artwork_path
        )));
    }

    let file_bytes = tokio::fs::read(artwork_path)
        .await
        .map_err(|error| map_media_artwork_io_error(kind, media_item_id, artwork_path, error))?;
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

    Ok(response)
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

fn map_media_artwork_io_error(
    kind: ArtworkKind,
    media_item_id: i64,
    artwork_path: &str,
    error: std::io::Error,
) -> ApiError {
    match error.kind() {
        ErrorKind::NotFound => ApiError::NotFound(format!(
            "{} file not found for media item {}: {}",
            kind.field_name(),
            media_item_id,
            artwork_path
        )),
        _ => {
            tracing::error!(
                media_item_id,
                artwork_path,
                error = ?error,
                "failed to access media artwork on disk"
            );
            ApiError::Internal
        }
    }
}

fn is_external_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}
