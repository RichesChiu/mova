use serde::Deserialize;
use std::{io, path::Path, process::Command};

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct MediaProbe {
    pub duration_seconds: Option<i32>,
    pub video_title: Option<String>,
    pub video_codec: Option<String>,
    pub video_profile: Option<String>,
    pub video_level: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bitrate: Option<i64>,
    pub video_bitrate: Option<i64>,
    pub video_frame_rate: Option<f64>,
    pub video_aspect_ratio: Option<String>,
    pub video_scan_type: Option<String>,
    pub video_color_primaries: Option<String>,
    pub video_color_space: Option<String>,
    pub video_color_transfer: Option<String>,
    pub video_bit_depth: Option<i32>,
    pub video_pixel_format: Option<String>,
    pub video_reference_frames: Option<i32>,
    pub audio_streams: Vec<EmbeddedAudioStream>,
    pub subtitle_streams: Vec<EmbeddedSubtitleStream>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmbeddedAudioStream {
    pub stream_index: i32,
    pub language: Option<String>,
    pub audio_codec: Option<String>,
    pub label: Option<String>,
    pub channel_layout: Option<String>,
    pub channels: Option<i32>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i32>,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmbeddedSubtitleStream {
    pub stream_index: i32,
    pub language: Option<String>,
    pub subtitle_format: String,
    pub label: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
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
    profile: Option<String>,
    level: Option<i32>,
    width: Option<i32>,
    height: Option<i32>,
    display_aspect_ratio: Option<String>,
    field_order: Option<String>,
    avg_frame_rate: Option<String>,
    bit_rate: Option<String>,
    sample_rate: Option<String>,
    channels: Option<i32>,
    channel_layout: Option<String>,
    pix_fmt: Option<String>,
    color_space: Option<String>,
    color_transfer: Option<String>,
    color_primaries: Option<String>,
    bits_per_raw_sample: Option<String>,
    bits_per_sample: Option<i32>,
    refs: Option<i32>,
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
    #[serde(default)]
    hearing_impaired: i32,
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
            "format=duration,bit_rate:stream=index,codec_type,codec_name,profile,level,width,height,display_aspect_ratio,field_order,avg_frame_rate,bit_rate,sample_rate,channels,channel_layout,pix_fmt,color_space,color_transfer,color_primaries,bits_per_raw_sample,bits_per_sample,refs:stream_tags=language,title:stream_disposition=default,forced,hearing_impaired",
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
    let audio_streams = parsed
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .filter_map(map_embedded_audio_stream)
        .collect::<Vec<_>>();
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
        video_title: video_stream
            .and_then(|stream| stream.tags.as_ref())
            .and_then(|tags| tags.title.as_ref())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        video_codec: video_stream.and_then(|stream| stream.codec_name.clone()),
        video_profile: video_stream
            .and_then(|stream| stream.profile.as_ref())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        video_level: video_stream.and_then(|stream| {
            stream
                .level
                .and_then(|level| format_video_level(level, stream.codec_name.as_deref()))
        }),
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
        video_bitrate: video_stream
            .and_then(|stream| stream.bit_rate.as_deref())
            .and_then(parse_i64_field),
        video_frame_rate: video_stream
            .and_then(|stream| stream.avg_frame_rate.as_deref())
            .and_then(parse_frame_rate),
        video_aspect_ratio: video_stream
            .and_then(|stream| stream.display_aspect_ratio.as_deref())
            .and_then(normalize_ratio),
        video_scan_type: video_stream
            .and_then(|stream| stream.field_order.as_deref())
            .and_then(normalize_scan_type),
        video_color_primaries: video_stream
            .and_then(|stream| stream.color_primaries.as_ref())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        video_color_space: video_stream
            .and_then(|stream| stream.color_space.as_ref())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        video_color_transfer: video_stream
            .and_then(|stream| stream.color_transfer.as_ref())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        video_bit_depth: video_stream.and_then(resolve_video_bit_depth),
        video_pixel_format: video_stream
            .and_then(|stream| stream.pix_fmt.as_ref())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        video_reference_frames: video_stream.and_then(|stream| stream.refs),
        audio_streams,
        subtitle_streams,
    })
}

