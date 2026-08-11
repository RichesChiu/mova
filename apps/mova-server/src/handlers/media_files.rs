use crate::audio_track_cache::{
    audio_track_remux_output_limit, cache_artifact_is_usable, generated_artifact_size_is_complete,
    reserve_audio_track_cache, try_admit_audio_track_remux,
};
use crate::auth::{
    require_media_file_access, require_media_file_with_library_access, AuthenticatedUser,
};
use crate::bounded_process::{run_with_bounded_stderr, BoundedCommandError};
use crate::error::ApiError;
use crate::media_path::{resolve_regular_file_within_library, LibraryMediaPathError};
use crate::response::{ok, ApiJson, AudioTrackResponse};
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{
        header::{self, HeaderMap, HeaderName, HeaderValue},
        Response, StatusCode,
    },
};
use mova_domain::MediaSourceKind;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::ErrorKind,
    path::{Path as StdPath, PathBuf},
    time::Duration,
};
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncSeekExt, SeekFrom},
    process::Command,
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::io::ReaderStream;

const AUDIO_TRACK_REMUX_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const AUDIO_TRACK_CACHE_KEY_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const AUDIO_TRACK_CACHE_VERSION: u8 = 2;
const FFMPEG_DIAGNOSTIC_LIMIT: usize = 64 * 1024;

#[derive(Debug, Deserialize, Default)]
pub struct MediaFileStreamQuery {
    pub audio_track_id: Option<i64>,
}

/// 返回某个媒体文件可切换的内嵌音轨列表。
pub async fn list_media_file_audio_tracks(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Path(media_file_id): Path<i64>,
) -> Result<ApiJson<Vec<AudioTrackResponse>>, ApiError> {
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
    AuthenticatedUser(user): AuthenticatedUser,
    Path(media_file_id): Path<i64>,
    Query(query): Query<MediaFileStreamQuery>,
    headers: HeaderMap,
) -> Result<Response<Body>, ApiError> {
    build_media_file_stream_response(
        state,
        &user,
        media_file_id,
        query.audio_track_id,
        headers,
        false,
    )
    .await
}

/// 返回媒体文件的响应头，不返回实体内容。
pub async fn head_media_file(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Path(media_file_id): Path<i64>,
    Query(query): Query<MediaFileStreamQuery>,
    headers: HeaderMap,
) -> Result<Response<Body>, ApiError> {
    build_media_file_stream_response(
        state,
        &user,
        media_file_id,
        query.audio_track_id,
        headers,
        true,
    )
    .await
}

async fn build_media_file_stream_response(
    state: AppState,
    user: &mova_domain::UserProfile,
    media_file_id: i64,
    audio_track_id: Option<i64>,
    headers: HeaderMap,
    head_only: bool,
) -> Result<Response<Body>, ApiError> {
    let (media_file, library) =
        require_media_file_with_library_access(&state, user, media_file_id).await?;
    if media_file.source_kind == MediaSourceKind::Strm {
        if audio_track_id.is_some() {
            return Err(ApiError::from(
                mova_application::ApplicationError::validation(
                    "strm_audio_track_selection_unsupported",
                    BTreeMap::new(),
                    "STRM media does not support embedded audio-track selection",
                ),
            ));
        }

        let carrier_path = resolve_trusted_media_source(&media_file, &library.root_path).await?;
        let method = if head_only {
            mova_application::RemoteStreamMethod::Head
        } else {
            mova_application::RemoteStreamMethod::Get
        };
        let range = remote_request_header(&headers, header::RANGE, "Range")?;
        let if_range = remote_request_header(&headers, header::IF_RANGE, "If-Range")?;
        let request = mova_application::RemoteStreamRequest::new(method, range, if_range)
            .map_err(ApiError::from)?;
        let upstream = state
            .strm_streaming
            .open_carrier(&carrier_path, user.user.id, request)
            .await
            .map_err(ApiError::from)?;
        return build_remote_stream_response(upstream);
    }

    let source_path = resolve_trusted_media_source(&media_file, &library.root_path).await?;
    let content_type = content_type_for_media_file(&media_file);
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

            if head_only {
                let cached_path = audio_track_variant_cache_path(&state, &media_file, &audio_track);
                if cache_artifact_is_usable(&cached_path)
                    .await
                    .map_err(|error| {
                        tracing::error!(
                            error = ?error,
                            cache_path = %cached_path.display(),
                            "failed to inspect audio-track cache artifact for HEAD request"
                        );
                        ApiError::Internal
                    })?
                {
                    cached_path
                } else {
                    return Ok(build_unmaterialized_audio_track_head_response(content_type));
                }
            } else {
                materialize_audio_track_variant(&state, &media_file, &audio_track, &source_path)
                    .await?
            }
        }
        None => source_path,
    };

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

