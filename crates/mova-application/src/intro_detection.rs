use crate::error::{ApplicationError, ApplicationResult};
use sqlx::postgres::PgPool;
use std::{
    collections::HashSet,
    process::Stdio,
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    time,
};

const INTRO_DETECTOR_SCRIPT_PATH: &str = "scripts/detect_intro.py";
const INTRO_DETECTION_MIN_EPISODES: usize = 3;
const INTRO_DETECTION_MIN_DURATION_SECONDS: i32 = 12;
const INTRO_DETECTOR_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const INTRO_FFMPEG_TIMEOUT_SECONDS: u64 = 90;
const INTRO_DETECTOR_STDOUT_LIMIT_BYTES: usize = 1024 * 1024;
const INTRO_DETECTOR_STDERR_LIMIT_BYTES: usize = 64 * 1024;
const INTRO_DETECTOR_READ_CHUNK_BYTES: usize = 8 * 1024;

#[derive(Debug, serde::Serialize)]
struct IntroDetectorRequest {
    analysis_seconds: i32,
    max_start_offset_seconds: i32,
    min_intro_seconds: i32,
    ffmpeg_timeout_seconds: u64,
    episodes: Vec<IntroDetectorEpisodeInput>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct IntroDetectorEpisodeInput {
    episode_number: i32,
    file_path: String,
}

#[derive(Debug, serde::Deserialize)]
struct IntroDetectorResponse {
    status: String,
    intro_start_seconds: Option<i32>,
    intro_end_seconds: Option<i32>,
    confidence: Option<f64>,
    reason: Option<String>,
}

#[derive(Debug, Clone)]
struct SeasonIntroDetectionCandidate {
    season_id: i64,
    season_number: i32,
    episodes: Vec<IntroDetectorEpisodeInput>,
}

fn intro_detection_inflight() -> &'static Mutex<HashSet<i64>> {
    static INFLIGHT: OnceLock<Mutex<HashSet<i64>>> = OnceLock::new();
    INFLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

fn has_complete_intro_markers(
    intro_start_seconds: Option<i32>,
    intro_end_seconds: Option<i32>,
) -> bool {
    matches!(
        (intro_start_seconds, intro_end_seconds),
        (Some(start), Some(end)) if end > start
    )
}

pub(crate) fn needs_intro_detection(header: &mova_db::MediaItemPlaybackHeader) -> bool {
    header.media_type.eq_ignore_ascii_case("episode")
        && header.season_id.is_some()
        && !has_complete_intro_markers(
            header.episode_intro_start_seconds,
            header.episode_intro_end_seconds,
        )
        && !has_complete_intro_markers(
            header.season_intro_start_seconds,
            header.season_intro_end_seconds,
        )
}

pub(crate) async fn ensure_intro_markers_for_playback(
    pool: &PgPool,
    header: &mova_db::MediaItemPlaybackHeader,
) -> ApplicationResult<()> {
    if !needs_intro_detection(header) {
        return Ok(());
    }

    let Some(season_id) = header.season_id else {
        return Ok(());
    };

    {
        let mut inflight = intro_detection_inflight()
            .lock()
            .map_err(|error| ApplicationError::Unexpected(anyhow::Error::msg(error.to_string())))?;
        if !inflight.insert(season_id) {
            tracing::debug!(
                season_id,
                media_item_id = header.media_item_id,
                "on-demand intro detection already in progress for season"
            );
            return Ok(());
        }
    }

    let result = ensure_intro_markers_for_season(pool, season_id, header.season_number).await;

    if let Ok(mut inflight) = intro_detection_inflight().lock() {
        inflight.remove(&season_id);
    }

    result
}

async fn ensure_intro_markers_for_season(
    pool: &PgPool,
    season_id: i64,
    season_number: Option<i32>,
) -> ApplicationResult<()> {
    let episodes = mova_db::list_episodes_for_season(pool, season_id)
        .await
        .map_err(ApplicationError::from)?;
    if episodes.len() < INTRO_DETECTION_MIN_EPISODES {
        return Ok(());
    }

    let mut detection_episodes = Vec::new();
    for episode in episodes {
        let Some(primary_media_file) =
            mova_db::list_media_files_for_media_item(pool, episode.media_item_id)
                .await
                .map_err(ApplicationError::from)?
                .into_iter()
                .next()
        else {
            continue;
        };

        detection_episodes.push(IntroDetectorEpisodeInput {
            episode_number: episode.episode_number,
            file_path: primary_media_file.file_path,
        });
    }

    if detection_episodes.len() < INTRO_DETECTION_MIN_EPISODES {
        return Ok(());
    }

    let detection = detect_season_intro_with_python(SeasonIntroDetectionCandidate {
        season_id,
        season_number: season_number.unwrap_or_default(),
        episodes: detection_episodes,
    })
    .await?;

    let Some((intro_start_seconds, intro_end_seconds)) = detection else {
        return Ok(());
    };

    mova_db::update_season_intro_markers(
        pool,
        season_id,
        Some(intro_start_seconds),
        Some(intro_end_seconds),
    )
    .await
    .map_err(ApplicationError::from)?;

    Ok(())
}

async fn detect_season_intro_with_python(
    season: SeasonIntroDetectionCandidate,
) -> ApplicationResult<Option<(i32, i32)>> {
    let request = IntroDetectorRequest {
        analysis_seconds: 240,
        max_start_offset_seconds: 150,
        min_intro_seconds: INTRO_DETECTION_MIN_DURATION_SECONDS,
        ffmpeg_timeout_seconds: INTRO_FFMPEG_TIMEOUT_SECONDS,
        episodes: season.episodes,
    };
    let request_json = serde_json::to_vec(&request)
        .map_err(|error| ApplicationError::Unexpected(anyhow::Error::new(error)))?;

    let output = run_intro_detector_process(
        "python3",
        &[INTRO_DETECTOR_SCRIPT_PATH],
        &request_json,
        INTRO_DETECTOR_TIMEOUT,
    )
    .await
    .map_err(ApplicationError::Unexpected)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(ApplicationError::Unexpected(anyhow::anyhow!(
            "python intro detector failed for season {}: {}",
            season.season_number,
            if stderr.is_empty() {
                format!("exit status {}", output.status)
            } else {
                stderr
            }
        )));
    }

