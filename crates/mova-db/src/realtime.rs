use anyhow::{Context, Result};
use mova_domain::ScanJob;
use sqlx::{postgres::PgRow, PgPool, Row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealtimeRevision {
    pub resource_key: String,
    pub revision: i64,
}

pub async fn get_realtime_server_epoch(pool: &PgPool) -> Result<String> {
    sqlx::query_scalar(
        r#"
        select server_epoch
        from realtime_system_state
        where singleton = true
        "#,
    )
    .fetch_one(pool)
    .await
    .context("failed to read realtime server epoch")
}

pub async fn list_realtime_revisions(
    pool: &PgPool,
    resource_keys: &[String],
) -> Result<Vec<RealtimeRevision>> {
    if resource_keys.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        select resource_key, revision
        from realtime_revisions
        where resource_key = any($1)
        "#,
    )
    .bind(resource_keys)
    .fetch_all(pool)
    .await
    .context("failed to list realtime revisions")?;

    Ok(rows
        .into_iter()
        .map(|row| RealtimeRevision {
            resource_key: row.get("resource_key"),
            revision: row.get("revision"),
        })
        .collect())
}

pub async fn list_active_scan_jobs(
    pool: &PgPool,
    visible_library_ids: Option<&[i64]>,
) -> Result<Vec<ScanJob>> {
    if visible_library_ids.is_some_and(|ids| ids.is_empty()) {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        select
            id,
            library_id,
            status,
            phase,
            total_files,
            scanned_files,
            local_analyzed_files,
            local_committed_files,
            remote_completed_files,
            progress_percent,
            created_at,
            started_at,
            finished_at,
            error_message
        from scan_jobs
        where status in ('pending', 'running')
          and ($1::bigint[] is null or library_id = any($1))
        order by created_at asc
        "#,
    )
    .bind(visible_library_ids)
    .fetch_all(pool)
    .await
    .context("failed to list active scan jobs")?;

    Ok(rows.into_iter().map(map_scan_job_row).collect())
}

fn map_scan_job_row(row: PgRow) -> ScanJob {
    ScanJob {
        id: row.get("id"),
        library_id: row.get("library_id"),
        status: row.get("status"),
        phase: row.get("phase"),
        total_files: row.get("total_files"),
        scanned_files: row.get("scanned_files"),
        local_analyzed_files: row.get("local_analyzed_files"),
        local_committed_files: row.get("local_committed_files"),
        remote_completed_files: row.get("remote_completed_files"),
        progress_percent: row.get("progress_percent"),
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        error_message: row.get("error_message"),
    }
}
