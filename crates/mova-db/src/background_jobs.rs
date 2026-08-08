use anyhow::{bail, Context, Result};
use mova_domain::ScanJob;
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;

pub const BACKGROUND_JOB_FENCE_LOST_MESSAGE: &str =
    "background job execution fence is no longer valid";
pub const BACKGROUND_JOB_FINAL_ATTEMPT_LEASE_EXPIRED_MESSAGE: &str =
    "background job final attempt lease expired before completion";
const TMDB_MAINTENANCE_CANCELLED_ERROR: &str = "tmdb_maintenance_cancelled";
pub(crate) const TMDB_MAINTENANCE_FAILED_ERROR: &str = "tmdb_maintenance_failed";
pub(crate) const TMDB_MAINTENANCE_RETRY_ERROR: &str = "tmdb_maintenance_retry_scheduled";
const INTRO_DETECTION_CANCELLED_ERROR: &str = "intro_detection_cancelled";
const INTRO_DETECTION_FAILED_ERROR: &str = "intro_detection_failed";
const INTRO_DETECTION_RETRY_ERROR: &str = "intro_detection_retry_scheduled";

fn is_tmdb_maintenance_job_type(job_type: &str) -> bool {
    matches!(
        job_type,
        "metadata.tmdb.revalidate" | "metadata.tmdb.artwork.cleanup"
    )
}

