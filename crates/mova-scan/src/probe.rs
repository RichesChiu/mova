use serde::Deserialize;
use std::{
    io::{self, Read},
    path::Path,
    process::{Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const FFPROBE_TIMEOUT: Duration = Duration::from_secs(90);
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(25);
const FFPROBE_STDOUT_LIMIT: usize = 8 * 1024 * 1024;
const FFPROBE_STDERR_LIMIT: usize = 256 * 1024;

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct MediaProbe {
    pub error: Option<String>,
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
    pub technical_tags: Vec<String>,
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
    TimedOut(Duration),
    OutputTooLarge,
    Cancelled,
    CommandFailed(String),
    ParseOutput(String),
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(error) | Self::Io(error) => write!(f, "{error}"),
            Self::TimedOut(timeout) => write!(
                f,
                "ffprobe exceeded the {} second timeout",
                timeout.as_secs()
            ),
            Self::OutputTooLarge => write!(f, "ffprobe output exceeded the safety limit"),
            Self::Cancelled => write!(f, "scan cancelled"),
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
    codec_long_name: Option<String>,
    codec_tag_string: Option<String>,
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
    side_data_list: Option<Vec<FfprobeSideData>>,
    disposition: Option<FfprobeDisposition>,
    tags: Option<FfprobeStreamTags>,
}

#[derive(Debug, Deserialize)]
struct FfprobeSideData {
    side_data_type: Option<String>,
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

pub(crate) fn probe_media_file_with_cancel(
    path: &Path,
    probe_availability: &mut ProbeAvailability,
    should_cancel: &mut impl FnMut() -> bool,
) -> io::Result<MediaProbe> {
    if should_cancel() {
        return Err(scan_cancelled_error());
    }

    if matches!(probe_availability, ProbeAvailability::Unavailable) {
        return Ok(MediaProbe::default());
    }

    match run_ffprobe(path, should_cancel) {
        Ok(probe) => {
            *probe_availability = ProbeAvailability::Available;
            Ok(probe)
        }
        Err(ProbeError::Unavailable(error)) => {
            let detail = error.to_string();
            tracing::warn!(
                error = %error,
                "ffprobe is not available; media probe fields will remain empty"
            );
            *probe_availability = ProbeAvailability::Unavailable;
            Ok(MediaProbe {
                error: Some(detail),
                ..MediaProbe::default()
            })
        }
        Err(ProbeError::Cancelled) => Err(scan_cancelled_error()),
        Err(error) => {
            let detail = error.to_string();
            tracing::warn!(
                file_path = %path.display(),
                error = %error,
                "failed to probe media file with ffprobe"
            );
            Ok(MediaProbe {
                error: Some(detail),
                ..MediaProbe::default()
            })
        }
    }
}

fn scan_cancelled_error() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "scan cancelled")
}

fn run_ffprobe(
    path: &Path,
    should_cancel: &mut impl FnMut() -> bool,
) -> Result<MediaProbe, ProbeError> {
    let mut command = Command::new("ffprobe");
    command
        .arg("-v")
        .arg("error")
        .arg("-show_format")
        .arg("-show_streams")
        .arg("-of")
        .arg("json")
        .arg(path);

    let output = run_command_with_timeout(&mut command, FFPROBE_TIMEOUT, should_cancel).map_err(
        |error| match error {
            TimedCommandError::Spawn(error) if error.kind() == io::ErrorKind::NotFound => {
                ProbeError::Unavailable(error)
            }
            TimedCommandError::Spawn(error) | TimedCommandError::Io(error) => ProbeError::Io(error),
            TimedCommandError::TimedOut => ProbeError::TimedOut(FFPROBE_TIMEOUT),
            TimedCommandError::OutputTooLarge => ProbeError::OutputTooLarge,
            TimedCommandError::Cancelled => ProbeError::Cancelled,
        },
    )?;

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

#[derive(Debug)]
enum TimedCommandError {
    Spawn(io::Error),
    Io(io::Error),
    TimedOut,
    OutputTooLarge,
    Cancelled,
}

fn run_command_with_timeout(
    command: &mut Command,
    timeout: Duration,
    should_cancel: &mut impl FnMut() -> bool,
) -> Result<Output, TimedCommandError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command.spawn().map_err(TimedCommandError::Spawn)?;
    let stdout = child.stdout.take().ok_or_else(|| {
        TimedCommandError::Io(io::Error::other("child process stdout was not piped"))
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        TimedCommandError::Io(io::Error::other("child process stderr was not piped"))
    })?;

    // Drain both pipes while the process runs. Waiting first can deadlock when ffprobe emits
    // enough JSON or diagnostics to fill an OS pipe buffer.
    let output_limit_exceeded = Arc::new(AtomicBool::new(false));
    let stdout_limit_exceeded = output_limit_exceeded.clone();
    let stderr_limit_exceeded = output_limit_exceeded.clone();
    let stdout_reader = thread::spawn(move || {
        read_process_pipe_bounded(stdout, FFPROBE_STDOUT_LIMIT, stdout_limit_exceeded)
    });
    let stderr_reader = thread::spawn(move || {
        read_process_pipe_bounded(stderr, FFPROBE_STDERR_LIMIT, stderr_limit_exceeded)
    });
    let started_at = Instant::now();

    let status = loop {
        if should_cancel() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = join_process_pipe(stdout_reader);
            let _ = join_process_pipe(stderr_reader);
            return Err(TimedCommandError::Cancelled);
        }
        if output_limit_exceeded.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = join_process_pipe(stdout_reader);
            let _ = join_process_pipe(stderr_reader);
            return Err(TimedCommandError::OutputTooLarge);
        }

        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started_at.elapsed() < timeout => {
                let remaining = timeout.saturating_sub(started_at.elapsed());
                thread::sleep(PROCESS_POLL_INTERVAL.min(remaining));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_process_pipe(stdout_reader);
                let _ = join_process_pipe(stderr_reader);
                return Err(TimedCommandError::TimedOut);
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_process_pipe(stdout_reader);
                let _ = join_process_pipe(stderr_reader);
                return Err(TimedCommandError::Io(error));
            }
        }
    };

    let stdout = join_process_pipe(stdout_reader).map_err(TimedCommandError::Io)?;
    let stderr = join_process_pipe(stderr_reader).map_err(TimedCommandError::Io)?;
    if output_limit_exceeded.load(Ordering::SeqCst) {
        return Err(TimedCommandError::OutputTooLarge);
    }

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_process_pipe_bounded(
    mut pipe: impl Read,
    limit: usize,
    limit_exceeded: Arc<AtomicBool>,
) -> io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(limit.min(8192));
    let mut buffer = [0_u8; 8192];
    loop {
        let read = pipe.read(&mut buffer)?;
        if read == 0 {
            break;
        }

        let remaining = limit.saturating_sub(output.len());
        let retained = remaining.min(read);
        output.extend_from_slice(&buffer[..retained]);
        if retained < read {
            limit_exceeded.store(true, Ordering::SeqCst);
        }
    }
    Ok(output)
}

