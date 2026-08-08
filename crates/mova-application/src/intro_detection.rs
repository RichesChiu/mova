use crate::error::{ApplicationError, ApplicationResult};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::{atomic::AtomicBool, Arc};

pub const INTRO_DETECTION_ALGORITHM_VERSION: i32 = 2;
const INTRO_DETECTION_MAX_SAMPLED_EPISODES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroDetectionExecutionOutcome {
    Matched,
    NoMatch,
}

pub fn is_intro_detection_candidate(
    header: &crate::playback_header::MediaItemPlaybackHeader,
) -> bool {
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

fn has_complete_intro_markers(start_seconds: Option<i32>, end_seconds: Option<i32>) -> bool {
    matches!((start_seconds, end_seconds), (Some(start), Some(end)) if end > start)
}

pub async fn enqueue_intro_detection_for_playback(
    pool: &PgPool,
    header: &crate::playback_header::MediaItemPlaybackHeader,
) -> ApplicationResult<bool> {
    if !is_intro_detection_candidate(header) {
        return Ok(false);
    }
    let Some(season_id) = header.season_id else {
        return Ok(false);
    };

    mova_db::enqueue_season_intro_detection(
        pool,
        header.library_id,
        season_id,
        INTRO_DETECTION_ALGORITHM_VERSION,
    )
    .await
    .map_err(ApplicationError::from)
}

pub async fn execute_intro_detection_job(
    pool: &PgPool,
    library_id: i64,
    season_id: i64,
    algorithm_version: i32,
    fence: &mova_db::BackgroundJobFence,
    cancellation: Arc<AtomicBool>,
) -> ApplicationResult<IntroDetectionExecutionOutcome> {
    if algorithm_version != INTRO_DETECTION_ALGORITHM_VERSION {
        return Err(unexpected_code(
            "intro_detection_algorithm_version_unsupported",
        ));
    }

    let initial_inputs = mova_db::list_intro_detection_inputs(pool, season_id)
        .await
        .map_err(ApplicationError::from)?;
    validate_input_scope(&initial_inputs, library_id, season_id)?;
    let input_fingerprint = fingerprint_inputs(&initial_inputs);
    let input_snapshot = snapshot_inputs(&initial_inputs);
    let sampled_inputs = select_representative_inputs(&initial_inputs);
    let sampled_episode_count = sampled_inputs.len();
    let detection_episodes = sampled_inputs
        .iter()
        .map(|input| mova_scan::IntroDetectionEpisode {
            episode_number: input.episode_number,
            file_path: input.file_path.clone().into(),
        })
        .collect::<Vec<_>>();

    let cancellation_for_detector = cancellation.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        mova_scan::detect_repeated_intro_with_cancellation(
            &detection_episodes,
            mova_scan::IntroDetectionConfig::default(),
            &cancellation_for_detector,
        )
    })
    .await
    .map_err(|error| ApplicationError::Unexpected(anyhow::Error::new(error)))?;

    if cancellation.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(unexpected_code("intro_detection_cancelled"));
    }

    let current_inputs = mova_db::list_intro_detection_inputs(pool, season_id)
        .await
        .map_err(ApplicationError::from)?;
    validate_input_scope(&current_inputs, library_id, season_id)?;
    if fingerprint_inputs(&current_inputs) != input_fingerprint {
        return Err(unexpected_code("intro_detection_inputs_changed"));
    }

    match outcome {
        mova_scan::IntroDetectionOutcome::Match {
            intro_start_seconds,
            intro_end_seconds,
            confidence,
            analyzed_episode_count,
            failed_episode_count,
        } => {
            mova_db::record_season_intro_analysis(
                pool,
                fence,
                mova_db::RecordSeasonIntroAnalysisParams {
                    library_id,
                    season_id,
                    algorithm_version,
                    input_fingerprint,
                    input_snapshot,
                    outcome: "matched".to_string(),
                    intro_start_seconds: Some(intro_start_seconds),
                    intro_end_seconds: Some(intro_end_seconds),
                    confidence: Some(confidence),
                    sampled_episode_count: count_as_i32(sampled_episode_count),
                    analyzed_episode_count: count_as_i32(analyzed_episode_count),
                    failed_episode_count: count_as_i32(failed_episode_count),
                    reason_code: None,
                },
            )
            .await
            .map_err(ApplicationError::from)?;

            tracing::info!(
                library_id,
                season_id,
                intro_start_seconds,
                intro_end_seconds,
                confidence,
                sampled_episode_count,
                analyzed_episode_count,
                failed_episode_count,
                "completed production intro detection"
            );
            Ok(IntroDetectionExecutionOutcome::Matched)
        }
        mova_scan::IntroDetectionOutcome::NoMatch {
            reason_code,
            analyzed_episode_count,
            failed_episode_count,
        } => {
            mova_db::record_season_intro_analysis(
                pool,
                fence,
                mova_db::RecordSeasonIntroAnalysisParams {
                    library_id,
                    season_id,
                    algorithm_version,
                    input_fingerprint,
                    input_snapshot,
                    outcome: "no_match".to_string(),
                    intro_start_seconds: None,
                    intro_end_seconds: None,
                    confidence: None,
                    sampled_episode_count: count_as_i32(sampled_episode_count),
                    analyzed_episode_count: count_as_i32(analyzed_episode_count),
                    failed_episode_count: count_as_i32(failed_episode_count),
                    reason_code: Some(reason_code.clone()),
                },
            )
            .await
            .map_err(ApplicationError::from)?;

            tracing::info!(
                library_id,
                season_id,
                reason_code,
                sampled_episode_count,
                analyzed_episode_count,
                failed_episode_count,
                "season has no safe automatic intro match"
            );
            Ok(IntroDetectionExecutionOutcome::NoMatch)
        }
        mova_scan::IntroDetectionOutcome::RetryableFailure {
            reason_code,
            analyzed_episode_count,
            failed_episode_count,
        } => {
            tracing::warn!(
                library_id,
                season_id,
                reason_code,
                sampled_episode_count,
                analyzed_episode_count,
                failed_episode_count,
                "intro detection attempt needs retry"
            );
            Err(unexpected_code(&format!("intro_detection_{reason_code}")))
        }
        mova_scan::IntroDetectionOutcome::Cancelled => {
            Err(unexpected_code("intro_detection_cancelled"))
        }
    }
}

