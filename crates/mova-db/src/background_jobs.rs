use anyhow::{Context, Result};
use sqlx::{postgres::PgRow, PgPool, Row};
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct BackgroundJob {
    pub id: i64,
    pub job_type: String,
    pub related_scan_job_id: Option<i64>,
    pub payload_json: String,
    pub status: String,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub run_after: OffsetDateTime,
    pub locked_by: Option<String>,
    pub lease_expires_at: Option<OffsetDateTime>,
}

pub async fn claim_background_job(
    pool: &PgPool,
    worker_id: &str,
    lease_seconds: i64,
) -> Result<Option<BackgroundJob>> {
    let row = sqlx::query(
        r#"
        with candidate as (
            select id
            from background_jobs
            where attempt_count < max_attempts
              and run_after <= now()
              and (
                    status = 'pending'
                    or (status = 'running' and lease_expires_at < now())
                  )
            order by run_after asc, created_at asc
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
    .fetch_optional(pool)
    .await
    .context("failed to claim background job")?;

    Ok(row.map(map_background_job_row))
}

pub async fn renew_background_job_lease(
    pool: &PgPool,
    job_id: i64,
    worker_id: &str,
    lease_seconds: i64,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        update background_jobs
        set lease_expires_at = now() + make_interval(secs => $3),
            updated_at = now()
        where id = $1
          and status = 'running'
          and locked_by = $2
        "#,
    )
    .bind(job_id)
    .bind(worker_id)
    .bind(lease_seconds)
    .execute(pool)
    .await
    .context("failed to renew background job lease")?;

    Ok(result.rows_affected() == 1)
}

pub async fn complete_background_job(pool: &PgPool, job_id: i64, worker_id: &str) -> Result<bool> {
    let result = sqlx::query(
        r#"
        update background_jobs
        set status = 'succeeded',
            locked_by = null,
            locked_at = null,
            lease_expires_at = null,
            updated_at = now(),
            finished_at = now()
        where id = $1
          and status = 'running'
          and locked_by = $2
        "#,
    )
    .bind(job_id)
    .bind(worker_id)
    .execute(pool)
    .await
    .context("failed to complete background job")?;

    Ok(result.rows_affected() == 1)
}

pub async fn retry_or_fail_background_job(
    pool: &PgPool,
    job_id: i64,
    worker_id: &str,
    error_message: &str,
    retry_delay_seconds: i64,
) -> Result<Option<String>> {
    let status = sqlx::query_scalar::<_, String>(
        r#"
        update background_jobs
        set status = case
                when attempt_count >= max_attempts then 'failed'
                else 'pending'
            end,
            run_after = case
                when attempt_count >= max_attempts then run_after
                else now() + make_interval(secs => $4)
            end,
            locked_by = null,
            locked_at = null,
            lease_expires_at = null,
            last_error = $3,
            updated_at = now(),
            finished_at = case
                when attempt_count >= max_attempts then now()
                else null
            end
        where id = $1
          and status = 'running'
          and locked_by = $2
        returning status
        "#,
    )
    .bind(job_id)
    .bind(worker_id)
    .bind(error_message)
    .bind(retry_delay_seconds)
    .fetch_optional(pool)
    .await
    .context("failed to retry or fail background job")?;

    Ok(status)
}

fn map_background_job_row(row: PgRow) -> BackgroundJob {
    BackgroundJob {
        id: row.get("id"),
        job_type: row.get("job_type"),
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
