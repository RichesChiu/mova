use serde::Deserialize;
use std::{io, path::Path, process::Command};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct MediaProbe {
    pub duration_seconds: Option<i32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bitrate: Option<i64>,
    pub subtitle_streams: Vec<EmbeddedSubtitleStream>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmbeddedSubtitleStream {
    pub stream_index: i32,
    pub language: Option<String>,
    pub subtitle_format: String,
    pub label: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeAvailability {
    Unknown,
    Available,
    Unavailable,
}

#[derive(Debug)]
pub(crate) enum ProbeError {
    Unavailable(std::io::Error),
    Io(std::io::Error),
    CommandFailed(String),
    ParseOutput(String),
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(error) | Self::Io(error) => write!(f, "{error}"),
            Self::CommandFailed(message) | Self::ParseOutput(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ProbeError {}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    #[serde(default)]
    streams: Vec<FfprobeStream>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    index: Option<i32>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    bit_rate: Option<String>,
    disposition: Option<FfprobeDisposition>,
    tags: Option<FfprobeStreamTags>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeDisposition {
    #[serde(default)]
    default: i32,
    #[serde(default)]
    forced: i32,
}

#[derive(Debug, Deserialize)]
struct FfprobeStreamTags {
    language: Option<String>,
    title: Option<String>,
}

pub(crate) fn probe_media_file(
    path: &Path,
    probe_availability: &mut ProbeAvailability,
) -> MediaProbe {
    if matches!(probe_availability, ProbeAvailability::Unavailable) {
        return MediaProbe::default();
    }

    match run_ffprobe(path) {
        Ok(probe) => {
            *probe_availability = ProbeAvailability::Available;
            probe
        }
        Err(ProbeError::Unavailable(error)) => {
            tracing::warn!(
                error = %error,
                "ffprobe is not available; media probe fields will remain empty"
            );
            *probe_availability = ProbeAvailability::Unavailable;
            MediaProbe::default()
        }
        Err(error) => {
            tracing::warn!(
                file_path = %path.display(),
                error = %error,
                "failed to probe media file with ffprobe"
            );
            MediaProbe::default()
        }
    }
}

fn run_ffprobe(path: &Path) -> Result<MediaProbe, ProbeError> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg(
            "format=duration,bit_rate:stream=index,codec_type,codec_name,width,height,bit_rate:stream_tags=language,title:stream_disposition=default,forced",
        )
        .arg("-of")
        .arg("json")
        .arg(path)
        .output()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                ProbeError::Unavailable(error)
            } else {
                ProbeError::Io(error)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            format!("ffprobe exited with status {}", output.status)
        } else {
            stderr
        };

        return Err(ProbeError::CommandFailed(message));
    }

    parse_ffprobe_output(&output.stdout)
}

pub(crate) fn parse_ffprobe_output(output: &[u8]) -> Result<MediaProbe, ProbeError> {
    let parsed: FfprobeOutput = serde_json::from_slice(output)
        .map_err(|error| ProbeError::ParseOutput(error.to_string()))?;

    let video_stream = parsed
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("video"));
    let audio_stream = parsed
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"));
    let subtitle_streams = parsed
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("subtitle"))
        .filter_map(map_embedded_subtitle_stream)
        .collect::<Vec<_>>();

    Ok(MediaProbe {
        duration_seconds: parsed
            .format
            .as_ref()
            .and_then(|format| format.duration.as_deref())
            .and_then(parse_duration_seconds),
        video_codec: video_stream.and_then(|stream| stream.codec_name.clone()),
        audio_codec: audio_stream.and_then(|stream| stream.codec_name.clone()),
        width: video_stream.and_then(|stream| stream.width),
        height: video_stream.and_then(|stream| stream.height),
        bitrate: parsed
            .format
            .as_ref()
            .and_then(|format| format.bit_rate.as_deref())
            .and_then(parse_i64_field)
            .or_else(|| {
                video_stream
                    .and_then(|stream| stream.bit_rate.as_deref())
                    .and_then(parse_i64_field)
            }),
        subtitle_streams,
    })
}

fn map_embedded_subtitle_stream(stream: &FfprobeStream) -> Option<EmbeddedSubtitleStream> {
    let stream_index = stream.index?;
    let subtitle_format = normalize_subtitle_codec(stream.codec_name.as_deref()?)?;

    Some(EmbeddedSubtitleStream {
        stream_index,
        language: stream
            .tags
            .as_ref()
            .and_then(|tags| tags.language.as_ref())
            .and_then(|language| normalize_language_token(language)),
        subtitle_format,
        label: stream
            .tags
            .as_ref()
            .and_then(|tags| tags.title.as_ref())
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty()),
        is_default: stream
            .disposition
            .as_ref()
            .map(|disposition| disposition.default > 0)
            .unwrap_or(false),
        is_forced: stream
            .disposition
            .as_ref()
            .map(|disposition| disposition.forced > 0)
            .unwrap_or(false),
    })
}

fn normalize_subtitle_codec(codec_name: &str) -> Option<String> {
    match codec_name.to_ascii_lowercase().as_str() {
        "subrip" | "srt" => Some("srt".to_string()),
        "ass" => Some("ass".to_string()),
        "ssa" => Some("ssa".to_string()),
        "webvtt" => Some("vtt".to_string()),
        "mov_text" => Some("mov_text".to_string()),
        _ => None,
    }
}

fn normalize_language_token(token: &str) -> Option<String> {
    let normalized = token.trim().replace('_', "-").to_ascii_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

fn parse_duration_seconds(value: &str) -> Option<i32> {
    let duration = value.parse::<f64>().ok()?;

    if !duration.is_finite() || duration < 0.0 {
        return None;
    }

    let rounded = duration.round();
    if rounded > i32::MAX as f64 {
        return Some(i32::MAX);
    }

    Some(rounded as i32)
}

fn parse_i64_field(value: &str) -> Option<i64> {
    value.parse::<i64>().ok().filter(|value| *value >= 0)
}
