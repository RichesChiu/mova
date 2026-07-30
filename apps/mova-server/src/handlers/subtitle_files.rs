use crate::auth::{
    require_media_file_access, require_media_file_with_library_access, require_user,
};
use crate::error::ApiError;
use crate::media_path::{resolve_regular_file_within_library, LibraryMediaPathError};
use crate::response::{ok, ApiJson, SubtitleFileResponse};
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        header::{self, HeaderValue},
        HeaderMap, Response, StatusCode,
    },
};
use axum_extra::extract::cookie::CookieJar;
use serde_json::json;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    io,
    path::{Path as FileSystemPath, PathBuf},
    process::{ExitStatus, Stdio},
    time::Duration,
};
use tokio::{
    fs,
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::{Semaphore, SemaphorePermit},
    time::timeout,
};
use tokio_util::io::ReaderStream;

const SUBTITLE_CONVERSION_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const FFMPEG_DIAGNOSTIC_LIMIT: usize = 64 * 1024;
const MAX_SUBTITLE_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_SUBTITLE_VTT_BYTES: usize = 24 * 1024 * 1024;
const MAX_CONCURRENT_SUBTITLE_MATERIALIZATIONS: usize = 4;
const VTT_CONTENT_TYPE: &str = "text/vtt; charset=utf-8";
const VTT_CACHE_CONTROL: &str = "private, max-age=3600";
static SUBTITLE_MATERIALIZATION_PERMITS: Semaphore =
    Semaphore::const_new(MAX_CONCURRENT_SUBTITLE_MATERIALIZATIONS);

/// 返回某个媒体文件可切换的字幕轨道列表。
pub async fn list_media_file_subtitles(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(media_file_id): Path<i64>,
) -> Result<ApiJson<Vec<SubtitleFileResponse>>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    require_media_file_access(&state, &user, media_file_id).await?;
    let subtitles = mova_application::list_subtitle_files_for_media_file(&state.db, media_file_id)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(subtitles
        .into_iter()
        .map(|subtitle| SubtitleFileResponse::from_domain(subtitle, state.api_time_offset))
        .collect()))
}

struct TrustedSubtitleStream {
    subtitle_file: mova_domain::SubtitleFile,
    media_path: PathBuf,
    external_path: Option<PathBuf>,
    cached_path: PathBuf,
}

async fn trusted_subtitle_stream(
    state: &AppState,
    user: &mova_domain::UserProfile,
    subtitle_file_id: i64,
) -> Result<TrustedSubtitleStream, ApiError> {
    let subtitle_file = mova_application::get_subtitle_file(&state.db, subtitle_file_id)
        .await
        .map_err(ApiError::from)?;
    let (media_file, library) =
        require_media_file_with_library_access(state, user, subtitle_file.media_file_id).await?;
    let media_path = resolve_trusted_subtitle_input(
        FileSystemPath::new(&media_file.file_path),
        FileSystemPath::new(&library.root_path),
        media_file.library_id,
        subtitle_file.id,
        "media",
    )
    .await?;
    let external_path = if subtitle_file.source_kind == "external" {
        let source_path = subtitle_file.file_path.as_deref().ok_or_else(|| {
            ApiError::NotFound(format!(
                "subtitle file path missing for {}",
                subtitle_file.id
            ))
        })?;
        Some(
            resolve_trusted_subtitle_input(
                FileSystemPath::new(source_path),
                FileSystemPath::new(&library.root_path),
                media_file.library_id,
                subtitle_file.id,
                "external subtitle",
            )
            .await?,
        )
    } else {
        None
    };
    let cached_path = mova_application::library_subtitle_cache_path(
        &state.cache_dir,
        media_file.library_id,
        subtitle_file.id,
    );

    Ok(TrustedSubtitleStream {
        subtitle_file,
        media_path,
        external_path,
        cached_path,
    })
}

