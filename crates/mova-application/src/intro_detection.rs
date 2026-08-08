use crate::error::{ApplicationError, ApplicationResult};
use sqlx::postgres::PgPool;
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::task::JoinHandle;

const INTRO_DETECTION_MIN_EPISODES: usize = 3;
const INTRO_DETECTION_MIN_DURATION_SECONDS: i32 = 12;
const INTRO_DETECTION_RETRY_COOLDOWN: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone)]
struct SeasonIntroDetectionCandidate {
    season_id: i64,
    season_number: i32,
    episodes: Vec<mova_scan::IntroDetectionEpisode>,
}

fn intro_detection_inflight() -> &'static Mutex<HashSet<i64>> {
    static INFLIGHT: OnceLock<Mutex<HashSet<i64>>> = OnceLock::new();
    INFLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

fn intro_detection_recent_attempts() -> &'static Mutex<HashMap<i64, Instant>> {
    static RECENT_ATTEMPTS: OnceLock<Mutex<HashMap<i64, Instant>>> = OnceLock::new();
    RECENT_ATTEMPTS.get_or_init(|| Mutex::new(HashMap::new()))
}

struct IntroDetectionInflightGuard {
    season_id: i64,
}

impl Drop for IntroDetectionInflightGuard {
    fn drop(&mut self) {
        if let Ok(mut inflight) = intro_detection_inflight().lock() {
            inflight.remove(&self.season_id);
        }
    }
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
        let mut recent_attempts = intro_detection_recent_attempts()
            .lock()
            .map_err(|error| ApplicationError::Unexpected(anyhow::Error::msg(error.to_string())))?;
        recent_attempts
            .retain(|_, attempted_at| attempted_at.elapsed() < INTRO_DETECTION_RETRY_COOLDOWN);
        if recent_attempts.get(&season_id).is_some() {
            tracing::debug!(
                season_id,
                media_item_id = header.media_item_id,
                "on-demand intro detection is in retry cooldown"
            );
            return Ok(());
        }
    }

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
    let _inflight_guard = IntroDetectionInflightGuard { season_id };

    let result = ensure_intro_markers_for_season(pool, season_id, header.season_number).await;

    if let Ok(mut recent_attempts) = intro_detection_recent_attempts().lock() {
        recent_attempts.insert(season_id, Instant::now());
    }

    result
}

/// Schedule on-demand intro detection without holding the playback-header response open.
///
/// Detection can run ffmpeg across several episodes and is intentionally best-effort. The
/// per-season in-flight guard inside `ensure_intro_markers_for_playback` keeps concurrent player
/// requests from starting duplicate work for the same season.
pub(crate) fn schedule_intro_markers_for_playback(
    pool: PgPool,
    header: mova_db::MediaItemPlaybackHeader,
) -> Option<JoinHandle<()>> {
    if !needs_intro_detection(&header) {
        return None;
    }

    Some(tokio::spawn(async move {
        if let Err(error) = ensure_intro_markers_for_playback(&pool, &header).await {
            tracing::warn!(
                media_item_id = header.media_item_id,
                season_id = header.season_id,
                error = ?error,
                "background intro detection failed; playback remains available"
            );
        }
    }))
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

        detection_episodes.push(mova_scan::IntroDetectionEpisode {
            episode_number: episode.episode_number,
            file_path: primary_media_file.file_path.into(),
        });
    }

    if detection_episodes.len() < INTRO_DETECTION_MIN_EPISODES {
        return Ok(());
    }

    let detection = detect_season_intro(SeasonIntroDetectionCandidate {
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

async fn detect_season_intro(
    season: SeasonIntroDetectionCandidate,
) -> ApplicationResult<Option<(i32, i32)>> {
    let SeasonIntroDetectionCandidate {
        season_id,
        season_number,
        episodes,
    } = season;
    let outcome = tokio::task::spawn_blocking(move || {
        mova_scan::detect_repeated_intro(&episodes, mova_scan::IntroDetectionConfig::default())
    })
    .await
    .map_err(|error| ApplicationError::Unexpected(anyhow::Error::new(error)))?;

    let mova_scan::IntroDetectionOutcome::Match {
        intro_start_seconds,
        intro_end_seconds,
        confidence,
    } = outcome
    else {
        if let mova_scan::IntroDetectionOutcome::NoMatch { reason } = outcome {
            tracing::debug!(
                season_id,
                season_number,
                reason,
                "automatic intro detector skipped season"
            );
        }
        return Ok(None);
    };

    if intro_end_seconds - intro_start_seconds < INTRO_DETECTION_MIN_DURATION_SECONDS {
        return Ok(None);
    }

    tracing::info!(
        season_id,
        season_number,
        intro_start_seconds,
        intro_end_seconds,
        confidence,
        "detected season intro markers"
    );

    Ok(Some((intro_start_seconds, intro_end_seconds)))
}

#[cfg(test)]
mod tests {
    use super::{needs_intro_detection, schedule_intro_markers_for_playback};
    use sqlx::postgres::PgPoolOptions;

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

    #[tokio::test]
    async fn schedules_detection_without_waiting_for_database_or_ffmpeg() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://mova:mova@127.0.0.1:9/mova")
            .expect("test database URL should parse");

        let task = schedule_intro_markers_for_playback(pool, build_header())
            .expect("episode without intro markers should schedule background detection");

        task.abort();
        let _ = task.await;
    }
}