fn validate_input_scope(
    inputs: &[mova_db::IntroDetectionInput],
    library_id: i64,
    season_id: i64,
) -> ApplicationResult<()> {
    if inputs.iter().any(|input| {
        input.library_id != library_id || input.season_id != season_id || input.file_size < 0
    }) {
        return Err(unexpected_code("intro_detection_input_scope_invalid"));
    }
    Ok(())
}

fn select_representative_inputs(
    inputs: &[mova_db::IntroDetectionInput],
) -> Vec<mova_db::IntroDetectionInput> {
    if inputs.len() <= INTRO_DETECTION_MAX_SAMPLED_EPISODES {
        return inputs.to_vec();
    }

    let last_index = inputs.len() - 1;
    (0..INTRO_DETECTION_MAX_SAMPLED_EPISODES)
        .map(|sample_index| {
            let numerator = sample_index * last_index;
            let denominator = INTRO_DETECTION_MAX_SAMPLED_EPISODES - 1;
            let input_index = (numerator + denominator / 2) / denominator;
            inputs[input_index].clone()
        })
        .collect()
}

fn fingerprint_inputs(inputs: &[mova_db::IntroDetectionInput]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((inputs.len() as u64).to_le_bytes());
    for input in inputs {
        hasher.update(input.season_id.to_le_bytes());
        hasher.update(input.episode_number.to_le_bytes());
        hasher.update(input.media_file_id.to_le_bytes());
        hasher.update(input.file_size.to_le_bytes());
        hasher.update(input.duration_seconds.unwrap_or(-1).to_le_bytes());
        write_hash_string(&mut hasher, &input.file_path);
        write_hash_string(&mut hasher, input.scan_hash.as_deref().unwrap_or(""));
    }
    format!("{:x}", hasher.finalize())
}