/// 把外挂/内嵌字幕统一转换成 WebVTT，供浏览器自定义播放器挂载。
pub async fn stream_subtitle_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(subtitle_file_id): Path<i64>,
) -> Result<Response<Body>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    let stream = trusted_subtitle_stream(&state, &user, subtitle_file_id).await?;
    let cache_dir = stream.cached_path.parent().ok_or(ApiError::Internal)?;
    fs::create_dir_all(cache_dir)
        .await
        .map_err(|_| ApiError::Internal)?;

    materialize_subtitle_vtt(
        &stream.subtitle_file,
        &stream.media_path,
        stream.external_path.as_deref(),
        &stream.cached_path,
    )
    .await?;

    let (cache_file, payload_length) = open_file_bounded(
        &stream.cached_path,
        MAX_SUBTITLE_VTT_BYTES,
        "subtitle cache not found",
    )
    .await?;

    let stream = ReaderStream::new(cache_file.take(payload_length));
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(VTT_CONTENT_TYPE),
    );
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&payload_length.to_string()).map_err(|_| ApiError::Internal)?,
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(VTT_CACHE_CONTROL),
    );

    Ok(response)
}

/// 返回字幕缓存的准确响应头，不生成或转换字幕。
pub async fn head_subtitle_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(subtitle_file_id): Path<i64>,
) -> Result<Response<Body>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    let stream = trusted_subtitle_stream(&state, &user, subtitle_file_id).await?;

    Ok(build_subtitle_head_response(&stream.cached_path).await)
}

async fn build_subtitle_head_response(cached_path: &FileSystemPath) -> Response<Body> {
    let cached_length = valid_vtt_cache_file_length(cached_path).await;
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(VTT_CONTENT_TYPE),
    );

    match cached_length {
        Some(payload_length) => {
            headers.insert(
                header::CONTENT_LENGTH,
                HeaderValue::from_str(&payload_length.to_string())
                    .unwrap_or_else(|_| HeaderValue::from_static("0")),
            );
            headers.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static(VTT_CACHE_CONTROL),
            );
        }
        None => {
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        }
    }

    response
}

async fn materialize_subtitle_vtt(
    subtitle_file: &mova_domain::SubtitleFile,
    media_file_path: &FileSystemPath,
    external_source_path: Option<&FileSystemPath>,
    output_path: &FileSystemPath,
) -> Result<(), ApiError> {
    // Published cache files are immutable and appear via atomic rename, so this
    // lock-free fast path is safe and does not consume materialization capacity.
    if is_valid_vtt_cache_file(output_path).await {
        return Ok(());
    }

    // Admission happens before the per-key lock. At most the fixed permit count can
    // therefore wait for another request or generate a subtitle; overload fails fast.
    let _materialization_permit =
        try_acquire_subtitle_materialization_permit(&SUBTITLE_MATERIALIZATION_PERMITS)?;
    let _cache_guard = mova_application::lock_cache_path(output_path).await;

    // Another admitted request may have published this cache while this request
    // waited for the per-key lock.
    if is_valid_vtt_cache_file(output_path).await {
        return Ok(());
    }
    if fs::metadata(output_path).await.is_ok() {
        let _ = fs::remove_file(output_path).await;
    }

    let mut temporary_file = mova_application::CacheTempFileGuard::new(output_path);
    let temporary_path = temporary_file.path().to_path_buf();
    let materialization_result = materialize_subtitle_vtt_to_path(
        subtitle_file,
        media_file_path,
        external_source_path,
        &temporary_path,
    )
    .await;
    if let Err(error) = materialization_result {
        let _ = fs::remove_file(&temporary_path).await;
        return Err(error);
    }

    if !is_valid_vtt_cache_file(&temporary_path).await {
        let _ = fs::remove_file(&temporary_path).await;
        tracing::error!(
            subtitle_file_id = subtitle_file.id,
            "generated subtitle cache is not valid WebVTT"
        );
        return Err(ApiError::Internal);
    }

    if let Err(error) = mova_application::commit_cache_file(&temporary_path, output_path).await {
        let _ = fs::remove_file(&temporary_path).await;
        tracing::error!(
            error = ?error,
            subtitle_file_id = subtitle_file.id,
            "failed to publish subtitle cache"
        );
        return Err(ApiError::Internal);
    }
    temporary_file.disarm();

    Ok(())
}

