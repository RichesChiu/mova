use crate::auth::{require_media_file_access, require_user};
use crate::error::ApiError;
use crate::response::{ok, ApiJson, AudioTrackResponse};
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{
        header::{self, HeaderMap, HeaderValue},
        Response, StatusCode,
    },
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use std::{
    io::ErrorKind,
    path::{Path as StdPath, PathBuf},
};
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncSeekExt, SeekFrom},
    process::Command,
};
use tokio_util::io::ReaderStream;

#[derive(Debug, Deserialize, Default)]
pub struct MediaFileStreamQuery {
    pub audio_track_id: Option<i64>,
}

/// 返回某个媒体文件可切换的内嵌音轨列表。
pub async fn list_media_file_audio_tracks(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_file_id): Path<i64>,
) -> Result<ApiJson<Vec<AudioTrackResponse>>, ApiError> {
    let user = require_user(&state, &jar).await?;
    require_media_file_access(&state, &user, media_file_id).await?;
    let audio_tracks = mova_application::list_audio_tracks_for_media_file(&state.db, media_file_id)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(audio_tracks
        .into_iter()
        .map(|audio_track| AudioTrackResponse::from_domain(audio_track, state.api_time_offset))
        .collect()))
}

/// 读取媒体文件内容，支持 HTTP Range 请求，供浏览器视频播放使用。
pub async fn stream_media_file(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_file_id): Path<i64>,
    Query(query): Query<MediaFileStreamQuery>,
    headers: HeaderMap,
) -> Result<Response<Body>, ApiError> {
    let user = require_user(&state, &jar).await?;
    build_media_file_stream_response(state, &user, media_file_id, query.audio_track_id, headers, false).await
}

/// 返回媒体文件的响应头，不返回实体内容。
pub async fn head_media_file(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_file_id): Path<i64>,
    Query(query): Query<MediaFileStreamQuery>,
    headers: HeaderMap,
) -> Result<Response<Body>, ApiError> {
    let user = require_user(&state, &jar).await?;
    build_media_file_stream_response(state, &user, media_file_id, query.audio_track_id, headers, true).await
}

async fn build_media_file_stream_response(
    state: AppState,
    user: &mova_domain::UserProfile,
    media_file_id: i64,
    audio_track_id: Option<i64>,
    headers: HeaderMap,
    head_only: bool,
) -> Result<Response<Body>, ApiError> {
    let media_file = require_media_file_access(&state, user, media_file_id).await?;
    let stream_path = match audio_track_id {
        Some(audio_track_id) => {
            let audio_track = mova_application::get_audio_track(&state.db, audio_track_id)
                .await
                .map_err(ApiError::from)?;

            if audio_track.media_file_id != media_file.id {
                return Err(ApiError::NotFound(format!(
                    "audio track {} does not belong to media file {}",
                    audio_track_id, media_file_id
                )));
            }

            materialize_audio_track_variant(&state, &media_file, &audio_track).await?
        }
        None => PathBuf::from(&media_file.file_path),
    };
    let content_type = content_type_for_media_file(&media_file);

    build_file_stream_response(
        &stream_path,
        content_type,
        headers,
        head_only,
        if audio_track_id.is_some() {
            format!(
                "audio track stream not found on disk for media file {}: {}",
                media_file_id,
                stream_path.display()
            )
        } else {
            format!(
                "media file not found on disk for id {}: {}",
                media_file_id, media_file.file_path
            )
        },
    )
    .await
}

async fn build_file_stream_response(
    file_path: &StdPath,
    content_type: &'static str,
    headers: HeaderMap,
    head_only: bool,
    not_found_message: String,
) -> Result<Response<Body>, ApiError> {
    let metadata = fs::metadata(file_path)
        .await
        .map_err(|error| map_stream_file_io_error(file_path, error, &not_found_message))?;

    if !metadata.is_file() {
        return Err(ApiError::NotFound(format!(
            "media file path is not a regular file: {}",
            file_path.display()
        )));
    }

    let file_size = metadata.len();
    let requested_range = parse_requested_range(headers.get(header::RANGE), file_size)?;

    let (status, start, end) = match requested_range {
        Some(range) => (StatusCode::PARTIAL_CONTENT, range.start, range.end),
        None => {
            if file_size == 0 {
                (StatusCode::OK, 0, 0)
            } else {
                (StatusCode::OK, 0, file_size - 1)
            }
        }
    };

    let content_length = if file_size == 0 { 0 } else { end - start + 1 };
    let body = if head_only || file_size == 0 {
        Body::empty()
    } else {
        let mut file =
            File::open(file_path).await.map_err(|error| map_stream_file_io_error(file_path, error, &not_found_message))?;

        if start > 0 {
            file.seek(SeekFrom::Start(start))
                .await
                .map_err(|error| map_stream_file_io_error(file_path, error, &not_found_message))?;
        }

        let stream = ReaderStream::new(file.take(content_length));
        Body::from_stream(stream)
    };

    let mut response = Response::new(body);
    *response.status_mut() = status;
    let response_headers = response.headers_mut();
    response_headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response_headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&content_length.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );

    if status == StatusCode::PARTIAL_CONTENT {
        response_headers.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, file_size))
                .unwrap_or_else(|_| HeaderValue::from_static("bytes */0")),
        );
    }

    Ok(response)
}