fn remote_request_header<'a>(
    headers: &'a HeaderMap,
    name: HeaderName,
    display_name: &str,
) -> Result<Option<&'a str>, ApiError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(ApiError::BadRequest(format!(
            "multiple {display_name} headers are not supported"
        )));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| ApiError::BadRequest(format!("invalid {display_name} header")))
        })
        .transpose()
}

fn build_remote_stream_response(
    upstream: mova_application::RemoteStreamResponse,
) -> Result<Response<Body>, ApiError> {
    let status = StatusCode::from_u16(upstream.status()).map_err(|_| ApiError::Internal)?;
    let safe_headers = upstream.headers().to_vec();
    let body = match upstream.into_body() {
        Some(mut upstream_body) => {
            let (sender, receiver) = tokio::sync::mpsc::channel(1);
            tokio::spawn(async move {
                loop {
                    let next_chunk = tokio::select! {
                        biased;
                        _ = sender.closed() => break,
                        next_chunk = upstream_body.next_chunk() => next_chunk,
                    };
                    match next_chunk {
                        Ok(Some(chunk)) => {
                            if sender.send(Ok(chunk)).await.is_err() {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(failure) => {
                            let diagnostics = upstream_body.diagnostics();
                            tracing::warn!(
                                scheme = diagnostics.scheme(),
                                host_fingerprint = diagnostics.host_fingerprint(),
                                port = diagnostics.port(),
                                reference_hash_prefix = diagnostics.reference_hash_prefix(),
                                failure = failure.as_str(),
                                "remote STRM response body ended before completion"
                            );
                            let _ = sender
                                .send(Err(std::io::Error::other("remote media stream failed")))
                                .await;
                            break;
                        }
                    }
                }
            });
            Body::from_stream(ReceiverStream::new(receiver))
        }
        None => Body::empty(),
    };

    let mut response = Response::new(body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    for safe_header in safe_headers {
        let name = HeaderName::from_static(safe_header.name().as_str());
        let value = HeaderValue::from_bytes(safe_header.value()).map_err(|_| ApiError::Internal)?;
        headers.insert(name, value);
    }
    apply_remote_response_security_headers(headers);
    Ok(response)
}

fn apply_remote_response_security_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
}

fn build_unmaterialized_audio_track_head_response(content_type: &'static str) -> Response<Body> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("none"));
    response
}

async fn resolve_trusted_media_source(
    media_file: &mova_domain::MediaFile,
    library_root: &str,
) -> Result<PathBuf, ApiError> {
    let not_found_message = format!("media file not found on disk for id {}", media_file.id);
    let source_path = StdPath::new(&media_file.file_path);
    match resolve_regular_file_within_library(source_path, StdPath::new(library_root)).await {
        Ok(canonical_source) => Ok(canonical_source),
        Err(LibraryMediaPathError::LibraryRoot(error)) => {
            tracing::error!(
                media_file_id = media_file.id,
                library_id = media_file.library_id,
                library_root,
                error = ?error,
                "failed to validate media library root before streaming"
            );
            Err(ApiError::Internal)
        }
        Err(LibraryMediaPathError::MediaSource(error)) => Err(map_stream_file_io_error(
            source_path,
            error,
            &not_found_message,
        )),
        Err(LibraryMediaPathError::OutsideLibraryRoot | LibraryMediaPathError::NotRegularFile) => {
            tracing::warn!(
                media_file_id = media_file.id,
                library_id = media_file.library_id,
                file_path = %source_path.display(),
                library_root,
                "refused to stream an invalid media file path"
            );
            Err(ApiError::NotFound(not_found_message))
        }
    }
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
        let mut file = File::open(file_path)
            .await
            .map_err(|error| map_stream_file_io_error(file_path, error, &not_found_message))?;

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
    source_path: &StdPath,
) -> Result<PathBuf, ApiError> {
    let cached_path = audio_track_variant_cache_path(state, media_file, audio_track);
    if cache_artifact_is_usable(&cached_path)
        .await
        .map_err(|error| {
            tracing::error!(
                error = ?error,
                cache_path = %cached_path.display(),
                "failed to inspect audio-track cache artifact"
            );
            ApiError::Internal
        })?
    {
        return Ok(cached_path);
    }

    let admission = try_admit_audio_track_remux().map_err(|error| {
        if error.kind() == std::io::ErrorKind::WouldBlock {
            tracing::debug!(
                audio_track_id = audio_track.id,
                "rejected audio-track remux because process capacity is busy"
            );
        } else {
            tracing::error!(
                error = ?error,
                audio_track_id = audio_track.id,
                "failed to acquire audio-track remux admission"
            );
        }
        ApiError::ServiceUnavailable("audio track preparation is busy".to_string())
    })?;
    let _cache_guard = tokio::time::timeout(
        AUDIO_TRACK_CACHE_KEY_WAIT_TIMEOUT,
        mova_application::lock_cache_path(&cached_path),
    )
    .await
    .map_err(|_| {
        tracing::debug!(
            audio_track_id = audio_track.id,
            wait_seconds = AUDIO_TRACK_CACHE_KEY_WAIT_TIMEOUT.as_secs(),
            "timed out waiting for the same audio-track cache key"
        );
        ApiError::ServiceUnavailable("audio track preparation is already in progress".to_string())
    })?;

    // A request that owned this key may have published while we waited.
    if cache_artifact_is_usable(&cached_path)
        .await
        .map_err(|error| {
            tracing::error!(
                error = ?error,
                cache_path = %cached_path.display(),
                "failed to recheck audio-track cache artifact"
            );
            ApiError::Internal
        })?
    {
        return Ok(cached_path);
    }

    let cache_dir = cached_path.parent().ok_or(ApiError::Internal)?;
    fs::create_dir_all(cache_dir)
        .await
        .map_err(|_| ApiError::Internal)?;
    if fs::metadata(&cached_path).await.is_ok() {
        let _ = fs::remove_file(&cached_path).await;
    }

    let source_size = fs::metadata(source_path)
        .await
        .map_err(|error| {
            tracing::error!(
                error = ?error,
                media_file_id = media_file.id,
                "failed to inspect media before audio-track remux"
            );
            ApiError::Internal
        })?
        .len();
    let output_limit = audio_track_remux_output_limit(source_size).ok_or_else(|| {
        tracing::warn!(
            audio_track_id = audio_track.id,
            source_bytes = source_size,
            "refused audio-track remux whose safety bound exceeds the artifact limit"
        );
        ApiError::BadRequest(
            "the selected media file exceeds the server audio-track cache limit".to_string(),
        )
    })?;
    let mut temporary_file = mova_application::CacheTempFileGuard::new(&cached_path);
    let temporary_path = temporary_file.path().to_path_buf();
    let _cache_reservation =
        reserve_audio_track_cache(&state.cache_dir, &temporary_path, output_limit, admission)
            .await
            .map_err(|error| {
                tracing::error!(
                    error = ?error,
                    audio_track_id = audio_track.id,
                    reserved_bytes = output_limit,
                    "failed to reserve audio-track cache capacity"
                );
                ApiError::ServiceUnavailable(
                    "audio track cache capacity is unavailable".to_string(),
                )
            })?;

    let mut command = Command::new("ffmpeg");
    command
        .kill_on_drop(true)
        .arg("-nostdin")
        .arg("-y")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(source_path)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg(format!("0:{}", audio_track.stream_index))
        .arg("-dn")
        .arg("-c")
        .arg("copy")
        .arg("-fs")
        .arg(output_limit.to_string());

    if matches!(media_file.container.as_deref(), Some("mp4" | "m4v" | "mov")) {
        command.arg("-movflags").arg("+faststart");
    }

    let output = match run_with_bounded_stderr(
        command.arg(&temporary_path),
        AUDIO_TRACK_REMUX_TIMEOUT,
        FFMPEG_DIAGNOSTIC_LIMIT,
    )
    .await
    {
        Ok(output) => output,
        Err(BoundedCommandError::Spawn(error) | BoundedCommandError::Io(error)) => {
            let _ = fs::remove_file(&temporary_path).await;
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::error!(error = ?error, "failed to spawn ffmpeg audio track remux");
            }
            return Err(ApiError::Internal);
        }
        Err(BoundedCommandError::TimedOut) => {
            let _ = fs::remove_file(&temporary_path).await;
            tracing::error!(
                timeout_seconds = AUDIO_TRACK_REMUX_TIMEOUT.as_secs(),
                audio_track_id = audio_track.id,
                "ffmpeg audio track remux timed out"
            );
            return Err(ApiError::ServiceUnavailable(
                "audio track preparation timed out".to_string(),
            ));
        }
    };

    if !output.status.success() {
        let _ = fs::remove_file(&temporary_path).await;
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        tracing::error!(
            stderr,
            stderr_truncated = output.stderr_truncated,
            audio_track_id = audio_track.id,
            "ffmpeg audio track remux failed"
        );
        return Err(ApiError::BadRequest(format!(
            "failed to prepare the selected audio track for playback: {}",
            if stderr.is_empty() {
                "ffmpeg remux failed"
            } else {
                &stderr
            }
        )));
    }

    let generated_metadata = fs::metadata(&temporary_path).await.map_err(|error| {
        tracing::error!(
            error = ?error,
            audio_track_id = audio_track.id,
            "failed to inspect generated audio-track cache artifact"
        );
        ApiError::Internal
    })?;
    if !generated_metadata.is_file() || generated_metadata.len() == 0 {
        let _ = fs::remove_file(&temporary_path).await;
        tracing::error!(
            audio_track_id = audio_track.id,
            "ffmpeg produced an invalid audio-track cache artifact"
        );
        return Err(ApiError::Internal);
    }
    // FFmpeg may exit successfully after `-fs` stops a mux. Reaching the
    // boundary therefore cannot prove completeness and must not be published.
    if !generated_artifact_size_is_complete(generated_metadata.len(), output_limit) {
        let _ = fs::remove_file(&temporary_path).await;
        tracing::warn!(
            audio_track_id = audio_track.id,
            generated_bytes = generated_metadata.len(),
            max_bytes = output_limit,
            "rejected oversized audio-track cache artifact"
        );
        return Err(ApiError::BadRequest(
            "the selected audio-track variant exceeds the server cache limit".to_string(),
        ));
    }

    if let Err(error) = mova_application::commit_cache_file(&temporary_path, &cached_path).await {
        let _ = fs::remove_file(&temporary_path).await;
        tracing::error!(
            error = ?error,
            audio_track_id = audio_track.id,
            "failed to publish remuxed audio track cache"
        );
        return Err(ApiError::Internal);
    }
    temporary_file.disarm();

    Ok(cached_path)
}

fn audio_track_variant_cache_path(
    state: &AppState,
    media_file: &mova_domain::MediaFile,
    audio_track: &mova_domain::AudioTrack,
) -> PathBuf {
    let cache_dir =
        mova_application::library_audio_track_cache_dir(&state.cache_dir, media_file.library_id);
    let extension = media_file
        .container
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or("mp4");
    let cache_key = media_file.updated_at.unix_timestamp_nanos();
    cache_dir.join(format!(
        "v{}-media-file-{}-audio-track-{}-{}.{}",
        AUDIO_TRACK_CACHE_VERSION, media_file.id, audio_track.id, cache_key, extension
    ))
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
    use super::{
        apply_remote_response_security_headers, build_file_stream_response,
        build_unmaterialized_audio_track_head_response, parse_requested_range,
        remote_request_header, RequestedRange,
    };
    use crate::error::ApiError;
    use axum::http::{
        header::{self, HeaderMap},
        HeaderValue, StatusCode,
    };
    use uuid::Uuid;

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

    #[test]
    fn remote_headers_reject_duplicate_values_before_contacting_upstream() {
        let mut headers = HeaderMap::new();
        headers.append(header::RANGE, HeaderValue::from_static("bytes=0-9"));
        headers.append(header::RANGE, HeaderValue::from_static("bytes=20-29"));

        let error = remote_request_header(&headers, header::RANGE, "Range").unwrap_err();
        assert!(matches!(error, ApiError::BadRequest(_)));
    }

    #[test]
    fn remote_stream_responses_disable_caching_and_content_sniffing() {
        let mut headers = HeaderMap::new();
        apply_remote_response_security_headers(&mut headers);

        assert_eq!(
            headers.get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        assert_eq!(
            headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );
    }

    #[test]
    fn unmaterialized_audio_track_head_does_not_claim_a_resource_length() {
        let response = build_unmaterialized_audio_track_head_response("video/mp4");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "video/mp4"
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert_eq!(
            response.headers().get(header::ACCEPT_RANGES).unwrap(),
            "none"
        );
        assert!(!response.headers().contains_key(header::CONTENT_LENGTH));
        assert!(!response.headers().contains_key(header::CONTENT_RANGE));
    }

    #[tokio::test]
    async fn materialized_audio_track_head_reports_the_cached_resource_length() {
        let root = std::env::temp_dir().join(format!("mova-audio-head-test-{}", Uuid::new_v4()));
        let cached_path = root.join("variant.mp4");
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(&cached_path, vec![b'x'; 37])
            .await
            .unwrap();

        let response = build_file_stream_response(
            &cached_path,
            "video/mp4",
            HeaderMap::new(),
            true,
            "missing audio cache".to_string(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_LENGTH).unwrap(),
            "37"
        );
        assert_eq!(
            response.headers().get(header::ACCEPT_RANGES).unwrap(),
            "bytes"
        );
        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