fn try_acquire_subtitle_materialization_permit(
    semaphore: &Semaphore,
) -> Result<SemaphorePermit<'_>, ApiError> {
    semaphore
        .try_acquire()
        .map_err(|_| ApiError::ServiceUnavailable("subtitle materialization is busy".to_string()))
}

async fn materialize_subtitle_vtt_to_path(
    subtitle_file: &mova_domain::SubtitleFile,
    media_file_path: &FileSystemPath,
    external_source_path: Option<&FileSystemPath>,
    output_path: &FileSystemPath,
) -> Result<(), ApiError> {
    if subtitle_file.source_kind == "external" {
        let source_path = external_source_path.ok_or_else(|| {
            ApiError::NotFound(format!(
                "subtitle file path missing for {}",
                subtitle_file.id
            ))
        })?;

        match subtitle_file.subtitle_format.as_str() {
            "vtt" => {
                copy_file_bounded(
                    FileSystemPath::new(source_path),
                    output_path,
                    MAX_SUBTITLE_SOURCE_BYTES.min(MAX_SUBTITLE_VTT_BYTES),
                )
                .await?;
                return Ok(());
            }
            "srt" => {
                let source = read_file_bounded(
                    FileSystemPath::new(source_path),
                    MAX_SUBTITLE_SOURCE_BYTES,
                    "subtitle file not found",
                )
                .await?;
                let source = String::from_utf8(source).map_err(|_| {
                    ApiError::BadRequest("subtitle source is not valid UTF-8".to_string())
                })?;
                let converted = convert_srt_to_vtt(&source)?;
                fs::write(output_path, converted)
                    .await
                    .map_err(map_subtitle_io_error)?;
                return Ok(());
            }
            "ass" | "ssa" => {
                run_ffmpeg_subtitle_conversion(
                    vec![
                        OsString::from("-f"),
                        OsString::from("ass"),
                        OsString::from("-i"),
                        OsString::from("pipe:0"),
                    ],
                    Some(source_path),
                    output_path,
                    "external subtitle conversion",
                )
                .await?;
                return Ok(());
            }
            _ => {}
        }
    }

    let stream_index = subtitle_file.stream_index.ok_or_else(|| {
        ApiError::NotFound(format!(
            "subtitle stream index missing for embedded subtitle {}",
            subtitle_file.id
        ))
    })?;

    run_ffmpeg_subtitle_conversion(
        vec![
            OsString::from("-i"),
            media_file_path.as_os_str().to_owned(),
            OsString::from("-map"),
            OsString::from(format!("0:{stream_index}")),
        ],
        None,
        output_path,
        "embedded subtitle extraction",
    )
    .await
}

async fn run_ffmpeg_subtitle_conversion(
    args: Vec<OsString>,
    input_source: Option<&FileSystemPath>,
    output_path: &FileSystemPath,
    operation: &str,
) -> Result<(), ApiError> {
    let mut command = Command::new("ffmpeg");
    command
        .kill_on_drop(true)
        .arg("-nostdin")
        .arg("-y")
        .arg("-loglevel")
        .arg("error")
        .args(&args)
        .arg("-f")
        .arg("webvtt")
        .arg("pipe:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input_source.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }

    let output = match run_ffmpeg_with_bounded_output(
        &mut command,
        input_source,
        output_path,
        MAX_SUBTITLE_VTT_BYTES,
    )
    .await
    {
        Ok(output) => output,
        Err(SubtitleCommandError::Spawn(error) | SubtitleCommandError::Io(error)) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Err(ApiError::Internal);
            } else {
                tracing::error!(error = ?error, operation, "failed to spawn ffmpeg subtitle conversion");
                return Err(ApiError::Internal);
            }
        }
        Err(SubtitleCommandError::ResourceTooLarge { max_bytes }) => {
            tracing::warn!(
                operation,
                max_bytes,
                "ffmpeg subtitle conversion exceeded a resource limit"
            );
            return Err(subtitle_too_large(max_bytes));
        }
        Err(SubtitleCommandError::TimedOut) => {
            tracing::error!(
                operation,
                timeout_seconds = SUBTITLE_CONVERSION_TIMEOUT.as_secs(),
                "ffmpeg subtitle conversion timed out"
            );
            return Err(ApiError::ServiceUnavailable(
                "subtitle conversion timed out".to_string(),
            ));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        tracing::error!(
            operation,
            stderr,
            stderr_truncated = output.stderr_truncated,
            "ffmpeg subtitle conversion failed"
        );
        return Err(ApiError::BadRequest(format!(
            "failed to convert subtitle for web playback: {}",
            if stderr.is_empty() {
                "ffmpeg conversion failed"
            } else {
                &stderr
            }
        )));
    }

    Ok(())
}