fn persisted_background_job_error(job_type: &str, status: &str, error_message: &str) -> String {
    if job_type == crate::INTRO_DETECTION_JOB_TYPE {
        return match status {
            "cancelled" => INTRO_DETECTION_CANCELLED_ERROR.to_string(),
            "failed" => INTRO_DETECTION_FAILED_ERROR.to_string(),
            _ => INTRO_DETECTION_RETRY_ERROR.to_string(),
        };
    }
    if !is_tmdb_maintenance_job_type(job_type) {
        return error_message.to_string();
    }

    match status {
        "cancelled" => TMDB_MAINTENANCE_CANCELLED_ERROR.to_string(),
        "failed" => TMDB_MAINTENANCE_FAILED_ERROR.to_string(),
        _ => TMDB_MAINTENANCE_RETRY_ERROR.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct BackgroundJob {
    pub id: i64,
    pub job_type: String,
    pub scope_type: String,
    pub scope_id: i64,
    pub related_scan_job_id: Option<i64>,
    pub payload_json: String,
    pub status: String,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub run_after: OffsetDateTime,
    pub locked_by: Option<String>,
    pub lease_expires_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundJobFence {
    pub job_id: i64,
    pub worker_id: String,
    pub attempt_count: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibraryScanFenceMode {
    Active,
    Running,
    Finalize,
}

#[derive(Debug, Clone)]
pub struct BackgroundJobRetryOutcome {
    pub status: String,
    pub scan_job: Option<ScanJob>,
}

#[derive(Debug, Clone)]
pub struct AbandonedBackgroundJobOutcome {
    pub job: BackgroundJob,
    pub scan_job: Option<ScanJob>,
}

#[derive(Debug, Clone)]
pub struct ClaimBackgroundJobOutcome {
    pub terminalized_jobs: Vec<AbandonedBackgroundJobOutcome>,
    pub claimed_job: Option<BackgroundJob>,
}

impl BackgroundJob {
    pub fn execution_fence(&self) -> Result<BackgroundJobFence> {
        let Some(worker_id) = self.locked_by.as_ref() else {
            bail!("{BACKGROUND_JOB_FENCE_LOST_MESSAGE}: claimed job has no owner");
        };
        if self.status != "running" || self.attempt_count <= 0 {
            bail!("{BACKGROUND_JOB_FENCE_LOST_MESSAGE}: claimed job is not running");
        }

        Ok(BackgroundJobFence {
            job_id: self.id,
            worker_id: worker_id.clone(),
            attempt_count: self.attempt_count,
        })
    }
}

pub(crate) async fn lock_background_job_fence(
    tx: &mut Transaction<'_, Postgres>,
    fence: &BackgroundJobFence,
) -> Result<()> {
    let owned = sqlx::query_scalar::<_, i32>(
        r#"
        select 1
        from background_jobs
        where id = $1
          and status in ('running', 'cancel_requested')
          and locked_by = $2
          and attempt_count = $3
          and lease_expires_at > clock_timestamp()
        for update
        "#,
    )
    .bind(fence.job_id)
    .bind(&fence.worker_id)
    .bind(fence.attempt_count)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to validate background job execution fence")?
    .is_some();

    if !owned {
        bail!(
            "{BACKGROUND_JOB_FENCE_LOST_MESSAGE}: job {} attempt {} is not owned by {}",
            fence.job_id,
            fence.attempt_count,
            fence.worker_id
        );
    }

    Ok(())
}

pub(crate) async fn lock_library_scan_background_job_fence(
    tx: &mut Transaction<'_, Postgres>,
    fence: &BackgroundJobFence,
    scan_job_id: i64,
    expected_library_id: Option<i64>,
    mode: LibraryScanFenceMode,
) -> Result<String> {
    let allow_cancel_requested = mode == LibraryScanFenceMode::Finalize;
    let require_running_scan = mode == LibraryScanFenceMode::Running;
    let owned_status = sqlx::query_scalar::<_, String>(
        r#"
        select job.status
        from background_jobs job
        join scan_jobs scan
          on scan.id = job.related_scan_job_id
        where job.id = $1
          and job.job_type = 'library.scan'
          and job.scope_type = 'library'
          and job.related_scan_job_id = $4
          and job.scope_id = scan.library_id
          and ($5::bigint is null or job.scope_id = $5)
          and (
                job.status = 'running'
                or ($6 and job.status = 'cancel_requested')
              )
          and job.locked_by = $2
          and job.attempt_count = $3
          and job.lease_expires_at > clock_timestamp()
          and scan.status in ('pending', 'running')
          and (not $7 or scan.status = 'running')
        for update of job, scan
        "#,
    )
    .bind(fence.job_id)
    .bind(&fence.worker_id)
    .bind(fence.attempt_count)
    .bind(scan_job_id)
    .bind(expected_library_id)
    .bind(allow_cancel_requested)
    .bind(require_running_scan)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to validate library scan background job execution fence")?;

    if let Some(status) = owned_status {
        return Ok(status);
    }

    // Deleting a library cascades the scan row and clears related_scan_job_id before the
    // cancelled worker gets a chance to finalize its visible scan state. In that one terminal
    // case there is no scan row left to mutate, but the fence must still be tied to the deleted
    // library scope before the caller can return a clean cancellation.
    let cascade_cancel_status = if allow_cancel_requested {
        if let Some(library_id) = expected_library_id {
            sqlx::query_scalar::<_, String>(
                r#"
                select job.status
                from background_jobs job
                where job.id = $1
                  and job.job_type = 'library.scan'
                  and job.scope_type = 'library'
                  and job.scope_id = $4
                  and job.related_scan_job_id is null
                  and job.status = 'cancel_requested'
                  and job.locked_by = $2
                  and job.attempt_count = $3
                  and job.lease_expires_at > clock_timestamp()
                  and not exists (
                        select 1
                        from scan_jobs scan
                        where scan.id = $5
                  )
                for update
                "#,
            )
            .bind(fence.job_id)
            .bind(&fence.worker_id)
            .bind(fence.attempt_count)
            .bind(library_id)
            .bind(scan_job_id)
            .fetch_optional(&mut **tx)
            .await
            .context("failed to validate cascaded library scan cancellation fence")?
        } else {
            None
        }
    } else {
        None
    };

    if let Some(status) = cascade_cancel_status {
        return Ok(status);
    }

    bail!(
        "{BACKGROUND_JOB_FENCE_LOST_MESSAGE}: scan job {} is not owned by background job {} attempt {}",
        scan_job_id,
        fence.job_id,
        fence.attempt_count
    );
}

pub(crate) async fn complete_library_scan_background_job_tx(
    tx: &mut Transaction<'_, Postgres>,
    fence: &BackgroundJobFence,
    scan_status: &str,
) -> Result<String> {
    let background_status = match scan_status {
        "success" => "succeeded",
        "cancelled" => "cancelled",
        unsupported => {
            bail!("unsupported scan terminal status for background completion: {unsupported}")
        }
    };
    let status = sqlx::query_scalar::<_, String>(
        r#"
        update background_jobs
        set status = case
                when status = 'cancel_requested' then 'cancelled'
                else $4
            end,
            locked_by = null,
            locked_at = null,
            lease_expires_at = null,
            updated_at = now(),
            finished_at = now()
        where id = $1
          and status in ('running', 'cancel_requested')
          and locked_by = $2
          and attempt_count = $3
          and lease_expires_at > clock_timestamp()
        returning status
        "#,
    )
    .bind(fence.job_id)
    .bind(&fence.worker_id)
    .bind(fence.attempt_count)
    .bind(background_status)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to complete library scan background job in terminal transaction")?;

    status.ok_or_else(|| {
        anyhow::anyhow!("{BACKGROUND_JOB_FENCE_LOST_MESSAGE}: atomic scan completion was rejected")
    })
}

pub async fn claim_background_job(
    pool: &PgPool,
    worker_id: &str,
    lease_seconds: i64,
) -> Result<ClaimBackgroundJobOutcome> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start background job claim transaction")?;
    // Claim decisions include cross-job exclusions (scan vs. metadata
    // revalidation/cleanup). Serialize the short claim transaction so two
    // workers cannot both observe the opposite job as pending and claim
    // conflicting work concurrently.
    sqlx::query("select pg_advisory_xact_lock(5066646249903483750)")
        .fetch_one(&mut *tx)
        .await
        .context("failed to serialize background job claims")?;

    // Active TMDB maintenance payloads contain provider identifiers or local
    // artwork paths only while the job can still execute. Terminal rows retain
    // local scope/status for diagnostics, and old history is removed in bounded
    // batches so it cannot grow forever.
    sqlx::query(
        r#"
        delete from background_jobs
        where id in (
            select id
            from background_jobs
            where job_type in (
                    'metadata.tmdb.revalidate',
                    'metadata.tmdb.artwork.cleanup',
                    'media.intro.detect'
                  )
              and status in ('succeeded', 'failed', 'cancelled')
              and coalesce(finished_at, updated_at) < now() - interval '30 days'
            order by coalesce(finished_at, updated_at) asc, id asc
            limit 64
        )
        "#,
    )
    .execute(&mut *tx)
    .await
    .context("failed to prune terminal TMDB maintenance jobs")?;

    let abandoned_rows = sqlx::query(
        r#"
        select
            job.id,
            job.job_type,
            job.scope_type,
            job.scope_id,
            job.related_scan_job_id,
            job.payload::text as payload_json,
            job.status,
            job.attempt_count,
            job.max_attempts,
            job.run_after,
            job.locked_by,
            job.lease_expires_at
        from background_jobs job
        where (
                job.status = 'cancel_requested'
                and job.lease_expires_at < now()
              )
           or (
                job.status = 'running'
                and job.lease_expires_at < now()
                and job.attempt_count >= job.max_attempts
        )
        order by job.created_at asc
        limit 64
        for update skip locked
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    .context("failed to lock abandoned background jobs")?;

    let mut terminalized_jobs = Vec::with_capacity(abandoned_rows.len());
    for row in abandoned_rows {
        let abandoned_job = map_background_job_row(row);
        if abandoned_job.job_type == "metadata.tmdb.artwork.cleanup"
            && abandoned_job.status == "running"
            && abandoned_job.attempt_count >= abandoned_job.max_attempts
        {
            sqlx::query(
                r#"
                update background_jobs
                set status = 'pending',
                    attempt_count = greatest(max_attempts - 1, 0),
                    run_after = now() + interval '24 hours',
                    locked_by = null,
                    locked_at = null,
                    lease_expires_at = null,
                    last_error = 'TMDB artwork cleanup lease expired; retry scheduled',
                    updated_at = now(),
                    finished_at = null
                where id = $1
                "#,
            )
            .bind(abandoned_job.id)
            .execute(&mut *tx)
            .await
            .context("failed to reschedule abandoned TMDB artwork cleanup")?;
            continue;
        }
        let Some((terminal_status, terminal_error)) = abandoned_terminal_transition(
            &abandoned_job.status,
            abandoned_job.attempt_count,
            abandoned_job.max_attempts,
        ) else {
            continue;
        };

        let scan_job = if let Some(scan_job_id) = abandoned_job.related_scan_job_id {
            let existing = crate::scan_jobs::get_scan_job_tx(&mut tx, scan_job_id).await?;
            if let Some(existing) = existing {
                crate::scan_jobs::finalize_scan_job_tx(
                    &mut tx,
                    scan_job_id,
                    terminal_status,
                    existing.total_files,
                    existing.scanned_files,
                    terminal_error,
                    None,
                )
                .await?
            } else {
                None
            }
        } else {
            None
        };
        let persisted_terminal_error = if is_tmdb_maintenance_job_type(&abandoned_job.job_type)
            || abandoned_job.job_type == crate::INTRO_DETECTION_JOB_TYPE
        {
            Some(persisted_background_job_error(
                &abandoned_job.job_type,
                terminal_status,
                terminal_error.unwrap_or_default(),
            ))
        } else {
            terminal_error.map(ToOwned::to_owned)
        };

        if abandoned_job.job_type == "metadata.tmdb.revalidate" && terminal_status == "failed" {
            let provider_item_id = serde_json::from_str::<Value>(&abandoned_job.payload_json)
                .ok()
                .and_then(|payload| {
                    payload
                        .get("provider_item_id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                });
            if let Some(provider_item_id) = provider_item_id {
                sqlx::query(
                    r#"
                    with failure_clock as materialized (
                        select clock_timestamp() as failed_at
                    )
                    update tmdb_metadata_revalidations revalidation
                    set last_attempt_at = failure_clock.failed_at,
                        next_attempt_at = failure_clock.failed_at + case
                            when consecutive_failures = 0 then interval '15 minutes'
                            when consecutive_failures = 1 then interval '1 hour'
                            when consecutive_failures = 2 then interval '6 hours'
                            else interval '24 hours'
                        end,
                        consecutive_failures = consecutive_failures + 1,
                        updated_at = failure_clock.failed_at
                    from background_jobs job, media_items item, failure_clock
                    where revalidation.media_item_id = $1
                      and revalidation.provider_item_id = $2
                      and job.id = $3
                      and item.id = revalidation.media_item_id
                      and revalidation.updated_at <= job.locked_at
                      and item.updated_at <= job.locked_at
                      and (
                            revalidation.last_attempt_at is null
                            or revalidation.last_attempt_at < job.locked_at
                          )
                    "#,
                )
                .bind(abandoned_job.scope_id)
                .bind(provider_item_id)
                .bind(abandoned_job.id)
                .execute(&mut *tx)
                .await
                .context("failed to back off abandoned TMDB metadata revalidation")?;
            }
        }

        let terminalized_row = sqlx::query(
            r#"
            update background_jobs
            set status = $2,
                locked_by = null,
                locked_at = null,
                lease_expires_at = null,
                last_error = coalesce($3, last_error),
                payload = case
                    when job_type in (
                            'metadata.tmdb.revalidate',
                            'metadata.tmdb.artwork.cleanup'
                         ) then
                        jsonb_strip_nulls(jsonb_build_object(
                            'library_id', case
                                when job_type = 'metadata.tmdb.artwork.cleanup'
                                    then to_jsonb(scope_id)
                                else payload -> 'library_id'
                            end,
                            'media_item_id', case
                                when job_type = 'metadata.tmdb.revalidate'
                                    then to_jsonb(scope_id)
                                else payload -> 'media_item_id'
                            end,
                            'maintenance', payload -> 'maintenance'
                        ))
                    else payload
                end,
                updated_at = now(),
                finished_at = now()
            where id = $1
            returning
                id,
                job_type,
                scope_type,
                scope_id,
                related_scan_job_id,
                payload::text as payload_json,
                status,
                attempt_count,
                max_attempts,
                run_after,
                locked_by,
                lease_expires_at
            "#,
        )
        .bind(abandoned_job.id)
        .bind(terminal_status)
        .bind(persisted_terminal_error.as_deref())
        .fetch_one(&mut *tx)
        .await
        .context("failed to terminalize abandoned background job")?;

        if abandoned_job.job_type == crate::INTRO_DETECTION_JOB_TYPE && terminal_status == "failed"
        {
            crate::intro_detection::record_season_intro_detection_failure_tx(
                &mut tx,
                abandoned_job.id,
                "lease_expired",
                crate::intro_detection::INTRO_DETECTION_TERMINAL_RETRY_COOLDOWN_SECONDS,
            )
            .await?;
        }

        terminalized_jobs.push(AbandonedBackgroundJobOutcome {
            job: map_background_job_row(terminalized_row),
            scan_job,
        });
    }

    let row = sqlx::query(
        r#"
        with candidate as (
            select job.id
            from background_jobs job
            where job.attempt_count < job.max_attempts
              and job.run_after <= now()
              and (
                    job.status = 'pending'
                    or (job.status = 'running' and job.lease_expires_at < now())
                  )
              and not (
                    job.job_type in (
                        'library.cache.cleanup',
                        'metadata.tmdb.artwork.cleanup'
                    )
                    and exists (
                        select 1
                        from background_jobs blocker
                        where blocker.job_type = 'library.scan'
                          and blocker.scope_type = job.scope_type
                          and blocker.scope_id = job.scope_id
                          and blocker.status in ('running', 'cancel_requested')
                    )
                  )
              and not (
                    job.job_type = 'metadata.tmdb.revalidate'
                    and exists (
                        select 1
                        from background_jobs blocker
                        where blocker.job_type = 'library.scan'
                          and blocker.scope_type = 'library'
                          and blocker.scope_id =
                              nullif(job.payload ->> 'library_id', '')::bigint
                          and (
                                blocker.status in ('running', 'cancel_requested')
                                or (
                                    blocker.status = 'pending'
                                    and coalesce(
                                        job.payload ->> 'retention_expired',
                                        'false'
                                    ) <> 'true'
                                )
                              )
                    )
                  )
              and not (
                    job.job_type = 'media.intro.detect'
                    and exists (
                        select 1
                        from background_jobs blocker
                        where blocker.id <> job.id
                          and blocker.job_type = 'media.intro.detect'
                          and blocker.status in ('running', 'cancel_requested')
                    )
                  )
              and not (
                    job.job_type = 'media.intro.detect'
                    and exists (
                        select 1
                        from background_jobs blocker
                        where blocker.job_type = 'library.scan'
                          and blocker.scope_type = 'library'
                          and blocker.scope_id = job.scope_id
                          and blocker.status in ('pending', 'running', 'cancel_requested')
                    )
                  )
              and not (
                    job.job_type = 'library.scan'
                    and exists (
                        select 1
                        from background_jobs blocker
                        where (
                                (
                                    blocker.job_type = 'metadata.tmdb.revalidate'
                                    and nullif(
                                        blocker.payload ->> 'library_id',
                                        ''
                                    )::bigint = job.scope_id
                                    and (
                                        blocker.status in ('running', 'cancel_requested')
                                        or (
                                            blocker.status = 'pending'
                                            and coalesce(
                                                blocker.payload ->> 'retention_expired',
                                                'false'
                                            ) = 'true'
                                        )
                                    )
                                )
                                or (
                                    blocker.job_type = 'metadata.tmdb.artwork.cleanup'
                                    and blocker.scope_type = 'library'
                                    and blocker.scope_id = job.scope_id
                                    and blocker.status in ('running', 'cancel_requested')
                                )
                                or (
                                    blocker.job_type = 'media.intro.detect'
                                    and blocker.scope_type = 'library'
                                    and blocker.scope_id = job.scope_id
                                    and blocker.status in ('running', 'cancel_requested')
                                )
                              )
                    )
                  )
            order by
                case
                    when job.job_type = 'metadata.tmdb.revalidate'
                         and job.payload ->> 'retention_expired' = 'true' then -1
                    when job.job_type = 'library.scan' then -1
                    when job.job_type = 'metadata.tmdb.revalidate' then 2
                    when job.job_type = 'media.intro.detect' then 1
                    else 0
                end asc,
                job.run_after asc,
                job.created_at asc
            for update skip locked
            limit 1
        )
        update background_jobs as job
        set status = 'running',
            attempt_count = job.attempt_count + 1,
            locked_by = $1,
            locked_at = now(),
            lease_expires_at = now() + make_interval(secs => $2),
            updated_at = now(),
            last_error = null,
            finished_at = null
        from candidate
        where job.id = candidate.id
        returning
            job.id,
            job.job_type,
            job.scope_type,
            job.scope_id,
            job.related_scan_job_id,
            job.payload::text as payload_json,
            job.status,
            job.attempt_count,
            job.max_attempts,
            job.run_after,
            job.locked_by,
            job.lease_expires_at
        "#,
    )
    .bind(worker_id)
    .bind(lease_seconds)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to claim background job")?;

    tx.commit()
        .await
        .context("failed to commit background job claim transaction")?;

    Ok(ClaimBackgroundJobOutcome {
        terminalized_jobs,
        claimed_job: row.map(map_background_job_row),
    })
}

fn abandoned_terminal_transition(
    status: &str,
    attempt_count: i32,
    max_attempts: i32,
) -> Option<(&'static str, Option<&'static str>)> {
    match status {
        "cancel_requested" => Some(("cancelled", Some("scan cancelled"))),
        "running" if attempt_count >= max_attempts => Some((
            "failed",
            Some(BACKGROUND_JOB_FINAL_ATTEMPT_LEASE_EXPIRED_MESSAGE),
        )),
        _ => None,
    }
}

pub async fn renew_background_job_lease(
    pool: &PgPool,
    fence: &BackgroundJobFence,
    lease_seconds: i64,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        update background_jobs
        set lease_expires_at = now() + make_interval(secs => $4),
            updated_at = now()
        where id = $1
          and status = 'running'
          and locked_by = $2
          and attempt_count = $3
          and lease_expires_at > clock_timestamp()
        "#,
    )
    .bind(fence.job_id)
    .bind(&fence.worker_id)
    .bind(fence.attempt_count)
    .bind(lease_seconds)
    .execute(pool)
    .await
    .context("failed to renew background job lease")?;

    Ok(result.rows_affected() == 1)
}

pub async fn complete_background_job(pool: &PgPool, fence: &BackgroundJobFence) -> Result<String> {
    let status = sqlx::query_scalar::<_, String>(
        r#"
        update background_jobs
        set status = case
                when status = 'cancel_requested' then 'cancelled'
                else 'succeeded'
            end,
            payload = case
                when job_type in (
                        'metadata.tmdb.revalidate',
                        'metadata.tmdb.artwork.cleanup'
                     ) then
                    jsonb_strip_nulls(jsonb_build_object(
                        'library_id', case
                            when job_type = 'metadata.tmdb.artwork.cleanup'
                                then to_jsonb(scope_id)
                            else payload -> 'library_id'
                        end,
                        'media_item_id', case
                            when job_type = 'metadata.tmdb.revalidate'
                                then to_jsonb(scope_id)
                            else payload -> 'media_item_id'
                        end,
                        'maintenance', payload -> 'maintenance'
                    ))
                else payload
            end,
            last_error = case
                when job_type in (
                        'metadata.tmdb.revalidate',
                        'metadata.tmdb.artwork.cleanup'
                     )
                     and status = 'cancel_requested'
                    then 'tmdb_maintenance_cancelled'
                when job_type in (
                        'metadata.tmdb.revalidate',
                        'metadata.tmdb.artwork.cleanup'
                     )
                    then null
                else last_error
            end,
            locked_by = null,
            locked_at = null,
            lease_expires_at = null,
            updated_at = now(),
            finished_at = now()
        where id = $1
          and status in ('running', 'cancel_requested')
          and locked_by = $2
          and attempt_count = $3
          and lease_expires_at > clock_timestamp()
        returning status
        "#,
    )
    .bind(fence.job_id)
    .bind(&fence.worker_id)
    .bind(fence.attempt_count)
    .fetch_optional(pool)
    .await
    .context("failed to complete background job")?;

    status.ok_or_else(|| {
        anyhow::anyhow!("{BACKGROUND_JOB_FENCE_LOST_MESSAGE}: completion was rejected")
    })
}

pub async fn retry_or_fail_background_job(
    pool: &PgPool,
    fence: &BackgroundJobFence,
    error_message: &str,
    retry_delay_seconds: i64,
) -> Result<Option<BackgroundJobRetryOutcome>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start background job retry transaction")?;
    lock_background_job_fence(&mut tx, fence).await?;

    let row = sqlx::query(
        r#"
        select status, job_type, max_attempts, related_scan_job_id
        from background_jobs
        where id = $1
        "#,
    )
    .bind(fence.job_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to load fenced background job retry state")?;
    let Some(row) = row else {
        bail!("{BACKGROUND_JOB_FENCE_LOST_MESSAGE}: retry state disappeared");
    };
    let current_status = row.get::<String, _>("status");
    let job_type = row.get::<String, _>("job_type");
    let max_attempts = row.get::<i32, _>("max_attempts");
    let related_scan_job_id = row.get::<Option<i64>, _>("related_scan_job_id");
    let status = if current_status == "cancel_requested" {
        "cancelled"
    } else if fence.attempt_count >= max_attempts && job_type != "metadata.tmdb.artwork.cleanup" {
        "failed"
    } else {
        "pending"
    };
    let persisted_error_message = persisted_background_job_error(&job_type, status, error_message);

    let scan_job = match (related_scan_job_id, status) {
        (Some(scan_job_id), "pending") => {
            crate::scan_jobs::mark_scan_job_retry_pending_tx(&mut tx, scan_job_id, error_message)
                .await?
        }
        (Some(scan_job_id), "failed" | "cancelled") => {
            let existing = crate::scan_jobs::get_scan_job_tx(&mut tx, scan_job_id).await?;
            if let Some(existing) = existing {
                let terminal_error = (status == "failed").then_some(error_message);
                crate::scan_jobs::finalize_scan_job_tx(
                    &mut tx,
                    scan_job_id,
                    status,
                    existing.total_files,
                    existing.scanned_files,
                    terminal_error,
                    None,
                )
                .await?
            } else {
                None
            }
        }
        _ => None,
    };

    let updated = sqlx::query_scalar::<_, String>(
        r#"
        update background_jobs
        set status = $2,
            run_after = case
                when $2 = 'pending' then now() + make_interval(secs => $4)
                else run_after
            end,
            payload = case
                when $2 in ('cancelled', 'failed')
                     and job_type in (
                        'metadata.tmdb.revalidate',
                        'metadata.tmdb.artwork.cleanup'
                     ) then
                    jsonb_strip_nulls(jsonb_build_object(
                        'library_id', case
                            when job_type = 'metadata.tmdb.artwork.cleanup'
                                then to_jsonb(scope_id)
                            else payload -> 'library_id'
                        end,
                        'media_item_id', case
                            when job_type = 'metadata.tmdb.revalidate'
                                then to_jsonb(scope_id)
                            else payload -> 'media_item_id'
                        end,
                        'maintenance', payload -> 'maintenance'
                    ))
                else payload
            end,
            attempt_count = case
                when $2 = 'pending'
                     and job_type = 'metadata.tmdb.artwork.cleanup'
                     and attempt_count >= max_attempts
                    then greatest(max_attempts - 1, 0)
                else attempt_count
            end,
            locked_by = null,
            locked_at = null,
            lease_expires_at = null,
            last_error = $3,
            updated_at = now(),
            finished_at = case
                when $2 in ('cancelled', 'failed') then now()
                else null
            end
        where id = $1
          and status in ('running', 'cancel_requested')
          and locked_by = $5
          and attempt_count = $6
        returning status
        "#,
    )
    .bind(fence.job_id)
    .bind(status)
    .bind(&persisted_error_message)
    .bind(retry_delay_seconds)
    .bind(&fence.worker_id)
    .bind(fence.attempt_count)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to retry or fail background job")?;
    let Some(updated_status) = updated else {
        bail!("{BACKGROUND_JOB_FENCE_LOST_MESSAGE}: retry transition was rejected");
    };
    if job_type == crate::INTRO_DETECTION_JOB_TYPE && updated_status == "failed" {
        crate::intro_detection::record_season_intro_detection_failure_tx(
            &mut tx,
            fence.job_id,
            "attempts_exhausted",
            crate::intro_detection::INTRO_DETECTION_TERMINAL_RETRY_COOLDOWN_SECONDS,
        )
        .await?;
    }
    tx.commit()
        .await
        .context("failed to commit background job retry transaction")?;

    Ok(Some(BackgroundJobRetryOutcome {
        status: updated_status,
        scan_job,
    }))
}

pub async fn persist_cache_cleanup_failure_notification(
    pool: &PgPool,
    job: &BackgroundJob,
    error_message: &str,
) -> Result<()> {
    if job.job_type != "library.cache.cleanup" {
        return Err(anyhow::anyhow!(
            "cache cleanup failure notification requires a library.cache.cleanup job"
        ));
    }

    let mut payload =
        serde_json::from_str::<Value>(&job.payload_json).unwrap_or_else(|_| json!({}));
    if !payload.is_object() {
        payload = json!({});
    }
    let Some(payload_object) = payload.as_object_mut() else {
        return Err(anyhow::anyhow!(
            "failed to normalize background job failure notification payload"
        ));
    };
    payload_object.insert("background_job_id".to_string(), json!(job.id));
    payload_object.insert("job_type".to_string(), json!(job.job_type));
    payload_object.insert("attempt_count".to_string(), json!(job.attempt_count));
    payload_object.insert("max_attempts".to_string(), json!(job.max_attempts));
    payload_object.insert("reason_code".to_string(), json!("cache_cleanup_failed"));
    payload_object.insert("reason_params".to_string(), json!({}));
    payload_object.insert("diagnostic_message".to_string(), json!(error_message));

    let mut tx = pool
        .begin()
        .await
        .context("failed to start background job failure notification transaction")?;
    sqlx::query(
        r#"
        insert into notifications (
            category,
            notification_type,
            severity,
            audience,
            source_key,
            payload
        )
        values ('system', 'cache.cleanup.failed', 'error', 'admin', $1, $2)
        on conflict (source_key) do update
        set severity = excluded.severity,
            payload = excluded.payload,
            updated_at = now()
        "#,
    )
    .bind(format!("background-job:{}:failed", job.id))
    .bind(payload)
    .execute(&mut *tx)
    .await
    .context("failed to persist background job failure notification")?;
    sqlx::query("select mova_bump_realtime_revision('admin:notifications')")
        .fetch_one(&mut *tx)
        .await
        .context("failed to bump admin notification revision")?;
    tx.commit()
        .await
        .context("failed to commit background job failure notification")?;
    Ok(())
}

fn map_background_job_row(row: PgRow) -> BackgroundJob {
    BackgroundJob {
        id: row.get("id"),
        job_type: row.get("job_type"),
        scope_type: row.get("scope_type"),
        scope_id: row.get("scope_id"),
        related_scan_job_id: row.get("related_scan_job_id"),
        payload_json: row.get("payload_json"),
        status: row.get("status"),
        attempt_count: row.get("attempt_count"),
        max_attempts: row.get("max_attempts"),
        run_after: row.get("run_after"),
        locked_by: row.get("locked_by"),
        lease_expires_at: row.get("lease_expires_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        abandoned_terminal_transition, claim_background_job, persisted_background_job_error,
        retry_or_fail_background_job, BackgroundJob, BACKGROUND_JOB_FENCE_LOST_MESSAGE,
        BACKGROUND_JOB_FINAL_ATTEMPT_LEASE_EXPIRED_MESSAGE, TMDB_MAINTENANCE_CANCELLED_ERROR,
        TMDB_MAINTENANCE_FAILED_ERROR, TMDB_MAINTENANCE_RETRY_ERROR,
    };
    use crate::{
        create_library, delete_library, enqueue_scan_job, get_scan_job, mark_scan_job_running,
        CreateLibraryParams, CreateScanJobParams,
    };
    use time::{Duration, OffsetDateTime};

    #[test]
    fn execution_fence_captures_claim_owner_and_attempt_generation() {
        let job = BackgroundJob {
            id: 41,
            job_type: "library.scan".to_string(),
            scope_type: "library".to_string(),
            scope_id: 7,
            related_scan_job_id: Some(11),
            payload_json: "{}".to_string(),
            status: "running".to_string(),
            attempt_count: 3,
            max_attempts: 5,
            run_after: OffsetDateTime::now_utc(),
            locked_by: Some("worker-a".to_string()),
            lease_expires_at: Some(OffsetDateTime::now_utc() + Duration::minutes(1)),
        };

        let fence = job.execution_fence().unwrap();
        assert_eq!(fence.job_id, 41);
        assert_eq!(fence.worker_id, "worker-a");
        assert_eq!(fence.attempt_count, 3);
    }

    #[test]
    fn abandoned_jobs_only_terminalize_cancelled_or_exhausted_attempts() {
        assert_eq!(
            abandoned_terminal_transition("cancel_requested", 1, 3),
            Some(("cancelled", Some("scan cancelled")))
        );
        assert_eq!(
            abandoned_terminal_transition("running", 3, 3),
            Some((
                "failed",
                Some(BACKGROUND_JOB_FINAL_ATTEMPT_LEASE_EXPIRED_MESSAGE)
            ))
        );
        assert_eq!(abandoned_terminal_transition("running", 2, 3), None);
        assert_eq!(abandoned_terminal_transition("pending", 3, 3), None);
    }

    #[test]
    fn tmdb_maintenance_errors_are_generic_when_terminal_and_bounded_when_active() {
        let diagnostic = format!(
            "provider 42 failed for /cache/libraries/1/artwork/tmdb/secret.jpg: {}",
            "细".repeat(4_096)
        );

        assert_eq!(
            persisted_background_job_error("metadata.tmdb.revalidate", "failed", &diagnostic),
            TMDB_MAINTENANCE_FAILED_ERROR
        );
        assert_eq!(
            persisted_background_job_error(
                "metadata.tmdb.artwork.cleanup",
                "cancelled",
                &diagnostic
            ),
            TMDB_MAINTENANCE_CANCELLED_ERROR
        );

        let active =
            persisted_background_job_error("metadata.tmdb.artwork.cleanup", "pending", &diagnostic);
        assert_eq!(active, TMDB_MAINTENANCE_RETRY_ERROR);
        assert!(!active.contains("42"));
        assert!(!active.contains("/cache"));
        assert_eq!(
            persisted_background_job_error("library.scan", "failed", &diagnostic),
            diagnostic
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn stale_attempt_cannot_write_after_a_new_worker_reclaims_the_job(
        pool: sqlx::postgres::PgPool,
    ) {
        let library = create_library(
            &pool,
            CreateLibraryParams {
                name: "Movies".to_string(),
                description: None,
                metadata_language: "en-US".to_string(),
                root_path: "/media/movies".to_string(),
            },
        )
        .await
        .unwrap();
        let scan_job = enqueue_scan_job(
            &pool,
            CreateScanJobParams {
                library_id: library.id,
            },
        )
        .await
        .unwrap()
        .scan_job;
        let first_claim = claim_background_job(&pool, "worker-a", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        let stale_fence = first_claim.execution_fence().unwrap();
        sqlx::query(
            "update background_jobs set lease_expires_at = now() - interval '1 second' where id = $1",
        )
        .bind(first_claim.id)
        .execute(&pool)
        .await
        .unwrap();
        let second_claim = claim_background_job(&pool, "worker-b", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        let current_fence = second_claim.execution_fence().unwrap();
        assert_eq!(current_fence.attempt_count, stale_fence.attempt_count + 1);

        let error = mark_scan_job_running(&pool, scan_job.id, &stale_fence)
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains(BACKGROUND_JOB_FENCE_LOST_MESSAGE));

        let started = mark_scan_job_running(&pool, scan_job.id, &current_fence)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(started.status, "running");
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn expired_final_attempt_terminalizes_its_parent_scan_job(pool: sqlx::postgres::PgPool) {
        let library = create_library(
            &pool,
            CreateLibraryParams {
                name: "Movies".to_string(),
                description: None,
                metadata_language: "en-US".to_string(),
                root_path: "/media/movies".to_string(),
            },
        )
        .await
        .unwrap();
        let scan_job = enqueue_scan_job(
            &pool,
            CreateScanJobParams {
                library_id: library.id,
            },
        )
        .await
        .unwrap()
        .scan_job;
        let claimed = claim_background_job(&pool, "worker-a", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        mark_scan_job_running(&pool, scan_job.id, &claimed.execution_fence().unwrap())
            .await
            .unwrap()
            .unwrap();
        sqlx::query(
            r#"
            update background_jobs
            set attempt_count = max_attempts,
                lease_expires_at = now() - interval '1 second'
            where id = $1
            "#,
        )
        .bind(claimed.id)
        .execute(&pool)
        .await
        .unwrap();

        let outcome = claim_background_job(&pool, "worker-b", 60).await.unwrap();
        assert!(outcome.claimed_job.is_none());
        assert_eq!(outcome.terminalized_jobs.len(), 1);
        assert_eq!(outcome.terminalized_jobs[0].job.status, "failed");
        assert_eq!(
            outcome.terminalized_jobs[0]
                .scan_job
                .as_ref()
                .unwrap()
                .status,
            "failed"
        );
        let persisted_scan = get_scan_job(&pool, scan_job.id).await.unwrap().unwrap();
        assert_eq!(persisted_scan.status, "failed");
        assert_eq!(
            persisted_scan.error_message.as_deref(),
            Some(BACKGROUND_JOB_FINAL_ATTEMPT_LEASE_EXPIRED_MESSAGE)
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn abandoned_cancellation_terminalizes_its_parent_scan_job(pool: sqlx::postgres::PgPool) {
        let library = create_library(
            &pool,
            CreateLibraryParams {
                name: "Movies".to_string(),
                description: None,
                metadata_language: "en-US".to_string(),
                root_path: "/media/movies".to_string(),
            },
        )
        .await
        .unwrap();
        let scan_job = enqueue_scan_job(
            &pool,
            CreateScanJobParams {
                library_id: library.id,
            },
        )
        .await
        .unwrap()
        .scan_job;
        let claimed = claim_background_job(&pool, "worker-a", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        mark_scan_job_running(&pool, scan_job.id, &claimed.execution_fence().unwrap())
            .await
            .unwrap()
            .unwrap();
        sqlx::query(
            r#"
            update background_jobs
            set status = 'cancel_requested',
                lease_expires_at = now() - interval '1 second'
            where id = $1
            "#,
        )
        .bind(claimed.id)
        .execute(&pool)
        .await
        .unwrap();

        let outcome = claim_background_job(&pool, "worker-b", 60).await.unwrap();
        assert!(outcome.claimed_job.is_none());
        assert_eq!(outcome.terminalized_jobs.len(), 1);
        assert_eq!(outcome.terminalized_jobs[0].job.status, "cancelled");
        assert_eq!(
            outcome.terminalized_jobs[0]
                .scan_job
                .as_ref()
                .unwrap()
                .status,
            "cancelled"
        );
        assert_eq!(
            get_scan_job(&pool, scan_job.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            "cancelled"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn library_cache_cleanup_waits_for_running_scan_cancellation(
        pool: sqlx::postgres::PgPool,
    ) {
        let library = create_library(
            &pool,
            CreateLibraryParams {
                name: "Movies".to_string(),
                description: None,
                metadata_language: "zh-CN".to_string(),
                root_path: "/media/movies".to_string(),
            },
        )
        .await
        .unwrap();
        enqueue_scan_job(
            &pool,
            CreateScanJobParams {
                library_id: library.id,
            },
        )
        .await
        .unwrap();

        let scan_job = claim_background_job(&pool, "scan-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        assert_eq!(scan_job.job_type, "library.scan");

        let deletion = delete_library(&pool, library.id).await.unwrap().unwrap();
        assert!(deletion.cache_cleanup_job_id > scan_job.id);
        assert!(claim_background_job(&pool, "cleanup-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .is_none());

        let fence = scan_job.execution_fence().unwrap();
        let cancellation = retry_or_fail_background_job(&pool, &fence, "cancelled", 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(cancellation.status, "cancelled");

        let cleanup_job = claim_background_job(&pool, "cleanup-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        assert_eq!(cleanup_job.job_type, "library.cache.cleanup");
        assert_eq!(cleanup_job.scope_type, "library");
        assert_eq!(cleanup_job.scope_id, library.id);
    }
}