    let response = serde_json::from_slice::<IntroDetectorResponse>(&output.stdout)
        .map_err(|error| ApplicationError::Unexpected(anyhow::Error::new(error)))?;

    if !response.status.eq_ignore_ascii_case("ok") {
        if let Some(reason) = response.reason {
            tracing::debug!(
                season_id = season.season_id,
                season_number = season.season_number,
                reason,
                "automatic intro detector skipped season"
            );
        }
        return Ok(None);
    }

    let Some(intro_start_seconds) = response.intro_start_seconds else {
        return Ok(None);
    };
    let Some(intro_end_seconds) = response.intro_end_seconds else {
        return Ok(None);
    };

    if intro_end_seconds - intro_start_seconds < INTRO_DETECTION_MIN_DURATION_SECONDS {
        return Ok(None);
    }

    if let Some(confidence) = response.confidence {
        tracing::info!(
            season_id = season.season_id,
            season_number = season.season_number,
            intro_start_seconds,
            intro_end_seconds,
            confidence,
            "detected season intro markers"
        );
    }

    Ok(Some((intro_start_seconds, intro_end_seconds)))
}

async fn run_intro_detector_process(
    program: &str,
    args: &[&str],
    request_json: &[u8],
    timeout: Duration,
) -> anyhow::Result<std::process::Output> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    // A dedicated process group keeps the detector and its ffmpeg child isolated from the
    // server's process group. Killing the detector closes ffmpeg's captured pipes as well.
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command.spawn()?;
    let mut process_guard = IntroDetectorProcessGuard::new(child.id());
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("intro detector stdin was not piped"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("intro detector stdout was not piped"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("intro detector stderr was not piped"))?;

    let stdout_reader = tokio::spawn(async move {
        drain_pipe_bounded(&mut stdout, INTRO_DETECTOR_STDOUT_LIMIT_BYTES).await
    });
    let stderr_reader = tokio::spawn(async move {
        drain_pipe_bounded(&mut stderr, INTRO_DETECTOR_STDERR_LIMIT_BYTES).await
    });

    let status = match time::timeout(timeout, async {
        stdin.write_all(request_json).await?;
        stdin.shutdown().await?;
        drop(stdin);
        child.wait().await
    })
    .await
    {
        Ok(result) => result?,
        Err(_) => {
            process_guard.terminate();
            let _ = child.start_kill();
            let _ = child.wait().await;
            let _ = stdout_reader.await;
            let _ = stderr_reader.await;
            anyhow::bail!(
                "python intro detector exceeded the {} second timeout",
                timeout.as_secs()
            );
        }
    };
    process_guard.disarm();

    let stdout = stdout_reader.await??;
    let stderr = stderr_reader.await??;
    if stdout.truncated {
        anyhow::bail!(
            "python intro detector stdout exceeded the {} byte machine-output limit",
            INTRO_DETECTOR_STDOUT_LIMIT_BYTES
        );
    }
    let stderr = retain_truncation_marker(
        stderr,
        INTRO_DETECTOR_STDERR_LIMIT_BYTES,
        "python intro detector stderr",
    );

    Ok(std::process::Output {
        status,
        stdout: stdout.bytes,
        stderr,
    })
}

