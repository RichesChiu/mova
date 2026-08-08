use crate::{background_jobs::lock_background_job_fence, BackgroundJobFence};
use anyhow::{bail, Context, Result};
use sqlx::{PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;

pub const INTRO_DETECTION_JOB_TYPE: &str = "media.intro.detect";
pub(crate) const INTRO_DETECTION_TERMINAL_RETRY_COOLDOWN_SECONDS: i64 = 6 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntroDetectionInput {
    pub library_id: i64,
    pub season_id: i64,
    pub season_number: i32,
    pub episode_number: i32,
    pub media_file_id: i64,
    pub file_path: String,
    pub file_size: i64,
    pub duration_seconds: Option<i32>,
    pub scan_hash: Option<String>,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct RecordSeasonIntroAnalysisParams {
    pub library_id: i64,
    pub season_id: i64,
    pub algorithm_version: i32,
    pub input_fingerprint: String,
    pub input_snapshot: serde_json::Value,
    pub outcome: String,
    pub intro_start_seconds: Option<i32>,
    pub intro_end_seconds: Option<i32>,
    pub confidence: Option<f64>,
    pub sampled_episode_count: i32,
    pub analyzed_episode_count: i32,
    pub failed_episode_count: i32,
    pub reason_code: Option<String>,
}

pub async fn list_intro_detection_inputs(
    pool: &PgPool,
    season_id: i64,
) -> Result<Vec<IntroDetectionInput>> {
    let rows = sqlx::query(
        r#"
        select
            series.library_id,
            season.id as season_id,
            season.season_number,
            episode.episode_number,
            media_file.id as media_file_id,
            media_file.file_path,
            media_file.file_size,
            media_file.duration_seconds,
            media_file.scan_hash,
            media_file.updated_at
        from seasons season
        join media_items series
          on series.id = season.series_id
         and series.media_type = 'series'
        join episodes episode on episode.season_id = season.id
        join lateral (
            select candidate.*
            from media_files candidate
            where candidate.media_item_id = episode.media_item_id
            order by candidate.created_at asc, candidate.id asc
            limit 1
        ) media_file on true
        where season.id = $1
        order by episode.episode_number asc, episode.media_item_id asc
        "#,
    )
    .bind(season_id)
    .fetch_all(pool)
    .await
    .context("failed to list season intro detection inputs")?;

    Ok(rows
        .into_iter()
        .map(|row| IntroDetectionInput {
            library_id: row.get("library_id"),
            season_id: row.get("season_id"),
            season_number: row.get("season_number"),
            episode_number: row.get("episode_number"),
            media_file_id: row.get("media_file_id"),
            file_path: row.get("file_path"),
            file_size: row.get("file_size"),
            duration_seconds: row.get("duration_seconds"),
            scan_hash: row.get("scan_hash"),
            updated_at: row.get("updated_at"),
        })
        .collect())
}

pub async fn enqueue_season_intro_detection(
    pool: &PgPool,
    library_id: i64,
    season_id: i64,
    algorithm_version: i32,
) -> Result<bool> {
    let inserted = sqlx::query_scalar::<_, i64>(
        r#"
        insert into background_jobs (
            job_type,
            scope_type,
            scope_id,
            payload,
            max_attempts
        )
        select
            $1,
            'library',
            $2,
            jsonb_build_object(
                'library_id', $2,
                'season_id', $3,
                'algorithm_version', $4
            ),
            3
        from seasons season
        join media_items series
          on series.id = season.series_id
         and series.media_type = 'series'
         and series.library_id = $2
        where season.id = $3
          and not (
                season.intro_start_seconds is not null
                and season.intro_end_seconds is not null
                and season.intro_end_seconds > season.intro_start_seconds
              )
          and (
                select count(*)
                from episodes episode
                where episode.season_id = season.id
                  and exists (
                        select 1
                        from media_files media_file
                        where media_file.media_item_id = episode.media_item_id
                  )
              ) >= 3
          and not exists (
                select 1
                from season_intro_analyses analysis
                where analysis.season_id = season.id
                  and analysis.algorithm_version = $4
                  and (
                        analysis.outcome = 'no_match'
                        or (
                            analysis.outcome = 'matched'
                            and season.intro_start_seconds is not null
                            and season.intro_end_seconds is not null
                            and season.intro_end_seconds > season.intro_start_seconds
                        )
                        or (
                            analysis.outcome = 'failed'
                            and analysis.retry_after > now()
                        )
                      )
          )
        on conflict do nothing
        returning id
        "#,
    )
    .bind(INTRO_DETECTION_JOB_TYPE)
    .bind(library_id)
    .bind(season_id)
    .bind(algorithm_version)
    .fetch_optional(pool)
    .await
    .context("failed to enqueue season intro detection")?;

    Ok(inserted.is_some())
}

async fn lock_intro_detection_fence(
    tx: &mut Transaction<'_, Postgres>,
    fence: &BackgroundJobFence,
    library_id: i64,
    season_id: i64,
    algorithm_version: i32,
) -> Result<()> {
    lock_background_job_fence(tx, fence).await?;

    let valid_scope = sqlx::query_scalar::<_, i32>(
        r#"
        select 1
        from background_jobs job
        join seasons season
          on season.id = nullif(job.payload ->> 'season_id', '')::bigint
        join media_items series
          on series.id = season.series_id
         and series.media_type = 'series'
        where job.id = $1
          and job.job_type = $2
          and job.scope_type = 'library'
          and job.status = 'running'
          and job.scope_id = $3
          and series.library_id = $3
          and season.id = $4
          and nullif(job.payload ->> 'algorithm_version', '')::integer = $5
        "#,
    )
    .bind(fence.job_id)
    .bind(INTRO_DETECTION_JOB_TYPE)
    .bind(library_id)
    .bind(season_id)
    .bind(algorithm_version)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to validate intro detection job scope")?
    .is_some();

    if !valid_scope {
        bail!("intro detection background job scope is no longer valid");
    }

    Ok(())
}

pub async fn record_season_intro_analysis(
    pool: &PgPool,
    fence: &BackgroundJobFence,
    params: RecordSeasonIntroAnalysisParams,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin season intro analysis transaction")?;
    lock_intro_detection_fence(
        &mut tx,
        fence,
        params.library_id,
        params.season_id,
        params.algorithm_version,
    )
    .await?;

    sqlx::query("select id from seasons where id = $1 for update")
        .bind(params.season_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to lock season before publishing intro analysis")?;

    let current_snapshot = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        select coalesce(
            jsonb_agg(
                jsonb_build_object(
                    'episode_number', episode.episode_number,
                    'media_file_id', media_file.id,
                    'file_path', media_file.file_path,
                    'file_size', media_file.file_size,
                    'duration_seconds', media_file.duration_seconds,
                    'scan_hash', media_file.scan_hash
                )
                order by episode.episode_number asc, episode.media_item_id asc
            ),
            '[]'::jsonb
        )
        from episodes episode
        join lateral (
            select candidate.*
            from media_files candidate
            where candidate.media_item_id = episode.media_item_id
            order by candidate.created_at asc, candidate.id asc
            limit 1
        ) media_file on true
        where episode.season_id = $1
        "#,
    )
    .bind(params.season_id)
    .fetch_one(&mut *tx)
    .await
    .context("failed to verify intro analysis inputs")?;
    if current_snapshot != params.input_snapshot {
        bail!("intro_detection_inputs_changed");
    }

    sqlx::query("select set_config('mova.defer_catalog_revision', 'on', true)")
        .execute(&mut *tx)
        .await
        .context("failed to defer catalog revision for intro analysis")?;

    sqlx::query(
        r#"
        insert into season_intro_analyses (
            season_id,
            algorithm_version,
            input_fingerprint,
            outcome,
            intro_start_seconds,
            intro_end_seconds,
            confidence,
            sampled_episode_count,
            analyzed_episode_count,
            failed_episode_count,
            reason_code,
            retry_after
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, null)
        on conflict (season_id) do update
        set algorithm_version = excluded.algorithm_version,
            input_fingerprint = excluded.input_fingerprint,
            outcome = excluded.outcome,
            intro_start_seconds = excluded.intro_start_seconds,
            intro_end_seconds = excluded.intro_end_seconds,
            confidence = excluded.confidence,
            sampled_episode_count = excluded.sampled_episode_count,
            analyzed_episode_count = excluded.analyzed_episode_count,
            failed_episode_count = excluded.failed_episode_count,
            reason_code = excluded.reason_code,
            retry_after = null,
            updated_at = now()
        "#,
    )
    .bind(params.season_id)
    .bind(params.algorithm_version)
    .bind(&params.input_fingerprint)
    .bind(&params.outcome)
    .bind(params.intro_start_seconds)
    .bind(params.intro_end_seconds)
    .bind(params.confidence)
    .bind(params.sampled_episode_count)
    .bind(params.analyzed_episode_count)
    .bind(params.failed_episode_count)
    .bind(params.reason_code.as_deref())
    .execute(&mut *tx)
    .await
    .context("failed to persist season intro analysis")?;

    let updated = sqlx::query(
        r#"
        update seasons
        set intro_start_seconds = $2,
            intro_end_seconds = $3,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(params.season_id)
    .bind(params.intro_start_seconds)
    .bind(params.intro_end_seconds)
    .execute(&mut *tx)
    .await
    .context("failed to publish season intro markers")?;
    if updated.rows_affected() != 1 {
        bail!("season disappeared while recording intro analysis");
    }

    sqlx::query("select mova_bump_realtime_revision($1)")
        .bind(format!("library:{}:catalog", params.library_id))
        .execute(&mut *tx)
        .await
        .context("failed to bump catalog revision for intro analysis")?;

    tx.commit()
        .await
        .context("failed to commit season intro analysis transaction")
}

pub(crate) async fn record_season_intro_detection_failure_tx(
    tx: &mut Transaction<'_, Postgres>,
    background_job_id: i64,
    reason_code: &str,
    retry_delay_seconds: i64,
) -> Result<bool> {
    let retry_seconds = retry_delay_seconds.max(1);
    let inserted = sqlx::query_scalar::<_, i64>(
        r#"
        insert into season_intro_analyses (
            season_id,
            algorithm_version,
            input_fingerprint,
            outcome,
            sampled_episode_count,
            analyzed_episode_count,
            failed_episode_count,
            reason_code,
            retry_after
        )
        select
            nullif(job.payload ->> 'season_id', '')::bigint,
            nullif(job.payload ->> 'algorithm_version', '')::integer,
            null,
            'failed',
            0,
            0,
            0,
            $2,
            now() + make_interval(secs => $3)
        from background_jobs job
        join seasons season
          on season.id = nullif(job.payload ->> 'season_id', '')::bigint
        join media_items series
          on series.id = season.series_id
         and series.media_type = 'series'
         and series.library_id = job.scope_id
        where job.id = $1
          and job.job_type = $4
          and job.scope_type = 'library'
          and job.status = 'failed'
        on conflict (season_id) do update
        set algorithm_version = excluded.algorithm_version,
            input_fingerprint = null,
            outcome = 'failed',
            intro_start_seconds = null,
            intro_end_seconds = null,
            confidence = null,
            sampled_episode_count = 0,
            analyzed_episode_count = 0,
            failed_episode_count = 0,
            reason_code = excluded.reason_code,
            retry_after = excluded.retry_after,
            updated_at = now()
        returning season_id
        "#,
    )
    .bind(background_job_id)
    .bind(reason_code)
    .bind(retry_seconds)
    .bind(INTRO_DETECTION_JOB_TYPE)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to persist terminal intro detection failure")?;

    Ok(inserted.is_some())
}

#[cfg(test)]
mod tests {
    use super::{
        enqueue_season_intro_detection, list_intro_detection_inputs, record_season_intro_analysis,
        RecordSeasonIntroAnalysisParams, INTRO_DETECTION_JOB_TYPE,
    };
    use crate::{
        claim_background_job, enqueue_scan_job, retry_or_fail_background_job, CreateScanJobParams,
    };
    use serde_json::json;
    use sqlx::PgPool;

    async fn seed_episode_season(pool: &PgPool) -> (i64, i64, Vec<i64>) {
        let library_id = sqlx::query_scalar::<_, i64>(
            "insert into libraries (name, root_path) values ('TV', '/media/tv') returning id",
        )
        .fetch_one(pool)
        .await
        .expect("library should insert");
        let series_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, media_type, title, source_title)
            values ($1, 'series', 'Series', 'Series')
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(pool)
        .await
        .expect("series should insert");
        let season_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into seasons (library_id, series_id, season_number)
            values ($1, $2, 1)
            returning id
            "#,
        )
        .bind(library_id)
        .bind(series_id)
        .fetch_one(pool)
        .await
        .expect("season should insert");

        let mut media_file_ids = Vec::new();
        for episode_number in 1..=3 {
            let media_item_id = sqlx::query_scalar::<_, i64>(
                r#"
                insert into media_items (library_id, media_type, title, source_title)
                values ($1, 'episode', $2, $2)
                returning id
                "#,
            )
            .bind(library_id)
            .bind(format!("Episode {episode_number}"))
            .fetch_one(pool)
            .await
            .expect("episode item should insert");
            sqlx::query(
                r#"
                insert into episodes (media_item_id, library_id, season_id, episode_number)
                values ($1, $2, $3, $4)
                "#,
            )
            .bind(media_item_id)
            .bind(library_id)
            .bind(season_id)
            .bind(episode_number)
            .execute(pool)
            .await
            .expect("episode should insert");
            let media_file_id = sqlx::query_scalar::<_, i64>(
                r#"
                insert into media_files (
                    library_id,
                    media_item_id,
                    file_path,
                    file_size,
                    duration_seconds,
                    scan_hash
                )
                values ($1, $2, $3, 100, 2400, $4)
                returning id
                "#,
            )
            .bind(library_id)
            .bind(media_item_id)
            .bind(format!("/media/tv/series/S01E{episode_number:02}.mkv"))
            .bind(format!("scan-{episode_number}"))
            .fetch_one(pool)
            .await
            .expect("media file should insert");
            media_file_ids.push(media_file_id);
        }

        (library_id, season_id, media_file_ids)
    }

    fn snapshot(inputs: &[super::IntroDetectionInput]) -> serde_json::Value {
        serde_json::Value::Array(
            inputs
                .iter()
                .map(|input| {
                    json!({
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

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn enqueue_is_deduplicated_and_a_scan_cancels_running_intro_work(pool: PgPool) {
        let (library_id, season_id, _) = seed_episode_season(&pool).await;
        assert!(
            enqueue_season_intro_detection(&pool, library_id, season_id, 2)
                .await
                .expect("intro job should enqueue")
        );
        assert!(
            !enqueue_season_intro_detection(&pool, library_id, season_id, 2)
                .await
                .expect("duplicate enqueue should be harmless")
        );

        let claimed = claim_background_job(&pool, "intro-cancel-test-worker", 60)
            .await
            .expect("intro job claim should succeed")
            .claimed_job
            .expect("intro job should be claimed");
        assert_eq!(claimed.job_type, INTRO_DETECTION_JOB_TYPE);

        enqueue_scan_job(&pool, CreateScanJobParams { library_id })
            .await
            .expect("scan should enqueue");
        let status = sqlx::query_scalar::<_, String>(
            "select status from background_jobs where job_type = $1",
        )
        .bind(INTRO_DETECTION_JOB_TYPE)
        .fetch_one(&pool)
        .await
        .expect("intro job should remain auditable");
        assert_eq!(status, "cancel_requested");

        let blocked_scan = claim_background_job(&pool, "scan-must-wait-worker", 60)
            .await
            .expect("blocked scan claim should be valid")
            .claimed_job;
        assert!(blocked_scan.is_none());

        let fence = claimed
            .execution_fence()
            .expect("intro job should have a fence");
        let cancellation = retry_or_fail_background_job(&pool, &fence, "cancelled", 0)
            .await
            .expect("intro cancellation should persist")
            .expect("intro cancellation should return an outcome");
        assert_eq!(cancellation.status, "cancelled");

        let scan = claim_background_job(&pool, "scan-after-intro-worker", 60)
            .await
            .expect("scan claim should succeed")
            .claimed_job
            .expect("scan should become available after intro exits");
        assert_eq!(scan.job_type, "library.scan");
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn cancelled_intro_work_cannot_publish_a_stale_result(pool: PgPool) {
        let (library_id, season_id, _) = seed_episode_season(&pool).await;
        enqueue_season_intro_detection(&pool, library_id, season_id, 2)
            .await
            .expect("intro job should enqueue");
        let claimed = claim_background_job(&pool, "intro-fence-test-worker", 60)
            .await
            .expect("intro job claim should succeed")
            .claimed_job
            .expect("intro job should be claimed");
        let fence = claimed.execution_fence().expect("job should have a fence");
        let inputs = list_intro_detection_inputs(&pool, season_id)
            .await
            .expect("inputs should load");

        enqueue_scan_job(&pool, CreateScanJobParams { library_id })
            .await
            .expect("scan should cancel the running intro job");

        let result = record_season_intro_analysis(
            &pool,
            &fence,
            RecordSeasonIntroAnalysisParams {
                library_id,
                season_id,
                algorithm_version: 2,
                input_fingerprint: "c".repeat(64),
                input_snapshot: snapshot(&inputs),
                outcome: "matched".to_string(),
                intro_start_seconds: Some(10),
                intro_end_seconds: Some(70),
                confidence: Some(0.95),
                sampled_episode_count: 3,
                analyzed_episode_count: 3,
                failed_episode_count: 0,
                reason_code: None,
            },
        )
        .await;
        assert!(result.is_err());

        let markers = sqlx::query_as::<_, (Option<i32>, Option<i32>)>(
            "select intro_start_seconds, intro_end_seconds from seasons where id = $1",
        )
        .bind(season_id)
        .fetch_one(&pool)
        .await
        .expect("markers should load");
        assert_eq!(markers, (None, None));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn media_identity_change_invalidates_analysis_and_markers(pool: PgPool) {
        let (_, season_id, media_file_ids) = seed_episode_season(&pool).await;
        sqlx::query(
            r#"
            insert into season_intro_analyses (
                season_id,
                algorithm_version,
                input_fingerprint,
                outcome,
                intro_start_seconds,
                intro_end_seconds,
                confidence,
                sampled_episode_count,
                analyzed_episode_count,
                failed_episode_count
            )
            values ($1, 2, $2, 'matched', 12, 72, 0.94, 3, 3, 0)
            "#,
        )
        .bind(season_id)
        .bind("a".repeat(64))
        .execute(&pool)
        .await
        .expect("analysis should insert");
        sqlx::query(
            "update seasons set intro_start_seconds = 12, intro_end_seconds = 72 where id = $1",
        )
        .bind(season_id)
        .execute(&pool)
        .await
        .expect("markers should update");

        sqlx::query("update media_files set file_size = file_size + 1 where id = $1")
            .bind(media_file_ids[0])
            .execute(&pool)
            .await
            .expect("media file should update");

        let analysis_count = sqlx::query_scalar::<_, i64>(
            "select count(*) from season_intro_analyses where season_id = $1",
        )
        .bind(season_id)
        .fetch_one(&pool)
        .await
        .expect("analysis count should load");
        let markers = sqlx::query_as::<_, (Option<i32>, Option<i32>)>(
            "select intro_start_seconds, intro_end_seconds from seasons where id = $1",
        )
        .bind(season_id)
        .fetch_one(&pool)
        .await
        .expect("markers should load");
        assert_eq!(analysis_count, 0);
        assert_eq!(markers, (None, None));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn fenced_result_publishes_markers_and_catalog_revision(pool: PgPool) {
        let (library_id, season_id, _) = seed_episode_season(&pool).await;
        enqueue_season_intro_detection(&pool, library_id, season_id, 2)
            .await
            .expect("intro job should enqueue");
        let claimed = claim_background_job(&pool, "intro-test-worker", 60)
            .await
            .expect("job claim should succeed")
            .claimed_job
            .expect("intro job should be claimed");
        let fence = claimed.execution_fence().expect("job should have a fence");
        let inputs = list_intro_detection_inputs(&pool, season_id)
            .await
            .expect("inputs should load");
        let before_revision = sqlx::query_scalar::<_, i64>(
            "select coalesce(revision, 0) from realtime_revisions where resource_key = $1",
        )
        .bind(format!("library:{library_id}:catalog"))
        .fetch_optional(&pool)
        .await
        .expect("revision should load")
        .unwrap_or_default();

        record_season_intro_analysis(
            &pool,
            &fence,
            RecordSeasonIntroAnalysisParams {
                library_id,
                season_id,
                algorithm_version: 2,
                input_fingerprint: "b".repeat(64),
                input_snapshot: snapshot(&inputs),
                outcome: "matched".to_string(),
                intro_start_seconds: Some(10),
                intro_end_seconds: Some(70),
                confidence: Some(0.95),
                sampled_episode_count: 3,
                analyzed_episode_count: 3,
                failed_episode_count: 0,
                reason_code: None,
            },
        )
        .await
        .expect("fenced result should persist");

        let markers = sqlx::query_as::<_, (Option<i32>, Option<i32>)>(
            "select intro_start_seconds, intro_end_seconds from seasons where id = $1",
        )
        .bind(season_id)
        .fetch_one(&pool)
        .await
        .expect("markers should load");
        let after_revision = sqlx::query_scalar::<_, i64>(
            "select revision from realtime_revisions where resource_key = $1",
        )
        .bind(format!("library:{library_id}:catalog"))
        .fetch_one(&pool)
        .await
        .expect("revision should exist");
        assert_eq!(markers, (Some(10), Some(70)));
        assert!(after_revision > before_revision);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn workers_claim_at_most_one_intro_detection_globally(pool: PgPool) {
        let (first_library_id, first_season_id, _) = seed_episode_season(&pool).await;
        let (second_library_id, second_season_id, _) = seed_episode_season(&pool).await;
        enqueue_season_intro_detection(&pool, first_library_id, first_season_id, 2)
            .await
            .expect("first intro job should enqueue");
        enqueue_season_intro_detection(&pool, second_library_id, second_season_id, 2)
            .await
            .expect("second intro job should enqueue");

        let first = claim_background_job(&pool, "intro-worker-one", 60)
            .await
            .expect("first claim should succeed")
            .claimed_job
            .expect("one intro job should be available");
        assert_eq!(first.job_type, INTRO_DETECTION_JOB_TYPE);
        let second = claim_background_job(&pool, "intro-worker-two", 60)
            .await
            .expect("second claim query should succeed")
            .claimed_job;
        assert!(second.is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn terminal_failure_and_retry_cooldown_are_committed_atomically(pool: PgPool) {
        let (library_id, season_id, _) = seed_episode_season(&pool).await;
        enqueue_season_intro_detection(&pool, library_id, season_id, 2)
            .await
            .expect("intro job should enqueue");

        for attempt in 1..=3 {
            let claimed = claim_background_job(&pool, &format!("intro-retry-worker-{attempt}"), 60)
                .await
                .expect("intro job claim should succeed")
                .claimed_job
                .expect("intro job should be available");
            let fence = claimed.execution_fence().expect("job should have a fence");
            let outcome = retry_or_fail_background_job(&pool, &fence, "ffmpeg failed", 0)
                .await
                .expect("retry transition should succeed")
                .expect("retry transition should return an outcome");
            assert_eq!(
                outcome.status,
                if attempt == 3 { "failed" } else { "pending" }
            );
        }

        let analysis = sqlx::query_as::<_, (String, Option<String>, bool)>(
            r#"
            select outcome, reason_code, retry_after > now()
            from season_intro_analyses
            where season_id = $1
            "#,
        )
        .bind(season_id)
        .fetch_one(&pool)
        .await
        .expect("terminal failure state should already be visible");
        assert_eq!(analysis.0, "failed");
        assert_eq!(analysis.1.as_deref(), Some("attempts_exhausted"));
        assert!(analysis.2);

        assert!(
            !enqueue_season_intro_detection(&pool, library_id, season_id, 2)
                .await
                .expect("cooldown should suppress immediate re-enqueue")
        );
    }
}
