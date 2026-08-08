use std::{
    collections::HashSet,
    io::Read,
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

const SAMPLE_RATE: usize = 8_000;
const PCM_BYTES_PER_SAMPLE: usize = 2;
const FRAME_SIZE: usize = SAMPLE_RATE;
const MIN_MATCH_SECONDS: usize = 12;
const MAX_MATCH_SECONDS: usize = 150;
const OFFSET_TOLERANCE_SECONDS: isize = 18;
const FRAME_SIMILARITY_THRESHOLD: f64 = 0.93;
const DEFAULT_MINIMUM_CONFIDENCE: f64 = 0.82;
const MAX_ANALYSIS_SECONDS: usize = 600;
const MAX_FFMPEG_TIMEOUT: Duration = Duration::from_secs(600);
const READ_CHUNK_BYTES: usize = 64 * 1024;
const STDERR_LIMIT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct IntroDetectionEpisode {
    pub episode_number: i32,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub struct IntroDetectionConfig {
    pub analysis_seconds: usize,
    pub max_start_offset_seconds: usize,
    pub min_intro_seconds: usize,
    pub ffmpeg_timeout: Duration,
    pub total_timeout: Duration,
    pub minimum_confidence: f64,
}

impl Default for IntroDetectionConfig {
    fn default() -> Self {
        Self {
            analysis_seconds: 240,
            max_start_offset_seconds: 150,
            min_intro_seconds: MIN_MATCH_SECONDS,
            ffmpeg_timeout: Duration::from_secs(90),
            total_timeout: Duration::from_secs(10 * 60),
            minimum_confidence: DEFAULT_MINIMUM_CONFIDENCE,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum IntroDetectionOutcome {
    Match {
        intro_start_seconds: i32,
        intro_end_seconds: i32,
        confidence: f64,
        analyzed_episode_count: usize,
        failed_episode_count: usize,
    },
    NoMatch {
        reason_code: String,
        analyzed_episode_count: usize,
        failed_episode_count: usize,
    },
    RetryableFailure {
        reason_code: String,
        analyzed_episode_count: usize,
        failed_episode_count: usize,
    },
    Cancelled,
}

impl IntroDetectionOutcome {
    fn no_match(
        reason_code: impl Into<String>,
        analyzed_episode_count: usize,
        failed_episode_count: usize,
    ) -> Self {
        Self::NoMatch {
            reason_code: reason_code.into(),
            analyzed_episode_count,
            failed_episode_count,
        }
    }

    fn retryable_failure(
        reason_code: impl Into<String>,
        analyzed_episode_count: usize,
        failed_episode_count: usize,
    ) -> Self {
        Self::RetryableFailure {
            reason_code: reason_code.into(),
            analyzed_episode_count,
            failed_episode_count,
        }
    }
}

#[derive(Debug, Clone)]
struct PairCandidate {
    start_seconds: i32,
    end_seconds: i32,
    similarity: f64,
    episode_numbers: [i32; 2],
}

#[derive(Debug)]
struct BoundedCapture {
    bytes: Vec<u8>,
    truncated: bool,
}

#[derive(Debug)]
struct BoundedOutput {
    status: ExitStatus,
    stdout: BoundedCapture,
    stderr: BoundedCapture,
}

/// Detect a stable repeated opening segment across episodes in one season.
///
/// All media inputs are local paths discovered and authorized by the server. FFmpeg extraction is
/// bounded by both per-process and whole-operation deadlines. Analysis failures are represented as
/// `NoMatch` so playback remains available when an episode cannot be decoded.
pub fn detect_repeated_intro(
    episodes: &[IntroDetectionEpisode],
    config: IntroDetectionConfig,
) -> IntroDetectionOutcome {
    let cancellation = AtomicBool::new(false);
    detect_repeated_intro_with_cancellation(episodes, config, &cancellation)
}

pub fn detect_repeated_intro_with_cancellation(
    episodes: &[IntroDetectionEpisode],
    config: IntroDetectionConfig,
    cancellation: &AtomicBool,
) -> IntroDetectionOutcome {
    if episodes.len() < 3 {
        return IntroDetectionOutcome::no_match("insufficient_episodes", 0, 0);
    }

    let analysis_seconds = config.analysis_seconds.clamp(1, MAX_ANALYSIS_SECONDS);
    let max_start_offset_seconds = config.max_start_offset_seconds;
    let min_match_seconds = config.min_intro_seconds.max(8);
    let ffmpeg_timeout = config
        .ffmpeg_timeout
        .clamp(Duration::from_secs(1), MAX_FFMPEG_TIMEOUT);
    let minimum_confidence = config.minimum_confidence.clamp(0.0, 1.0);
    let deadline = Instant::now()
        .checked_add(config.total_timeout)
        .unwrap_or_else(Instant::now);

    let mut episode_features = Vec::with_capacity(episodes.len());
    let mut failed_episode_count = 0;
    for episode in episodes {
        if cancellation.load(Ordering::Relaxed) {
            return IntroDetectionOutcome::Cancelled;
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return IntroDetectionOutcome::retryable_failure(
                "total_timeout",
                episode_features.len(),
                failed_episode_count,
            );
        };
        let extraction_timeout = ffmpeg_timeout.min(remaining);
        let features = match load_episode_features(
            &episode.file_path,
            analysis_seconds,
            extraction_timeout,
            cancellation,
        ) {
            Ok(features) => features,
            Err(error) => {
                if cancellation.load(Ordering::Relaxed) {
                    return IntroDetectionOutcome::Cancelled;
                }
                failed_episode_count += 1;
                tracing::warn!(
                    episode_number = episode.episode_number,
                    error,
                    "skipping episode that could not be analyzed for intro detection"
                );
                continue;
            }
        };

        if features.len() < min_match_seconds {
            failed_episode_count += 1;
            tracing::debug!(
                episode_number = episode.episode_number,
                audio_frame_count = features.len(),
                "skipping episode with insufficient audio for intro detection"
            );
            continue;
        }
        episode_features.push((episode.episode_number, features));
    }

    if episode_features.len() < 3 {
        return IntroDetectionOutcome::retryable_failure(
            "insufficient_analyzable_episodes",
            episode_features.len(),
            failed_episode_count,
        );
    }
    if episode_features.len() < minimum_supported_episode_count(episodes.len()) {
        return IntroDetectionOutcome::retryable_failure(
            "insufficient_analyzable_episodes",
            episode_features.len(),
            failed_episode_count,
        );
    }

    let mut pair_candidates = Vec::new();
    for left_index in 0..episode_features.len() {
        for right_index in (left_index + 1)..episode_features.len() {
            if Instant::now() >= deadline {
                return IntroDetectionOutcome::retryable_failure(
                    "total_timeout",
                    episode_features.len(),
                    failed_episode_count,
                );
            }
            if cancellation.load(Ordering::Relaxed) {
                return IntroDetectionOutcome::Cancelled;
            }
            let (left_episode_number, left_features) = &episode_features[left_index];
            let (right_episode_number, right_features) = &episode_features[right_index];
            match detect_pair_candidate(
                *left_episode_number,
                left_features,
                *right_episode_number,
                right_features,
                max_start_offset_seconds,
                min_match_seconds,
                deadline,
                cancellation,
            ) {
                Ok(Some(candidate)) => pair_candidates.push(candidate),
                Ok(None) => {}
                Err(()) => {
                    if cancellation.load(Ordering::Relaxed) {
                        return IntroDetectionOutcome::Cancelled;
                    }
                    return IntroDetectionOutcome::retryable_failure(
                        "total_timeout",
                        episode_features.len(),
                        failed_episode_count,
                    );
                }
            }
        }
    }

    let Some((start, end, confidence)) =
        cluster_candidates(&pair_candidates, episodes.len(), min_match_seconds)
    else {
        return IntroDetectionOutcome::no_match(
            "no_stable_segment",
            episode_features.len(),
            failed_episode_count,
        );
    };

    if confidence < minimum_confidence {
        return IntroDetectionOutcome::no_match(
            "confidence_below_threshold",
            episode_features.len(),
            failed_episode_count,
        );
    }

    IntroDetectionOutcome::Match {
        intro_start_seconds: start,
        intro_end_seconds: end,
        confidence,
        analyzed_episode_count: episode_features.len(),
        failed_episode_count,
    }
}

fn load_episode_features(
    file_path: &PathBuf,
    analysis_seconds: usize,
    timeout: Duration,
    cancellation: &AtomicBool,
) -> Result<Vec<[f64; 8]>, String> {
    let raw_audio = run_ffmpeg_extract(file_path, analysis_seconds, timeout, cancellation)?;
    let samples = decode_pcm_mono_s16le(&raw_audio);
    let vectors = samples
        .chunks_exact(FRAME_SIZE)
        .map(build_frame_features)
        .collect::<Vec<_>>();
    Ok(normalize_feature_vectors(vectors))
}

fn run_ffmpeg_extract(
    file_path: &PathBuf,
    analysis_seconds: usize,
    timeout: Duration,
    cancellation: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let max_stdout_bytes = SAMPLE_RATE
        .saturating_mul(analysis_seconds)
        .saturating_mul(PCM_BYTES_PER_SAMPLE)
        .saturating_add(FRAME_SIZE * PCM_BYTES_PER_SAMPLE);

    let command = build_ffmpeg_extract_command(file_path, analysis_seconds);

    let output = run_bounded_process(
        command,
        max_stdout_bytes,
        STDERR_LIMIT_BYTES,
        timeout,
        cancellation,
    )?;
    if output.stdout.truncated {
        return Err(format!(
            "ffmpeg PCM output exceeded the {max_stdout_bytes} byte limit"
        ));
    }
    if !output.status.success() {
        let stderr = bounded_diagnostic(&output.stderr, STDERR_LIMIT_BYTES, "ffmpeg stderr");
        return Err(if stderr.trim().is_empty() {
            format!("ffmpeg exited with status {}", output.status)
        } else {
            stderr
        });
    }
    Ok(output.stdout.bytes)
}

fn build_ffmpeg_extract_command(file_path: &PathBuf, analysis_seconds: usize) -> Command {
    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-nostdin")
        .arg("-protocol_whitelist")
        .arg("file,pipe,crypto,data")
        .arg("-threads")
        .arg("1")
        .arg("-i")
        .arg(file_path)
        .arg("-map")
        .arg("0:a:0")
        .arg("-vn")
        .arg("-sn")
        .arg("-dn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg(SAMPLE_RATE.to_string())
        .arg("-t")
        .arg(analysis_seconds.to_string())
        .arg("-f")
        .arg("s16le")
        .arg("pipe:1");
    command
}

fn run_bounded_process(
    mut command: Command,
    stdout_limit: usize,
    stderr_limit: usize,
    timeout: Duration,
    cancellation: &AtomicBool,
) -> Result<BoundedOutput, String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start ffmpeg: {error}"))?;
    let process_id = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "ffmpeg stdout was not piped".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "ffmpeg stderr was not piped".to_string())?;

    let stdout_reader = thread::spawn(move || drain_bounded(stdout, stdout_limit));
    let stderr_reader = thread::spawn(move || drain_bounded(stderr, stderr_limit));
    let deadline = Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(Instant::now);

    let status = loop {
        if cancellation.load(Ordering::Relaxed) {
            terminate_process_group(process_id);
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err("intro detection was cancelled".to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                thread::sleep(remaining.min(Duration::from_millis(25)));
            }
            Ok(None) => {
                terminate_process_group(process_id);
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!(
                    "ffmpeg exceeded the {} second timeout",
                    timeout.as_secs_f64()
                ));
            }
            Err(error) => {
                terminate_process_group(process_id);
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("failed to wait for ffmpeg: {error}"));
            }
        }
    };

    let stdout = stdout_reader
        .join()
        .map_err(|_| "ffmpeg stdout reader panicked".to_string())?
        .map_err(|error| format!("failed to read ffmpeg stdout: {error}"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "ffmpeg stderr reader panicked".to_string())?
        .map_err(|error| format!("failed to read ffmpeg stderr: {error}"))?;

    Ok(BoundedOutput {
        status,
        stdout,
        stderr,
    })
}

fn terminate_process_group(process_id: u32) {
    #[cfg(unix)]
    // SAFETY: the child starts in a dedicated process group whose id is its pid. A negative pid
    // targets only that group, ensuring any FFmpeg descendants do not retain captured pipes.
    unsafe {
        libc::kill(-(process_id as i32), libc::SIGKILL);
    }

    #[cfg(not(unix))]
    let _ = process_id;
}

fn drain_bounded(mut reader: impl Read, limit: usize) -> std::io::Result<BoundedCapture> {
    let mut bytes = Vec::with_capacity(limit.min(READ_CHUNK_BYTES));
    let mut chunk = [0_u8; READ_CHUNK_BYTES];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let retained = limit.saturating_sub(bytes.len()).min(read);
        bytes.extend_from_slice(&chunk[..retained]);
        truncated |= retained < read;
    }
    Ok(BoundedCapture { bytes, truncated })
}

fn bounded_diagnostic(capture: &BoundedCapture, limit: usize, stream_name: &str) -> String {
    let mut message = String::from_utf8_lossy(&capture.bytes).trim().to_string();
    if capture.truncated {
        let marker = format!("[{stream_name} truncated after {limit} bytes]");
        if message.is_empty() {
            message = marker;
        } else {
            message.push('\n');
            message.push_str(&marker);
        }
    }
    message
}

fn decode_pcm_mono_s16le(raw_bytes: &[u8]) -> Vec<i16> {
    raw_bytes
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
        .collect()
}

fn build_frame_features(samples: &[i16]) -> [f64; 8] {
    if samples.is_empty() {
        return [0.0; 8];
    }

    let sample_count = samples.len() as f64;
    let sum_squares = samples
        .iter()
        .map(|sample| f64::from(*sample).powi(2))
        .sum::<f64>();
    let rms = (sum_squares / sample_count).sqrt();
    let zero_crossings = samples
        .windows(2)
        .filter(|pair| (pair[0] >= 0) != (pair[1] >= 0))
        .count() as f64;
    let mean_abs = samples
        .iter()
        .map(|sample| f64::from(*sample).abs())
        .sum::<f64>()
        / sample_count;

    let mut band_powers = [0.0; 5];
    for (index, frequency) in [120, 240, 480, 960, 1_920].into_iter().enumerate() {
        band_powers[index] = goertzel_power(samples, frequency);
    }
    let total_band_power = band_powers.iter().sum::<f64>().max(f64::MIN_POSITIVE);

    [
        rms.ln_1p(),
        zero_crossings / sample_count,
        mean_abs.ln_1p(),
        band_powers[0] / total_band_power,
        band_powers[1] / total_band_power,
        band_powers[2] / total_band_power,
        band_powers[3] / total_band_power,
        band_powers[4] / total_band_power,
    ]
}

fn goertzel_power(samples: &[i16], target_frequency: usize) -> f64 {
    let normalized_frequency = target_frequency as f64 / SAMPLE_RATE as f64;
    let coefficient = 2.0 * (2.0 * std::f64::consts::PI * normalized_frequency).cos();
    let mut previous = 0.0;
    let mut previous2 = 0.0;
    for sample in samples {
        let current = f64::from(*sample) + coefficient * previous - previous2;
        previous2 = previous;
        previous = current;
    }
    (previous2 * previous2 + previous * previous - coefficient * previous * previous2).max(0.0)
}

fn normalize_feature_vectors(mut vectors: Vec<[f64; 8]>) -> Vec<[f64; 8]> {
    if vectors.is_empty() {
        return vectors;
    }

    for dimension in 0..8 {
        let mean =
            vectors.iter().map(|vector| vector[dimension]).sum::<f64>() / vectors.len() as f64;
        let deviation = (vectors
            .iter()
            .map(|vector| (vector[dimension] - mean).powi(2))
            .sum::<f64>()
            / vectors.len() as f64)
            .sqrt();
        let divisor = if deviation == 0.0 { 1.0 } else { deviation };
        for vector in &mut vectors {
            vector[dimension] = (vector[dimension] - mean) / divisor;
        }
    }
    vectors
}

fn cosine_similarity(left: &[f64; 8], right: &[f64; 8]) -> f64 {
    let numerator = left
        .iter()
        .zip(right.iter())
        .map(|(left, right)| left * right)
        .sum::<f64>();
    let left_norm = left.iter().map(|value| value * value).sum::<f64>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f64>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        numerator / (left_norm * right_norm)
    }
}

#[allow(clippy::too_many_arguments)]
fn detect_pair_candidate(
    left_episode_number: i32,
    left_features: &[[f64; 8]],
    right_episode_number: i32,
    right_features: &[[f64; 8]],
    max_start_offset_seconds: usize,
    min_match_seconds: usize,
    deadline: Instant,
    cancellation: &AtomicBool,
) -> Result<Option<PairCandidate>, ()> {
    let Some(max_left_start) = left_features.len().checked_sub(min_match_seconds) else {
        return Ok(None);
    };
    let Some(max_right_start) = right_features.len().checked_sub(min_match_seconds) else {
        return Ok(None);
    };
    let max_left_start = max_left_start.min(max_start_offset_seconds);
    let max_right_start = max_right_start.min(max_start_offset_seconds);
    let mut best_candidate = None;

    for delta in -OFFSET_TOLERANCE_SECONDS..=OFFSET_TOLERANCE_SECONDS {
        if Instant::now() >= deadline || cancellation.load(Ordering::Relaxed) {
            return Err(());
        }
        let left_start_min = (-delta).max(0) as usize;
        let right_adjusted_max = max_right_start as isize - delta;
        if right_adjusted_max < 0 {
            continue;
        }
        let left_start_max = max_left_start.min(right_adjusted_max as usize);
        if left_start_max < left_start_min {
            continue;
        }

        for left_start in left_start_min..=left_start_max {
            if cancellation.load(Ordering::Relaxed) || Instant::now() >= deadline {
                return Err(());
            }
            let right_start = (left_start as isize + delta) as usize;
            let max_length = (left_features.len() - left_start)
                .min(right_features.len() - right_start)
                .min(MAX_MATCH_SECONDS);
            let mut run_length = 0;
            let mut run_start_offset = 0;
            let mut similarity_sum = 0.0;

            for offset in 0..max_length {
                let similarity = cosine_similarity(
                    &left_features[left_start + offset],
                    &right_features[right_start + offset],
                );
                if similarity >= FRAME_SIMILARITY_THRESHOLD {
                    if run_length == 0 {
                        run_start_offset = offset;
                    }
                    run_length += 1;
                    similarity_sum += similarity;
                } else {
                    retain_best_candidate(
                        &mut best_candidate,
                        left_episode_number,
                        right_episode_number,
                        left_start,
                        right_start,
                        run_start_offset,
                        run_length,
                        similarity_sum,
                        min_match_seconds,
                    );
                    run_length = 0;
                    similarity_sum = 0.0;
                }
            }
            retain_best_candidate(
                &mut best_candidate,
                left_episode_number,
                right_episode_number,
                left_start,
                right_start,
                run_start_offset,
                run_length,
                similarity_sum,
                min_match_seconds,
            );
        }
    }

    Ok(best_candidate)
}

#[allow(clippy::too_many_arguments)]
fn retain_best_candidate(
    best: &mut Option<PairCandidate>,
    left_episode_number: i32,
    right_episode_number: i32,
    left_start: usize,
    right_start: usize,
    run_start_offset: usize,
    run_length: usize,
    similarity_sum: f64,
    min_match_seconds: usize,
) {
    if run_length < min_match_seconds {
        return;
    }
    let start_seconds = round_half_to_even(
        left_start + run_start_offset + right_start + run_start_offset,
        2,
    ) as i32;
    let candidate = PairCandidate {
        start_seconds,
        end_seconds: start_seconds + run_length as i32,
        similarity: similarity_sum / run_length as f64,
        episode_numbers: [left_episode_number, right_episode_number],
    };
    let replace = best.as_ref().is_none_or(|current| {
        let candidate_duration = candidate.end_seconds - candidate.start_seconds;
        let current_duration = current.end_seconds - current.start_seconds;
        candidate_duration > current_duration
            || (candidate_duration == current_duration && candidate.similarity > current.similarity)
    });
    if replace {
        *best = Some(candidate);
    }
}

fn cluster_candidates(
    candidates: &[PairCandidate],
    episode_count: usize,
    min_match_seconds: usize,
) -> Option<(i32, i32, f64)> {
    #[derive(Debug)]
    struct Cluster {
        start_seconds: i32,
        end_seconds: i32,
        starts: Vec<i32>,
        ends: Vec<i32>,
        similarities: Vec<f64>,
        episodes: HashSet<i32>,
    }

    let mut clusters: Vec<Cluster> = Vec::new();
    for candidate in candidates {
        let matched_index = clusters.iter().position(|cluster| {
            (cluster.start_seconds - candidate.start_seconds).abs() <= 6
                && (cluster.end_seconds - candidate.end_seconds).abs() <= 6
        });
        let Some(index) = matched_index else {
            clusters.push(Cluster {
                start_seconds: candidate.start_seconds,
                end_seconds: candidate.end_seconds,
                starts: vec![candidate.start_seconds],
                ends: vec![candidate.end_seconds],
                similarities: vec![candidate.similarity],
                episodes: HashSet::from(candidate.episode_numbers),
            });
            continue;
        };

        let cluster = &mut clusters[index];
        cluster.starts.push(candidate.start_seconds);
        cluster.ends.push(candidate.end_seconds);
        cluster.similarities.push(candidate.similarity);
        cluster.episodes.extend(candidate.episode_numbers);
        cluster.start_seconds = rounded_median(&cluster.starts);
        cluster.end_seconds = rounded_median(&cluster.ends);
    }

    let min_supported_episodes = minimum_supported_episode_count(episode_count);
    let mut best: Option<(usize, i32, f64, f64, i32, i32)> = None;
    for cluster in clusters {
        let supported_episodes = cluster.episodes.len();
        if supported_episodes < min_supported_episodes {
            continue;
        }
        let duration = cluster.end_seconds - cluster.start_seconds;
        if duration < min_match_seconds as i32 {
            continue;
        }
        let average_similarity =
            cluster.similarities.iter().sum::<f64>() / cluster.similarities.len() as f64;
        let support_ratio = supported_episodes as f64 / episode_count as f64;
        let duration_ratio = (f64::from(duration) / 90.0).min(1.0);
        let confidence = (average_similarity * 0.65 + support_ratio * 0.25 + duration_ratio * 0.10)
            .clamp(0.0, 1.0);

        let should_replace =
            best.as_ref()
                .is_none_or(|(best_support, best_duration, best_similarity, _, _, _)| {
                    supported_episodes > *best_support
                        || (supported_episodes == *best_support && duration > *best_duration)
                        || (supported_episodes == *best_support
                            && duration == *best_duration
                            && average_similarity > *best_similarity)
                });
        if should_replace {
            best = Some((
                supported_episodes,
                duration,
                average_similarity,
                (confidence * 10_000.0).round() / 10_000.0,
                cluster.start_seconds,
                cluster.end_seconds,
            ));
        }
    }

    best.map(|(_, _, _, confidence, start, end)| (start, end, confidence))
}

fn minimum_supported_episode_count(episode_count: usize) -> usize {
    3.max((episode_count * 3).div_ceil(5))
}

fn rounded_median(values: &[i32]) -> i32 {
    let mut values = values.to_vec();
    values.sort_unstable();
    if values.len() % 2 == 1 {
        values[values.len() / 2]
    } else {
        let right = values.len() / 2;
        round_half_to_even((values[right - 1] + values[right]) as usize, 2) as i32
    }
}

fn round_half_to_even(numerator: usize, denominator: usize) -> usize {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    if remainder * 2 > denominator || (remainder * 2 == denominator && quotient % 2 == 1) {
        quotient + 1
    } else {
        quotient
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_machine_stable_reason_for_insufficient_episodes() {
        assert_eq!(
            detect_repeated_intro(&[], IntroDetectionConfig::default()),
            IntroDetectionOutcome::NoMatch {
                reason_code: "insufficient_episodes".to_string(),
                analyzed_episode_count: 0,
                failed_episode_count: 0,
            }
        );
    }

    #[test]
    fn isolated_decode_failures_become_a_retryable_machine_reason() {
        let episodes = (1..=3)
            .map(|episode_number| IntroDetectionEpisode {
                episode_number,
                file_path: PathBuf::from(format!(
                    "/definitely-missing/mova-intro-{episode_number}.mkv"
                )),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            detect_repeated_intro(&episodes, IntroDetectionConfig::default()),
            IntroDetectionOutcome::RetryableFailure {
                reason_code: "insufficient_analyzable_episodes".to_string(),
                analyzed_episode_count: 0,
                failed_episode_count: 3,
            }
        );
    }

    #[test]
    fn support_threshold_scales_with_the_sampled_episode_count() {
        assert_eq!(minimum_supported_episode_count(3), 3);
        assert_eq!(minimum_supported_episode_count(5), 3);
        assert_eq!(minimum_supported_episode_count(8), 5);
    }

    #[test]
    fn ffmpeg_extraction_is_local_only_and_single_threaded() {
        let command = build_ffmpeg_extract_command(&PathBuf::from("/media/show.mkv"), 240);
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args
            .windows(2)
            .any(|pair| { pair == ["-protocol_whitelist", "file,pipe,crypto,data"] }));
        assert!(args.windows(2).any(|pair| pair == ["-threads", "1"]));
        assert!(args.windows(2).any(|pair| pair == ["-map", "0:a:0"]));
    }

    #[test]
    fn pcm_decoder_ignores_a_trailing_partial_sample() {
        assert_eq!(decode_pcm_mono_s16le(&[1, 0, 255, 255, 99]), vec![1, -1]);
    }

    #[test]
    fn half_values_use_ties_to_even_rounding() {
        assert_eq!(round_half_to_even(1, 2), 0);
        assert_eq!(round_half_to_even(3, 2), 2);
        assert_eq!(round_half_to_even(5, 2), 2);
        assert_eq!(round_half_to_even(7, 2), 4);
    }

    #[test]
    fn clusters_consistent_pair_matches() {
        let candidates = vec![
            PairCandidate {
                start_seconds: 9,
                end_seconds: 39,
                similarity: 0.98,
                episode_numbers: [1, 2],
            },
            PairCandidate {
                start_seconds: 10,
                end_seconds: 40,
                similarity: 0.97,
                episode_numbers: [1, 3],
            },
            PairCandidate {
                start_seconds: 11,
                end_seconds: 41,
                similarity: 0.96,
                episode_numbers: [2, 3],
            },
        ];
        let result = cluster_candidates(&candidates, 3, 12).expect("cluster should match");
        assert_eq!((result.0, result.1), (10, 40));
        assert!((0.0..=1.0).contains(&result.2));
    }

    #[test]
    fn locates_a_repeated_segment_with_small_episode_offset() {
        let mut left = vec![[0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]; 40];
        let mut right = vec![[0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0]; 40];
        for index in 0..15 {
            let mut feature = [0.0; 8];
            let angle = index as f64 * 0.5;
            feature[0] = angle.cos();
            feature[1] = angle.sin();
            left[5 + index] = feature;
            right[7 + index] = feature;
        }

        let candidate = detect_pair_candidate(
            1,
            &left,
            2,
            &right,
            20,
            12,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false),
        )
        .expect("analysis should stay within deadline")
        .expect("repeated segment should match");

        assert_eq!(candidate.start_seconds, 6);
        assert_eq!(candidate.end_seconds, 21);
        assert!(candidate.similarity >= FRAME_SIMILARITY_THRESHOLD);
    }

    #[cfg(unix)]
    #[test]
    fn bounded_process_terminates_after_timeout() {
        let started_at = Instant::now();
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30"]);
        let cancellation = AtomicBool::new(false);
        let error = run_bounded_process(
            command,
            1024,
            1024,
            Duration::from_millis(50),
            &cancellation,
        )
        .expect_err("process should time out");
        assert!(started_at.elapsed() < Duration::from_secs(2));
        assert!(error.contains("timeout"));
    }

    #[cfg(unix)]
    #[test]
    fn bounded_process_drains_and_marks_oversized_output() {
        let mut command = Command::new("sh");
        command.args(["-c", "head -c 2048 /dev/zero"]);
        let cancellation = AtomicBool::new(false);
        let output =
            run_bounded_process(command, 1024, 1024, Duration::from_secs(2), &cancellation)
                .expect("process should complete");
        assert!(output.status.success());
        assert_eq!(output.stdout.bytes.len(), 1024);
        assert!(output.stdout.truncated);
    }

    #[cfg(unix)]
    #[test]
    fn bounded_process_terminates_when_cancelled() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30"]);
        let cancellation = AtomicBool::new(true);
        let started_at = Instant::now();
        let error =
            run_bounded_process(command, 1024, 1024, Duration::from_secs(30), &cancellation)
                .expect_err("cancelled process should stop");
        assert!(started_at.elapsed() < Duration::from_secs(2));
        assert!(error.contains("cancelled"));
    }
}