fn join_process_pipe(reader: thread::JoinHandle<io::Result<Vec<u8>>>) -> io::Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| io::Error::other("child process output reader panicked"))?
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
        error: None,
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
        technical_tags: detect_technical_tags(video_stream, &parsed.streams),
        audio_streams,
        subtitle_streams,
    })
}

fn detect_technical_tags(
    video_stream: Option<&FfprobeStream>,
    streams: &[FfprobeStream],
) -> Vec<String> {
    let mut tags = Vec::new();

    if let Some(video_stream) = video_stream {
        if let Some(resolution_tag) = video_resolution_tag(video_stream) {
            push_unique_tag(&mut tags, resolution_tag);
        }

        if is_dolby_vision_stream(video_stream) {
            push_unique_tag(&mut tags, "Dolby Vision");
        } else if is_hdr10_plus_stream(video_stream) {
            push_unique_tag(&mut tags, "HDR10+");
        } else if is_hlg_stream(video_stream) {
            push_unique_tag(&mut tags, "HLG");
        } else if is_hdr10_stream(video_stream) {
            push_unique_tag(&mut tags, "HDR10");
        }
    }

    for stream in streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
    {
        if is_atmos_audio_stream(stream) {
            push_unique_tag(&mut tags, "Atmos");
        }

        if is_dts_hd_audio_stream(stream) {
            push_unique_tag(&mut tags, "DTS-HD");
        } else if is_dts_audio_stream(stream) {
            push_unique_tag(&mut tags, "DTS");
        }
    }

    tags
}

fn video_resolution_tag(stream: &FfprobeStream) -> Option<&'static str> {
    let width = stream.width?;
    let height = stream.height?;
    let long_edge = width.max(height);
    let short_edge = width.min(height);

    if long_edge >= 7680 || short_edge >= 4320 {
        return Some("8K");
    }

    if long_edge >= 3840 || short_edge >= 2160 {
        return Some("4K");
    }

    if long_edge >= 2560 || short_edge >= 1440 {
        return Some("1440p");
    }

    if long_edge >= 1920 || short_edge >= 1080 {
        return Some("1080p");
    }

    if long_edge >= 1280 || short_edge >= 720 {
        return Some("720p");
    }

    if long_edge >= 720 || short_edge >= 480 {
        return Some("480p");
    }

    None
}