fn map_embedded_audio_stream(stream: &FfprobeStream) -> Option<EmbeddedAudioStream> {
    Some(EmbeddedAudioStream {
        stream_index: stream.index?,
        language: stream
            .tags
            .as_ref()
            .and_then(|tags| tags.language.as_ref())
            .and_then(|language| normalize_language_token(language)),
        audio_codec: stream.codec_name.clone(),
        label: stream
            .tags
            .as_ref()
            .and_then(|tags| tags.title.as_ref())
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty()),
        channel_layout: stream
            .channel_layout
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        channels: stream.channels,
        bitrate: stream.bit_rate.as_deref().and_then(parse_i64_field),
        sample_rate: stream.sample_rate.as_deref().and_then(parse_i32_field),
        is_default: stream
            .disposition
            .as_ref()
            .map(|disposition| disposition.default > 0)
            .unwrap_or(false),
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
        is_hearing_impaired: stream
            .disposition
            .as_ref()
            .map(|disposition| disposition.hearing_impaired > 0)
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

fn parse_i32_field(value: &str) -> Option<i32> {
    value.parse::<i32>().ok().filter(|value| *value >= 0)
}

fn parse_i64_field(value: &str) -> Option<i64> {
    value.parse::<i64>().ok().filter(|value| *value >= 0)
}

fn normalize_ratio(value: &str) -> Option<String> {
    let trimmed = value.trim();

    match trimmed {
        "" | "0:1" | "N/A" => None,
        _ => Some(trimmed.to_string()),
    }
}

fn normalize_scan_type(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "unknown" => None,
        "progressive" => Some("Progressive".to_string()),
        "tt" | "bb" | "tb" | "bt" => Some("Interlaced".to_string()),
        other => Some(other.replace('_', " ")),
    }
}

fn format_video_level(level: i32, codec_name: Option<&str>) -> Option<String> {
    if level <= 0 {
        return None;
    }

    match codec_name.unwrap_or_default().to_ascii_lowercase().as_str() {
        "h264" | "avc" | "hevc" | "h265" => {
            let major = level / 10;
            let minor = level % 10;

            if minor == 0 {
                Some(major.to_string())
            } else {
                Some(format!("{major}.{minor}"))
            }
        }
        _ => Some(level.to_string()),
    }
}

fn parse_frame_rate(value: &str) -> Option<f64> {
    let trimmed = value.trim();

    if trimmed.is_empty() || trimmed == "0/0" {
        return None;
    }

    if let Some((numerator, denominator)) = trimmed.split_once('/') {
        let numerator = numerator.trim().parse::<f64>().ok()?;
        let denominator = denominator.trim().parse::<f64>().ok()?;

        if denominator <= 0.0 {
            return None;
        }

        let frame_rate = numerator / denominator;
        return Some((frame_rate * 1000.0).round() / 1000.0);
    }

    trimmed.parse::<f64>().ok()
}

fn resolve_video_bit_depth(stream: &FfprobeStream) -> Option<i32> {
    stream
        .bits_per_raw_sample
        .as_deref()
        .and_then(parse_i32_field)
        .or(stream.bits_per_sample)
        .or_else(|| {
            stream
                .pix_fmt
                .as_deref()
                .and_then(parse_bit_depth_from_pixel_format)
        })
}

fn parse_bit_depth_from_pixel_format(value: &str) -> Option<i32> {
    let marker = value.find('p')?;
    let suffix = &value[(marker + 1)..];
    let digits = suffix
        .chars()
        .take_while(|char| char.is_ascii_digit())
        .collect::<String>();

    if digits.is_empty() {
        return None;
    }

    digits.parse::<i32>().ok()
}