async fn materialize_audio_track_variant(
    state: &AppState,
    media_file: &mova_domain::MediaFile,
    audio_track: &mova_domain::AudioTrack,
) -> Result<PathBuf, ApiError> {
    let cache_dir = state.artwork_cache_dir.join("audio-tracks");
    fs::create_dir_all(&cache_dir)
        .await
        .map_err(|_| ApiError::Internal)?;

    let extension = media_file
        .container
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or("mp4");
    let cache_key = media_file.updated_at.unix_timestamp_nanos();
    let cached_path = cache_dir.join(format!(
        "media-file-{}-audio-track-{}-{}.{}",
        media_file.id, audio_track.id, cache_key, extension
    ));

    if fs::metadata(&cached_path).await.is_ok() {
        return Ok(cached_path);
    }

    let mut command = Command::new("ffmpeg");
    command
        .arg("-nostdin")
        .arg("-y")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(&media_file.file_path)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg(format!("0:{}", audio_track.stream_index))
        .arg("-dn")
        .arg("-c")
        .arg("copy");

    if matches!(
        media_file.container.as_deref(),
        Some("mp4" | "m4v" | "mov")
    ) {
        command.arg("-movflags").arg("+faststart");
    }

    let output = command
        .arg(&cached_path)
        .output()
        .await
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ApiError::Internal
            } else {
                tracing::error!(error = ?error, "failed to spawn ffmpeg audio track remux");
                ApiError::Internal
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        tracing::error!(stderr, audio_track_id = audio_track.id, "ffmpeg audio track remux failed");
        return Err(ApiError::BadRequest(format!(
            "failed to prepare the selected audio track for playback: {}",
            if stderr.is_empty() {
                "ffmpeg remux failed"
            } else {
                &stderr
            }
        )));
    }

    Ok(cached_path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RequestedRange {
    start: u64,
    end: u64,
}

fn parse_requested_range(
    range_header: Option<&HeaderValue>,
    file_size: u64,
) -> Result<Option<RequestedRange>, ApiError> {
    let Some(range_header) = range_header else {
        return Ok(None);
    };

    if file_size == 0 {
        return Err(ApiError::RangeNotSatisfiable {
            message: "range requests are not valid for empty files".to_string(),
            file_size,
        });
    }

    let range_header = range_header
        .to_str()
        .map_err(|_| ApiError::BadRequest("invalid Range header".to_string()))?;

    let Some(range_spec) = range_header.strip_prefix("bytes=") else {
        return Err(ApiError::BadRequest("unsupported Range header".to_string()));
    };

    if range_spec.contains(',') {
        return Err(ApiError::BadRequest(
            "multiple byte ranges are not supported".to_string(),
        ));
    }

    let (start_part, end_part) = range_spec
        .split_once('-')
        .ok_or_else(|| ApiError::BadRequest("invalid Range header".to_string()))?;

    let (start, end) = if start_part.is_empty() {
        let suffix_length = end_part
            .parse::<u64>()
            .map_err(|_| ApiError::BadRequest("invalid Range header".to_string()))?;

        if suffix_length == 0 {
            return Err(ApiError::BadRequest("invalid Range header".to_string()));
        }

        let start = file_size.saturating_sub(suffix_length);
        (start, file_size - 1)
    } else {
        let start = start_part
            .parse::<u64>()
            .map_err(|_| ApiError::BadRequest("invalid Range header".to_string()))?;

        let end = if end_part.is_empty() {
            file_size - 1
        } else {
            end_part
                .parse::<u64>()
                .map_err(|_| ApiError::BadRequest("invalid Range header".to_string()))?
        };

        (start, end.min(file_size - 1))
    };

    if start >= file_size || start > end {
        return Err(ApiError::RangeNotSatisfiable {
            message: "requested byte range is not satisfiable".to_string(),
            file_size,
        });
    }

    Ok(Some(RequestedRange { start, end }))
}

fn content_type_for_media_file(media_file: &mova_domain::MediaFile) -> &'static str {
    match media_file.container.as_deref() {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        Some("avi") => "video/x-msvideo",
        Some("wmv") => "video/x-ms-wmv",
        Some("flv") => "video/x-flv",
        Some("mpeg") | Some("mpg") => "video/mpeg",
        _ => "application/octet-stream",
    }
}

fn map_stream_file_io_error(
    file_path: &StdPath,
    error: std::io::Error,
    not_found_message: &str,
) -> ApiError {
    match error.kind() {
        ErrorKind::NotFound => ApiError::NotFound(not_found_message.to_string()),
        _ => {
            tracing::error!(
                file_path = %file_path.display(),
                error = ?error,
                "failed to access media file on disk"
            );
            ApiError::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_requested_range, RequestedRange};
    use crate::error::ApiError;
    use axum::http::HeaderValue;

    #[test]
    fn parse_requested_range_supports_explicit_start_end() {
        let range = parse_requested_range(Some(&HeaderValue::from_static("bytes=10-19")), 100)
            .unwrap()
            .unwrap();

        assert_eq!(range.start, 10);
        assert_eq!(range.end, 19);
    }

    #[test]
    fn parse_requested_range_supports_open_ended_ranges() {
        let range = parse_requested_range(Some(&HeaderValue::from_static("bytes=50-")), 100)
            .unwrap()
            .unwrap();

        assert_eq!(range, RequestedRange { start: 50, end: 99 });
    }

    #[test]
    fn parse_requested_range_supports_suffix_ranges() {
        let range = parse_requested_range(Some(&HeaderValue::from_static("bytes=-20")), 100)
            .unwrap()
            .unwrap();

        assert_eq!(range, RequestedRange { start: 80, end: 99 });
    }

    #[test]
    fn parse_requested_range_rejects_unsatisfiable_ranges() {
        let error = parse_requested_range(Some(&HeaderValue::from_static("bytes=120-140")), 100)
            .unwrap_err();

        assert!(matches!(
            error,
            ApiError::RangeNotSatisfiable { file_size: 100, .. }
        ));
    }
}
