use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgPool, Row};
use time::OffsetDateTime;

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

pub async fn claim_background_job(
    pool: &PgPool,
    worker_id: &str,
    lease_seconds: i64,
) -> Result<Option<BackgroundJob>> {
    sqlx::query(
        r#"
        update background_jobs
        set status = 'cancelled',
            locked_by = null,
            locked_at = null,
            lease_expires_at = null,
            updated_at = now(),
            finished_at = now()
        where status = 'cancel_requested'
          and (lease_expires_at is null or lease_expires_at < now())
        "#,
    )
    .execute(pool)
    .await
    .context("failed to finalize abandoned background job cancellations")?;

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
                    job.job_type = 'library.cache.cleanup'
                    and exists (
                        select 1
                        from background_jobs blocker
                        where blocker.job_type = 'library.scan'
                          and blocker.scope_type = job.scope_type
                          and blocker.scope_id = job.scope_id
                          and blocker.status in ('running', 'cancel_requested')
                    )
                  )
            order by job.run_after asc, job.created_at asc
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

pub async fn complete_background_job(
    pool: &PgPool,
    job_id: i64,
    worker_id: &str,
) -> Result<Option<String>> {
    let status = sqlx::query_scalar::<_, String>(
        r#"
        update background_jobs
        set status = case
                when status = 'cancel_requested' then 'cancelled'
                else 'succeeded'
            end,
            locked_by = null,
            locked_at = null,
            lease_expires_at = null,
            updated_at = now(),
            finished_at = now()
        where id = $1
          and status in ('running', 'cancel_requested')
          and locked_by = $2
        returning status
        "#,
    )
    .bind(job_id)
    .bind(worker_id)
    .fetch_optional(pool)
    .await
    .context("failed to complete background job")?;

    Ok(status)
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
                when status = 'cancel_requested' then 'cancelled'
                when attempt_count >= max_attempts then 'failed'
                else 'pending'
            end,
            run_after = case
                when status = 'cancel_requested' then run_after
                when attempt_count >= max_attempts then run_after
                else now() + make_interval(secs => $4)
            end,
            locked_by = null,
            locked_at = null,
            lease_expires_at = null,
            last_error = $3,
            updated_at = now(),
            finished_at = case
                when status = 'cancel_requested' then now()
                when attempt_count >= max_attempts then now()
                else null
            end
        where id = $1
          and status in ('running', 'cancel_requested')
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
    payload_object.insert("error_message".to_string(), json!(error_message));

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
    use super::{claim_background_job, retry_or_fail_background_job};
    use crate::{
        create_library, delete_library, enqueue_scan_job, CreateLibraryParams, CreateScanJobParams,
    };

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
            .unwrap();
        assert_eq!(scan_job.job_type, "library.scan");

        let deletion = delete_library(&pool, library.id).await.unwrap().unwrap();
        assert!(deletion.cache_cleanup_job_id > scan_job.id);
        assert!(claim_background_job(&pool, "cleanup-worker", 60)
            .await
            .unwrap()
            .is_none());

        let cancellation_status =
            retry_or_fail_background_job(&pool, scan_job.id, "scan-worker", "cancelled", 1)
                .await
                .unwrap();
        assert_eq!(cancellation_status.as_deref(), Some("cancelled"));

        let cleanup_job = claim_background_job(&pool, "cleanup-worker", 60)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(cleanup_job.job_type, "library.cache.cleanup");
        assert_eq!(cleanup_job.scope_type, "library");
        assert_eq!(cleanup_job.scope_id, library.id);
    }
}