#[derive(Debug)]
struct BoundedPipeRead {
    bytes: Vec<u8>,
    truncated: bool,
}

async fn drain_pipe_bounded<R>(
    reader: &mut R,
    limit_bytes: usize,
) -> std::io::Result<BoundedPipeRead>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(limit_bytes.min(INTRO_DETECTOR_READ_CHUNK_BYTES));
    let mut chunk = [0_u8; INTRO_DETECTOR_READ_CHUNK_BYTES];
    let mut truncated = false;

    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            break;
        }

        let remaining = limit_bytes.saturating_sub(bytes.len());
        let retained = remaining.min(read);
        bytes.extend_from_slice(&chunk[..retained]);
        truncated |= retained < read;
    }

    Ok(BoundedPipeRead { bytes, truncated })
}

fn retain_truncation_marker(
    mut output: BoundedPipeRead,
    limit_bytes: usize,
    stream_name: &str,
) -> Vec<u8> {
    if !output.truncated {
        return output.bytes;
    }

    let marker = format!("\n[{stream_name} truncated after {limit_bytes} bytes]\n").into_bytes();
    if marker.len() >= limit_bytes {
        marker.into_iter().take(limit_bytes).collect()
    } else {
        output.bytes.truncate(limit_bytes - marker.len());
        output.bytes.extend_from_slice(&marker);
        output.bytes
    }
}

struct IntroDetectorProcessGuard {
    #[cfg(unix)]
    process_id: Option<u32>,
    armed: bool,
}

impl IntroDetectorProcessGuard {
    fn new(process_id: Option<u32>) -> Self {
        #[cfg(not(unix))]
        let _ = process_id;

        Self {
            #[cfg(unix)]
            process_id,
            armed: true,
        }
    }