async fn resolve_trusted_subtitle_input(
    input_path: &FileSystemPath,
    library_root: &FileSystemPath,
    library_id: i64,
    subtitle_file_id: i64,
    input_kind: &'static str,
) -> Result<PathBuf, ApiError> {
    match resolve_regular_file_within_library(input_path, library_root).await {
        Ok(canonical_path) => Ok(canonical_path),
        Err(LibraryMediaPathError::LibraryRoot(error)) => {
            tracing::error!(
                error = ?error,
                library_id,
                subtitle_file_id,
                library_root = %library_root.display(),
                "failed to validate media library root before subtitle streaming"
            );
            Err(ApiError::Internal)
        }
        Err(LibraryMediaPathError::MediaSource(error)) => {
            if error.kind() == io::ErrorKind::NotFound {
                Err(ApiError::NotFound(format!("{input_kind} file not found")))
            } else {
                tracing::error!(
                    error = ?error,
                    library_id,
                    subtitle_file_id,
                    input_kind,
                    input_path = %input_path.display(),
                    "failed to validate subtitle input path"
                );
                Err(ApiError::Internal)
            }
        }
        Err(LibraryMediaPathError::OutsideLibraryRoot | LibraryMediaPathError::NotRegularFile) => {
            tracing::warn!(
                library_id,
                subtitle_file_id,
                input_kind,
                input_path = %input_path.display(),
                library_root = %library_root.display(),
                "refused to read an invalid subtitle input path"
            );
            Err(ApiError::NotFound(format!("{input_kind} file not found")))
        }
    }
}

#[derive(Debug)]
struct SubtitleCommandOutput {
    status: ExitStatus,
    stderr: Vec<u8>,
    stderr_truncated: bool,
}

#[derive(Debug)]
enum SubtitleCommandError {
    Spawn(io::Error),
    Io(io::Error),
    ResourceTooLarge { max_bytes: usize },
    TimedOut,
}

async fn run_ffmpeg_with_bounded_output(
    command: &mut Command,
    input_source: Option<&FileSystemPath>,
    output_path: &FileSystemPath,
    output_limit: usize,
) -> Result<SubtitleCommandOutput, SubtitleCommandError> {
    let mut child = command.spawn().map_err(SubtitleCommandError::Spawn)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| SubtitleCommandError::Io(io::Error::other("ffmpeg stdout was not piped")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| SubtitleCommandError::Io(io::Error::other("ffmpeg stderr was not piped")))?;
    let stdin = if input_source.is_some() {
        Some(child.stdin.take().ok_or_else(|| {
            SubtitleCommandError::Io(io::Error::other("ffmpeg stdin was not piped"))
        })?)
    } else {
        None
    };

    let result = timeout(SUBTITLE_CONVERSION_TIMEOUT, async {
        let (status, (), (), (stderr, stderr_truncated)) = tokio::try_join!(
            async { child.wait().await.map_err(SubtitleCommandError::Io) },
            write_ffmpeg_input_bounded(stdin, input_source, MAX_SUBTITLE_SOURCE_BYTES),
            write_stream_bounded(stdout, output_path, output_limit),
            read_stream_bounded(stderr, FFMPEG_DIAGNOSTIC_LIMIT),
        )?;

        Ok::<_, SubtitleCommandError>(SubtitleCommandOutput {
            status,
            stderr,
            stderr_truncated,
        })
    })
    .await;

    match result {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(error)
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(SubtitleCommandError::TimedOut)
        }
    }
}

