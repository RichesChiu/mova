use crate::auth::{require_media_file_access, require_user};
use crate::error::ApiError;
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
use std::path::PathBuf;
use tokio::{fs, process::Command};

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

/// 把外挂/内嵌字幕统一转换成 WebVTT，供浏览器自定义播放器挂载。
pub async fn stream_subtitle_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(subtitle_file_id): Path<i64>,
) -> Result<Response<Body>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    let subtitle_file = mova_application::get_subtitle_file(&state.db, subtitle_file_id)
        .await
        .map_err(ApiError::from)?;
    let media_file = require_media_file_access(&state, &user, subtitle_file.media_file_id).await?;
    let cache_dir = state.artwork_cache_dir.join("subtitles");
    fs::create_dir_all(&cache_dir)
        .await
        .map_err(|_| ApiError::Internal)?;
    let cached_path = cache_dir.join(format!("subtitle-{}.vtt", subtitle_file.id));

    if fs::metadata(&cached_path).await.is_err() {
        materialize_subtitle_vtt(&subtitle_file, &media_file.file_path, &cached_path).await?;
    }

    let payload = fs::read(&cached_path)
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => ApiError::NotFound(format!(
                "subtitle cache not found: {}",
                cached_path.display()
            )),
            _ => ApiError::Internal,
        })?;

    let mut response = Response::new(Body::from(payload));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/vtt; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );

    Ok(response)
}

async fn materialize_subtitle_vtt(
    subtitle_file: &mova_domain::SubtitleFile,
    media_file_path: &str,
    output_path: &PathBuf,
) -> Result<(), ApiError> {
    if subtitle_file.source_kind == "external" {
        let source_path = subtitle_file.file_path.as_deref().ok_or_else(|| {
            ApiError::NotFound(format!(
                "subtitle file path missing for {}",
                subtitle_file.id
            ))
        })?;

        match subtitle_file.subtitle_format.as_str() {
            "vtt" => {
                fs::copy(source_path, output_path)
                    .await
                    .map_err(map_subtitle_io_error)?;
                return Ok(());
            }
            "srt" => {
                let source = fs::read_to_string(source_path)
                    .await
                    .map_err(map_subtitle_io_error)?;
                let converted = convert_srt_to_vtt(&source);
                fs::write(output_path, converted)
                    .await
                    .map_err(map_subtitle_io_error)?;
                return Ok(());
            }
            "ass" | "ssa" => {
                run_ffmpeg_subtitle_conversion(
                    vec!["-i".to_string(), source_path.to_string()],
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
            "-i".to_string(),
            media_file_path.to_string(),
            "-map".to_string(),
            format!("0:{stream_index}"),
        ],
        output_path,
        "embedded subtitle extraction",
    )
    .await
}

async fn run_ffmpeg_subtitle_conversion(
    args: Vec<String>,
    output_path: &PathBuf,
    operation: &str,
) -> Result<(), ApiError> {
    let output = Command::new("ffmpeg")
        .arg("-nostdin")
        .arg("-y")
        .arg("-loglevel")
        .arg("error")
        .args(&args)
        .arg("-f")
        .arg("webvtt")
        .arg(output_path)
        .output()
        .await
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ApiError::Internal
            } else {
                tracing::error!(error = ?error, operation, "failed to spawn ffmpeg subtitle conversion");
                ApiError::Internal
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        tracing::error!(operation, stderr, "ffmpeg subtitle conversion failed");
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

fn convert_srt_to_vtt(source: &str) -> String {
    let mut output = String::from("WEBVTT\n\n");
    for line in source.lines() {
        if line.contains("-->") {
            output.push_str(&line.replace(',', "."));
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    output
}

fn map_subtitle_io_error(error: std::io::Error) -> ApiError {
    match error.kind() {
        std::io::ErrorKind::NotFound => ApiError::NotFound("subtitle file not found".to_string()),
        _ => ApiError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::convert_srt_to_vtt;

    #[test]
    fn convert_srt_to_vtt_rewrites_timestamp_separator() {
        let converted = convert_srt_to_vtt("1\n00:00:00,000 --> 00:00:01,500\nhello\n");
        assert!(converted.starts_with("WEBVTT\n\n1\n00:00:00.000 --> 00:00:01.500\nhello"));
    }
}
