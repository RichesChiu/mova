use crate::error::{ApplicationError, ApplicationResult};
use sqlx::postgres::PgPool;
use std::{
    collections::HashSet,
    process::Command,
    sync::{Mutex, OnceLock},
};
use tokio::task;

const INTRO_DETECTOR_SCRIPT_PATH: &str = "scripts/detect_intro.py";
const INTRO_DETECTION_MIN_EPISODES: usize = 3;
const INTRO_DETECTION_MIN_DURATION_SECONDS: i32 = 12;

#[derive(Debug, serde::Serialize)]
struct IntroDetectorRequest {
    analysis_seconds: i32,
    max_start_offset_seconds: i32,
    min_intro_seconds: i32,
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
        episodes: season.episodes,
    };
    let request_json = serde_json::to_vec(&request)
        .map_err(|error| ApplicationError::Unexpected(anyhow::Error::new(error)))?;

    let response = task::spawn_blocking(move || {
        let mut command = Command::new("python3");
        command.arg(INTRO_DETECTOR_SCRIPT_PATH);
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        let mut child = command.spawn().map_err(anyhow::Error::new)?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(&request_json)?;
        }

        let output = child.wait_with_output().map_err(anyhow::Error::new)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            anyhow::bail!(
                "python intro detector failed for season {}: {}",
                season.season_number,
                if stderr.is_empty() {
                    format!("exit status {}", output.status)
                } else {
                    stderr
                }
            );
        }

        serde_json::from_slice::<IntroDetectorResponse>(&output.stdout).map_err(anyhow::Error::new)
    })
    .await
    .map_err(|error| ApplicationError::Unexpected(anyhow::Error::new(error)))?
    .map_err(ApplicationError::Unexpected)?;

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

#[cfg(test)]
mod tests {
    use super::needs_intro_detection;

    fn build_header() -> mova_db::MediaItemPlaybackHeader {
        mova_db::MediaItemPlaybackHeader {
            media_item_id: 1,
            library_id: 1,
            media_type: "episode".to_string(),
            series_media_item_id: Some(10),
            title: "Severance".to_string(),
            original_title: None,
            year: Some(2022),
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
}