async fn write_ffmpeg_input_bounded(
    stdin: Option<tokio::process::ChildStdin>,
    input_source: Option<&FileSystemPath>,
    limit: usize,
) -> Result<(), SubtitleCommandError> {
    let (Some(mut stdin), Some(input_source)) = (stdin, input_source) else {
        return Ok(());
    };
    let mut source = fs::File::open(input_source)
        .await
        .map_err(SubtitleCommandError::Io)?;
    let metadata = source.metadata().await.map_err(SubtitleCommandError::Io)?;
    if !metadata.is_file() {
        return Err(SubtitleCommandError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "subtitle input is not a regular file",
        )));
    }
    if metadata.len() > limit as u64 {
        return Err(SubtitleCommandError::ResourceTooLarge { max_bytes: limit });
    }

    let mut written = 0_usize;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = source
            .read(&mut buffer)
            .await
            .map_err(SubtitleCommandError::Io)?;
        if read == 0 {
            break;
        }
        written = written
            .checked_add(read)
            .filter(|total| *total <= limit)
            .ok_or(SubtitleCommandError::ResourceTooLarge { max_bytes: limit })?;
        stdin
            .write_all(&buffer[..read])
            .await
            .map_err(SubtitleCommandError::Io)?;
    }
    stdin.shutdown().await.map_err(SubtitleCommandError::Io)
}

async fn write_stream_bounded(
    mut reader: impl AsyncRead + Unpin,
    output_path: &FileSystemPath,
    limit: usize,
) -> Result<(), SubtitleCommandError> {
    let mut output = fs::File::create(output_path)
        .await
        .map_err(SubtitleCommandError::Io)?;
    let mut written = 0_usize;
    let mut buffer = [0_u8; 16 * 1024];

    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(SubtitleCommandError::Io)?;
        if read == 0 {
            break;
        }
        written = written
            .checked_add(read)
            .filter(|total| *total <= limit)
            .ok_or(SubtitleCommandError::ResourceTooLarge { max_bytes: limit })?;
        output
            .write_all(&buffer[..read])
            .await
            .map_err(SubtitleCommandError::Io)?;
    }

    output.flush().await.map_err(SubtitleCommandError::Io)
}

async fn read_stream_bounded(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
) -> Result<(Vec<u8>, bool), SubtitleCommandError> {
    let mut retained = Vec::with_capacity(limit.min(8192));
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];

    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(SubtitleCommandError::Io)?;
        if read == 0 {
            break;
        }

        let remaining = limit.saturating_sub(retained.len());
        let keep = remaining.min(read);
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep < read;
    }

    Ok((retained, truncated))
}

async fn is_valid_vtt_cache_file(path: &FileSystemPath) -> bool {
    valid_vtt_cache_file_length(path).await.is_some()
}

async fn valid_vtt_cache_file_length(path: &FileSystemPath) -> Option<u64> {
    let Ok(metadata) = fs::metadata(path).await else {
        return None;
    };
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_SUBTITLE_VTT_BYTES as u64
    {
        return None;
    }

    let Ok(mut file) = fs::File::open(path).await else {
        return None;
    };
    let mut header = [0_u8; 10];
    let Ok(read) = file.read(&mut header).await else {
        return None;
    };
    let header = &header[..read];
    (header.starts_with(b"WEBVTT") || header.starts_with(b"\xef\xbb\xbfWEBVTT"))
        .then_some(metadata.len())
}

fn convert_srt_to_vtt(source: &str) -> Result<String, ApiError> {
    convert_srt_to_vtt_with_limit(source, MAX_SUBTITLE_VTT_BYTES)
}

fn convert_srt_to_vtt_with_limit(source: &str, output_limit: usize) -> Result<String, ApiError> {
    let mut output = String::from("WEBVTT\n\n");
    if output.len() > output_limit {
        return Err(subtitle_too_large(output_limit));
    }

    for line in source.lines() {
        let next_length = output
            .len()
            .checked_add(line.len())
            .and_then(|length| length.checked_add(1))
            .ok_or_else(|| subtitle_too_large(output_limit))?;
        if next_length > output_limit {
            return Err(subtitle_too_large(output_limit));
        }

        if line.contains("-->") {
            output.extend(line.chars().map(
                |character| {
                    if character == ',' {
                        '.'
                    } else {
                        character
                    }
                },
            ));
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }

    Ok(output)
}

async fn ensure_file_within_limit(
    path: &FileSystemPath,
    limit: usize,
    not_found_message: &str,
) -> Result<(), ApiError> {
    let metadata = fs::metadata(path)
        .await
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => ApiError::NotFound(not_found_message.to_string()),
            _ => ApiError::Internal,
        })?;
    if !metadata.is_file() {
        return Err(ApiError::NotFound(not_found_message.to_string()));
    }
    if metadata.len() > limit as u64 {
        return Err(subtitle_too_large(limit));
    }
    Ok(())
}