fn push_unique_tag(tags: &mut Vec<String>, tag: &str) {
    if !tags.iter().any(|value| value == tag) {
        tags.push(tag.to_string());
    }
}

fn is_dolby_vision_stream(stream: &FfprobeStream) -> bool {
    stream_text_values(stream).any(|value| {
        let normalized = value.to_ascii_lowercase();
        normalized.contains("dovi")
            || normalized.contains("dolby vision")
            || normalized.contains("dvhe")
            || normalized.contains("dvh1")
    })
}

fn is_hdr10_plus_stream(stream: &FfprobeStream) -> bool {
    stream_text_values(stream).any(|value| {
        let normalized = value.to_ascii_lowercase();
        normalized.contains("hdr10+") || normalized.contains("smpte2094-40")
    })
}

fn is_hlg_stream(stream: &FfprobeStream) -> bool {
    matches!(
        stream.color_transfer.as_deref().map(str::to_ascii_lowercase),
        Some(value) if value == "arib-std-b67"
    )
}

fn is_hdr10_stream(stream: &FfprobeStream) -> bool {
    let has_pq_transfer = matches!(
        stream.color_transfer.as_deref().map(str::to_ascii_lowercase),
        Some(value) if value == "smpte2084"
    );
    let has_bt2020_primaries = stream
        .color_primaries
        .as_deref()
        .map(str::to_ascii_lowercase)
        .is_some_and(|value| value.contains("bt2020"));

    has_pq_transfer && has_bt2020_primaries
}

fn is_atmos_audio_stream(stream: &FfprobeStream) -> bool {
    stream_text_values(stream).any(|value| {
        let normalized = value.to_ascii_lowercase();
        normalized.contains("atmos") || normalized.contains("joc")
    })
}

fn is_dts_hd_audio_stream(stream: &FfprobeStream) -> bool {
    is_dts_audio_stream(stream)
        && stream_text_values(stream).any(|value| {
            let normalized = value.to_ascii_lowercase();
            normalized.contains("dts-hd")
                || normalized.contains("master audio")
                || normalized.contains("high resolution audio")
                || normalized.contains("hra")
        })
}

fn is_dts_audio_stream(stream: &FfprobeStream) -> bool {
    matches!(
        stream.codec_name.as_deref().map(str::to_ascii_lowercase),
        Some(value) if value == "dts" || value == "dca"
    )
}

fn stream_text_values(stream: &FfprobeStream) -> impl Iterator<Item = &str> {
    stream
        .codec_name
        .as_deref()
        .into_iter()
        .chain(stream.codec_long_name.as_deref())
        .chain(stream.codec_tag_string.as_deref())
        .chain(stream.profile.as_deref())
        .chain(stream.tags.as_ref().and_then(|tags| tags.title.as_deref()))
        .chain(
            stream
                .side_data_list
                .as_deref()
                .into_iter()
                .flatten()
                .filter_map(|side_data| side_data.side_data_type.as_deref()),
        )
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

#[cfg(test)]
mod timeout_tests {
    use super::{read_process_pipe_bounded, run_command_with_timeout, TimedCommandError};
    use std::{
        io::Cursor,
        process::Command,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };

    #[test]
    fn process_pipe_reader_drains_but_does_not_retain_output_over_the_limit() {
        let limit_exceeded = Arc::new(AtomicBool::new(false));
        let retained =
            read_process_pipe_bounded(Cursor::new(vec![b'x'; 1024]), 64, limit_exceeded.clone())
                .unwrap();

        assert_eq!(retained.len(), 64);
        assert!(limit_exceeded.load(Ordering::SeqCst));
    }

    #[cfg(unix)]
    #[test]
    fn command_timeout_terminates_the_child_process() {
        let mut command = Command::new("sh");
        command.args(["-c", "exec sleep 30"]);
        let started_at = Instant::now();

        let result =
            run_command_with_timeout(&mut command, Duration::from_millis(50), &mut || false);

        assert!(matches!(result, Err(TimedCommandError::TimedOut)));
        assert!(started_at.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn command_cancellation_terminates_the_child_process() {
        let mut command = Command::new("sh");
        command.args(["-c", "exec sleep 30"]);
        let started_at = Instant::now();
        let mut poll_count = 0;

        let result = run_command_with_timeout(&mut command, Duration::from_secs(30), &mut || {
            poll_count += 1;
            poll_count >= 2
        });

        assert!(matches!(result, Err(TimedCommandError::Cancelled)));
        assert!(started_at.elapsed() < Duration::from_secs(2));
    }
}