fn snapshot_inputs(inputs: &[mova_db::IntroDetectionInput]) -> serde_json::Value {
    serde_json::Value::Array(
        inputs
            .iter()
            .map(|input| {
                serde_json::json!({
                    "episode_number": input.episode_number,
                    "media_file_id": input.media_file_id,
                    "file_path": input.file_path,
                    "file_size": input.file_size,
                    "duration_seconds": input.duration_seconds,
                    "scan_hash": input.scan_hash,
                })
            })
            .collect(),
    )
}

fn write_hash_string(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn count_as_i32(count: usize) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

fn unexpected_code(code: &str) -> ApplicationError {
    ApplicationError::Unexpected(anyhow::Error::msg(code.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        fingerprint_inputs, is_intro_detection_candidate, select_representative_inputs,
        INTRO_DETECTION_MAX_SAMPLED_EPISODES,
    };
    use time::OffsetDateTime;

    fn input(episode_number: i32) -> mova_db::IntroDetectionInput {
        mova_db::IntroDetectionInput {
            library_id: 1,
            season_id: 2,
            season_number: 1,
            episode_number,
            media_file_id: i64::from(episode_number),
            file_path: format!("/media/show/S01E{episode_number:02}.mkv"),
            file_size: 100,
            duration_seconds: Some(2_400),
            scan_hash: Some(format!("hash-{episode_number}")),
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn header(
        media_type: &str,
        season_id: Option<i64>,
    ) -> crate::playback_header::MediaItemPlaybackHeader {
        crate::playback_header::MediaItemPlaybackHeader {
            media_item_id: 1,
            library_id: 1,
            media_type: media_type.to_string(),
            series_media_item_id: Some(10),
            title: "Series".to_string(),
            original_title: None,
            year: Some(2026),
            logo_path: None,
            logo_updated_at: OffsetDateTime::UNIX_EPOCH,
            season_id,
            season_number: Some(1),
            episode_number: Some(1),
            episode_title: Some("Pilot".to_string()),
            season_intro_start_seconds: None,
            season_intro_end_seconds: None,
            episode_intro_start_seconds: None,
            episode_intro_end_seconds: None,
        }
    }

    #[test]
    fn only_episode_playback_can_schedule_detection() {
        assert!(is_intro_detection_candidate(&header("episode", Some(2))));
        assert!(!is_intro_detection_candidate(&header("movie", None)));

        let mut season_marked = header("episode", Some(2));
        season_marked.season_intro_start_seconds = Some(12);
        season_marked.season_intro_end_seconds = Some(72);
        assert!(!is_intro_detection_candidate(&season_marked));

        let mut episode_marked = header("episode", Some(2));
        episode_marked.episode_intro_start_seconds = Some(8);
        episode_marked.episode_intro_end_seconds = Some(68);
        assert!(!is_intro_detection_candidate(&episode_marked));
    }

    #[test]
    fn representative_sampling_is_bounded_and_spans_the_season() {
        let inputs = (1..=40).map(input).collect::<Vec<_>>();
        let sampled = select_representative_inputs(&inputs);
        assert_eq!(sampled.len(), INTRO_DETECTION_MAX_SAMPLED_EPISODES);
        assert_eq!(sampled.first().map(|item| item.episode_number), Some(1));
        assert_eq!(sampled.last().map(|item| item.episode_number), Some(40));
    }

    #[test]
    fn input_fingerprint_changes_with_media_identity() {
        let first = vec![input(1), input(2), input(3)];
        let mut changed = first.clone();
        changed[1].scan_hash = Some("replacement".to_string());
        assert_ne!(fingerprint_inputs(&first), fingerprint_inputs(&changed));
    }
}