async fn read_file_bounded(
    path: &FileSystemPath,
    limit: usize,
    not_found_message: &str,
) -> Result<Vec<u8>, ApiError> {
    let (file, _) = open_file_bounded(path, limit, not_found_message).await?;
    let mut reader = file.take(limit.saturating_add(1) as u64);
    let mut payload = Vec::new();
    reader
        .read_to_end(&mut payload)
        .await
        .map_err(map_subtitle_io_error)?;
    if payload.len() > limit {
        return Err(subtitle_too_large(limit));
    }
    Ok(payload)
}

async fn open_file_bounded(
    path: &FileSystemPath,
    limit: usize,
    not_found_message: &str,
) -> Result<(fs::File, u64), ApiError> {
    let file = fs::File::open(path)
        .await
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => ApiError::NotFound(not_found_message.to_string()),
            _ => ApiError::Internal,
        })?;
    let metadata = file.metadata().await.map_err(|_| ApiError::Internal)?;
    if !metadata.is_file() {
        return Err(ApiError::NotFound(not_found_message.to_string()));
    }
    if metadata.len() > limit as u64 {
        return Err(subtitle_too_large(limit));
    }
    Ok((file, metadata.len()))
}

async fn copy_file_bounded(
    source_path: &FileSystemPath,
    output_path: &FileSystemPath,
    limit: usize,
) -> Result<(), ApiError> {
    ensure_file_within_limit(source_path, limit, "subtitle file not found").await?;
    let mut source = fs::File::open(source_path)
        .await
        .map_err(map_subtitle_io_error)?;
    let mut output = fs::File::create(output_path)
        .await
        .map_err(map_subtitle_io_error)?;
    let mut copied = 0_usize;
    let mut buffer = [0_u8; 16 * 1024];

    loop {
        let read = source
            .read(&mut buffer)
            .await
            .map_err(map_subtitle_io_error)?;
        if read == 0 {
            break;
        }
        copied = copied
            .checked_add(read)
            .filter(|total| *total <= limit)
            .ok_or_else(|| subtitle_too_large(limit))?;
        output
            .write_all(&buffer[..read])
            .await
            .map_err(map_subtitle_io_error)?;
    }

    output.flush().await.map_err(map_subtitle_io_error)
}

fn subtitle_too_large(max_bytes: usize) -> ApiError {
    ApiError::Business {
        status: StatusCode::PAYLOAD_TOO_LARGE,
        error_code: "subtitle_too_large",
        params: BTreeMap::from([("max_bytes".to_string(), json!(max_bytes))]),
        diagnostic_message: format!("subtitle exceeds the {max_bytes}-byte processing limit"),
    }
}