    fn terminate(&mut self) {
        if !self.armed {
            return;
        }

        #[cfg(unix)]
        if let Some(process_id) = self.process_id {
            // The Python detector starts in its own process group. Targeting the negative group
            // id terminates both Python and a currently running ffmpeg extraction.
            let _ = std::process::Command::new("kill")
                .arg("-KILL")
                .arg(format!("-{process_id}"))
                .status();
        }

        self.armed = false;
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for IntroDetectorProcessGuard {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        needs_intro_detection, run_intro_detector_process, IntroDetectorEpisodeInput,
        IntroDetectorRequest, IntroDetectorResponse, INTRO_DETECTION_MIN_DURATION_SECONDS,
        INTRO_DETECTOR_STDERR_LIMIT_BYTES, INTRO_DETECTOR_STDOUT_LIMIT_BYTES,
        INTRO_FFMPEG_TIMEOUT_SECONDS,
    };
    use std::{path::PathBuf, time::Duration};

    fn build_header() -> mova_db::MediaItemPlaybackHeader {
        mova_db::MediaItemPlaybackHeader {
            media_item_id: 1,
            library_id: 1,
            media_type: "episode".to_string(),
            series_media_item_id: Some(10),
            title: "Severance".to_string(),
            original_title: None,
            year: Some(2022),
            logo_path: None,
            logo_updated_at: time::OffsetDateTime::UNIX_EPOCH,
            season_id: Some(20),
            season_number: Some(1),
            episode_number: Some(1),
            episode_title: Some("Good News About Hell".to_string()),
            season_intro_start_seconds: None,
            season_intro_end_seconds: None,
            episode_intro_start_seconds: None,
            episode_intro_end_seconds: None,
        }
    }

    #[test]
    fn detects_only_for_episode_without_existing_markers() {
        let header = build_header();
        assert!(needs_intro_detection(&header));
    }

    #[test]
    fn skips_when_season_markers_already_exist() {
        let mut header = build_header();
        header.season_intro_start_seconds = Some(15);
        header.season_intro_end_seconds = Some(82);
        assert!(!needs_intro_detection(&header));
    }

    #[test]
    fn skips_when_episode_markers_already_exist() {
        let mut header = build_header();
        header.episode_intro_start_seconds = Some(3);
        header.episode_intro_end_seconds = Some(76);
        assert!(!needs_intro_detection(&header));
    }

    #[test]
    fn skips_for_movies() {
        let mut header = build_header();
        header.media_type = "movie".to_string();
        header.season_id = None;
        header.season_number = None;
        header.episode_number = None;
        header.episode_title = None;
        assert!(!needs_intro_detection(&header));
    }

    #[test]
    fn detector_request_includes_the_per_ffmpeg_timeout() {
        let request = IntroDetectorRequest {
            analysis_seconds: 240,
            max_start_offset_seconds: 150,
            min_intro_seconds: INTRO_DETECTION_MIN_DURATION_SECONDS,
            ffmpeg_timeout_seconds: INTRO_FFMPEG_TIMEOUT_SECONDS,
            episodes: vec![IntroDetectorEpisodeInput {
                episode_number: 1,
                file_path: "/media/example.mkv".to_string(),
            }],
        };

        let payload = serde_json::to_value(request).expect("request should serialize");
        assert_eq!(
            payload["ffmpeg_timeout_seconds"],
            serde_json::json!(INTRO_FFMPEG_TIMEOUT_SECONDS)
        );
    }

    #[tokio::test]
    async fn detector_script_contract_returns_machine_readable_no_match() {
        let script_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/detect_intro.py");
        let request = IntroDetectorRequest {
            analysis_seconds: 240,
            max_start_offset_seconds: 150,
            min_intro_seconds: INTRO_DETECTION_MIN_DURATION_SECONDS,
            ffmpeg_timeout_seconds: INTRO_FFMPEG_TIMEOUT_SECONDS,
            episodes: Vec::new(),
        };
        let request_json = serde_json::to_vec(&request).expect("request should serialize");
        let script_path = script_path.to_string_lossy().into_owned();

        let output = run_intro_detector_process(
            "python3",
            &[script_path.as_str()],
            &request_json,
            Duration::from_secs(5),
        )
        .await
        .expect("detector script should run");

        assert!(output.status.success());
        let response = serde_json::from_slice::<IntroDetectorResponse>(&output.stdout)
            .expect("detector response should be JSON");
        assert_eq!(response.status, "no-match");
        assert_eq!(
            response.reason.as_deref(),
            Some("need at least three playable episodes")
        );
    }

    #[tokio::test]
    async fn detector_process_is_killed_after_timeout() {
        let started_at = tokio::time::Instant::now();
        let result = run_intro_detector_process(
            "python3",
            &["-c", "import sys,time; sys.stdin.read(); time.sleep(30)"],
            b"{}",
            Duration::from_millis(50),
        )
        .await;

        assert!(result.is_err());
        assert!(started_at.elapsed() < Duration::from_secs(2));
        assert!(result
            .expect_err("detector should time out")
            .to_string()
            .contains("timeout"));
    }

    #[tokio::test]
    async fn detector_process_rejects_oversized_machine_output_without_deadlock() {
        let script = format!(
            "import sys; sys.stdin.read(); sys.stdout.buffer.write(b'x' * {})",
            INTRO_DETECTOR_STDOUT_LIMIT_BYTES + 1
        );
        let result = run_intro_detector_process(
            "python3",
            &["-c", script.as_str()],
            b"{}",
            Duration::from_secs(5),
        )
        .await;

        assert!(result
            .expect_err("oversized machine output should be rejected")
            .to_string()
            .contains("machine-output limit"));
    }

    #[tokio::test]
    async fn detector_process_bounds_and_marks_diagnostics_without_deadlock() {
        let script = format!(
            "import sys; sys.stdin.read(); sys.stderr.buffer.write(b'x' * {}); sys.exit(7)",
            INTRO_DETECTOR_STDERR_LIMIT_BYTES + 1
        );
        let output = run_intro_detector_process(
            "python3",
            &["-c", script.as_str()],
            b"{}",
            Duration::from_secs(5),
        )
        .await
        .expect("bounded diagnostics should still return process output");

        assert!(!output.status.success());
        assert_eq!(output.stderr.len(), INTRO_DETECTOR_STDERR_LIMIT_BYTES);
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains("python intro detector stderr truncated after"));
    }
}