fn map_subtitle_io_error(error: std::io::Error) -> ApiError {
    match error.kind() {
        std::io::ErrorKind::NotFound => ApiError::NotFound("subtitle file not found".to_string()),
        _ => ApiError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_subtitle_head_response, convert_srt_to_vtt, convert_srt_to_vtt_with_limit,
        is_valid_vtt_cache_file, open_file_bounded, subtitle_too_large,
        try_acquire_subtitle_materialization_permit, write_stream_bounded, SubtitleCommandError,
    };
    use crate::error::ApiError;
    use axum::http::{
        header::{self},
        StatusCode,
    };
    use uuid::Uuid;

    #[test]
    fn convert_srt_to_vtt_rewrites_timestamp_separator() {
        let converted = convert_srt_to_vtt("1\n00:00:00,000 --> 00:00:01,500\nhello\n").unwrap();
        assert!(converted.starts_with("WEBVTT\n\n1\n00:00:00.000 --> 00:00:01.500\nhello"));
    }

    #[test]
    fn srt_conversion_rejects_output_over_the_limit() {
        let error = convert_srt_to_vtt_with_limit("1\nhello\n", 8).unwrap_err();
        assert!(matches!(
            error,
            ApiError::Business {
                error_code: "subtitle_too_large",
                ..
            }
        ));
    }

    #[test]
    fn subtitle_size_error_has_a_stable_code_and_limit() {
        let error = subtitle_too_large(123);
        let ApiError::Business {
            status,
            error_code,
            params,
            ..
        } = error
        else {
            panic!("expected a business error");
        };
        assert_eq!(status, axum::http::StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(error_code, "subtitle_too_large");
        assert_eq!(params.get("max_bytes"), Some(&serde_json::json!(123)));
    }

    #[test]
    fn subtitle_materialization_admission_fails_fast_when_saturated() {
        let semaphore = tokio::sync::Semaphore::new(1);
        let permit = try_acquire_subtitle_materialization_permit(&semaphore).unwrap();

        assert!(matches!(
            try_acquire_subtitle_materialization_permit(&semaphore),
            Err(ApiError::ServiceUnavailable(message))
                if message == "subtitle materialization is busy"
        ));

        drop(permit);
        assert!(try_acquire_subtitle_materialization_permit(&semaphore).is_ok());
    }

    #[tokio::test]
    async fn bounded_output_writer_stops_before_exceeding_the_limit() {
        let root =
            std::env::temp_dir().join(format!("mova-subtitle-output-test-{}", Uuid::new_v4()));
        let output = root.join("bounded.vtt");
        tokio::fs::create_dir_all(&root).await.unwrap();

        let error = write_stream_bounded(std::io::Cursor::new(vec![b'x'; 32]), &output, 16)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            SubtitleCommandError::ResourceTooLarge { max_bytes: 16 }
        ));
        assert!(tokio::fs::metadata(&output).await.unwrap().len() <= 16);

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn bounded_file_open_rejects_an_oversized_response_before_streaming() {
        let root =
            std::env::temp_dir().join(format!("mova-subtitle-response-test-{}", Uuid::new_v4()));
        let output = root.join("oversized.vtt");
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(&output, b"WEBVTT\n\n0123456789")
            .await
            .unwrap();

        let error = open_file_bounded(&output, 16, "not found")
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ApiError::Business {
                error_code: "subtitle_too_large",
                ..
            }
        ));

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn vtt_cache_validation_rejects_partial_or_wrong_content() {
        let root =
            std::env::temp_dir().join(format!("mova-subtitle-cache-test-{}", Uuid::new_v4()));
        let valid = root.join("valid.vtt");
        let invalid = root.join("invalid.vtt");
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(&valid, b"WEBVTT\n\n00:00.000 --> 00:01.000\nhello")
            .await
            .unwrap();
        tokio::fs::write(&invalid, b"partial subtitle")
            .await
            .unwrap();

        assert!(is_valid_vtt_cache_file(&valid).await);
        assert!(!is_valid_vtt_cache_file(&invalid).await);
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn subtitle_head_cache_miss_does_not_create_or_claim_a_resource_length() {
        let root = std::env::temp_dir().join(format!("mova-subtitle-head-miss-{}", Uuid::new_v4()));
        let cached_path = root.join("missing.vtt");

        let response = build_subtitle_head_response(&cached_path).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/vtt; charset=utf-8"
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert!(!response.headers().contains_key(header::CONTENT_LENGTH));
        assert!(!cached_path.exists());
    }

    #[tokio::test]
    async fn subtitle_head_cache_hit_reports_the_exact_vtt_length() {
        let root = std::env::temp_dir().join(format!("mova-subtitle-head-hit-{}", Uuid::new_v4()));
        let cached_path = root.join("cached.vtt");
        let payload = b"WEBVTT\n\n00:00.000 --> 00:01.000\nhello";
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(&cached_path, payload).await.unwrap();

        let response = build_subtitle_head_response(&cached_path).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_LENGTH)
                .unwrap()
                .to_str()
                .unwrap(),
            payload.len().to_string()
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, max-age=3600"
        );
        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
