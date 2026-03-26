use anyhow::{Context, Result};
use mova_domain::ScanJob;
use sqlx::{
    postgres::{PgPool, PgRow},
    Postgres, Row, Transaction,
};

/// 创建扫描任务时需要的参数。
#[derive(Debug)]
pub struct CreateScanJobParams {
    pub library_id: i64,
}

/// 入队扫描任务时返回的结果。
/// `created = false` 表示当前媒体库已经有活跃任务，本次直接复用它。
#[derive(Debug)]
pub struct EnqueueScanJobResult {
    pub scan_job: ScanJob,
    pub created: bool,
}

/// 新建一条 pending 状态的扫描任务记录。
pub async fn create_scan_job(pool: &PgPool, params: CreateScanJobParams) -> Result<ScanJob> {
    let row = sqlx::query(
        r#"
        insert into scan_jobs (library_id, status, total_files, scanned_files)
        values ($1, 'pending', 0, 0)
        returning
            id,
            library_id,
            status,
            total_files,
            scanned_files,
            created_at,
            started_at,
            finished_at,
            error_message
        "#,
    )
    .bind(params.library_id)
    .fetch_one(pool)
    .await
    .context("failed to create scan job")?;

    Ok(map_scan_job_row(row))
}

/// 为媒体库创建扫描任务前，先检查是否已有活跃任务。
/// 这里使用 PostgreSQL advisory lock，避免并发请求为同一个库重复创建扫描任务。
pub async fn enqueue_scan_job(
    pool: &PgPool,
    params: CreateScanJobParams,
) -> Result<EnqueueScanJobResult> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start scan job enqueue transaction")?;

    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(params.library_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to acquire scan job enqueue lock")?;

    if let Some(scan_job) = get_active_scan_job_for_library_tx(&mut tx, params.library_id).await? {
        tx.commit()
            .await
            .context("failed to commit scan job enqueue transaction")?;

        return Ok(EnqueueScanJobResult {
            scan_job,
            created: false,
        });
    }

    let row = sqlx::query(
        r#"
        insert into scan_jobs (library_id, status, total_files, scanned_files)
        values ($1, 'pending', 0, 0)
        returning
            id,
            library_id,
            status,
            total_files,
            scanned_files,
            created_at,
            started_at,
            finished_at,
            error_message
        "#,
    )
    .bind(params.library_id)
    .fetch_one(&mut *tx)
    .await
    .context("failed to create scan job")?;

    tx.commit()
        .await
        .context("failed to commit scan job enqueue transaction")?;

    Ok(EnqueueScanJobResult {
        scan_job: map_scan_job_row(row),
        created: true,
    })
}

/// 把任务状态切到 running，并记录开始时间。
/// 如果任务在启动前已经被删除，则返回 `None`。
pub async fn mark_scan_job_running(pool: &PgPool, scan_job_id: i64) -> Result<Option<ScanJob>> {
    let row = sqlx::query(
        r#"
        update scan_jobs
        set status = 'running',
            started_at = coalesce(started_at, now()),
            finished_at = null,
            error_message = null
        where id = $1
        returning
            id,
            library_id,
            status,
            total_files,
            scanned_files,
            created_at,
            started_at,
            finished_at,
            error_message
        "#,
    )
    .bind(scan_job_id)
    .fetch_optional(pool)
    .await
    .context("failed to mark scan job as running")?;

    Ok(row.map(map_scan_job_row))
}

/// 在扫描过程中更新当前已扫描文件数。
/// `total_files` 为 `None` 时保留数据库中的当前值，等最终统计完成后再补上总数。
pub async fn update_scan_job_progress(
    pool: &PgPool,
    scan_job_id: i64,
    total_files: Option<i32>,
    scanned_files: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        update scan_jobs
        set total_files = coalesce($2, total_files),
            scanned_files = $3
        where id = $1
        "#,
    )
    .bind(scan_job_id)
    .bind(total_files)
    .bind(scanned_files)
    .execute(pool)
    .await
    .context("failed to update scan job progress")?;

    Ok(())
}

/// 服务启动时把中断留下的 pending/running 任务统一标记成 failed。
/// 当前扫描任务在 API 进程内执行，重启后不会自动恢复，因此需要显式清理这些悬空任务。
pub async fn fail_incomplete_scan_jobs(pool: &PgPool, error_message: &str) -> Result<u64> {
    let result = sqlx::query(
        r#"
        update scan_jobs
        set status = 'failed',
            finished_at = now(),
            error_message = $1
        where status in ('pending', 'running')
        "#,
    )
    .bind(error_message)
    .execute(pool)
    .await
    .context("failed to mark incomplete scan jobs as failed")?;

    Ok(result.rows_affected())
}

/// 统一更新任务的终态信息，成功和失败都走这里。
/// 如果任务在终态写入前已经因为删库被级联删除，则返回 `None`。
pub async fn finalize_scan_job(
    pool: &PgPool,
    scan_job_id: i64,
    status: &str,
    total_files: i32,
    scanned_files: i32,
    error_message: Option<&str>,
) -> Result<Option<ScanJob>> {
    let row = sqlx::query(
        r#"
        update scan_jobs
        set status = $2,
            total_files = $3,
            scanned_files = $4,
            finished_at = now(),
            error_message = $5
        where id = $1
        returning
            id,
            library_id,
            status,
            total_files,
            scanned_files,
            created_at,
            started_at,
            finished_at,
            error_message
        "#,
    )
    .bind(scan_job_id)
    .bind(status)
    .bind(total_files)
    .bind(scanned_files)
    .bind(error_message)
    .fetch_optional(pool)
    .await
    .context("failed to finalize scan job")?;

    Ok(row.map(map_scan_job_row))
}

/// 读取某个媒体库的扫描历史，最新任务排在最前面。
pub async fn list_scan_jobs_for_library(pool: &PgPool, library_id: i64) -> Result<Vec<ScanJob>> {
    let rows = sqlx::query(
        r#"
        select
            id,
            library_id,
            status,
            total_files,
            scanned_files,
            created_at,
            started_at,
            finished_at,
            error_message
        from scan_jobs
        where library_id = $1
        order by created_at desc
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .context("failed to list scan jobs for library")?;

    Ok(rows.into_iter().map(map_scan_job_row).collect())
}

/// 读取某个媒体库最近一次扫描任务。
pub async fn get_latest_scan_job_for_library(
    pool: &PgPool,
    library_id: i64,
) -> Result<Option<ScanJob>> {
    let row = sqlx::query(
        r#"
        select
            id,
            library_id,
            status,
            total_files,
            scanned_files,
            created_at,
            started_at,
            finished_at,
            error_message
        from scan_jobs
        where library_id = $1
        order by created_at desc
        limit 1
        "#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .context("failed to get latest scan job for library")?;

    Ok(row.map(map_scan_job_row))
}

/// 按任务 id 读取扫描任务，供状态轮询接口使用。
pub async fn get_scan_job(pool: &PgPool, scan_job_id: i64) -> Result<Option<ScanJob>> {
    let row = sqlx::query(
        r#"
        select
            id,
            library_id,
            status,
            total_files,
            scanned_files,
            created_at,
            started_at,
            finished_at,
            error_message
        from scan_jobs
        where id = $1
        "#,
    )
    .bind(scan_job_id)
    .fetch_optional(pool)
    .await
    .context("failed to get scan job")?;

    Ok(row.map(map_scan_job_row))
}

async fn get_active_scan_job_for_library_tx(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
) -> Result<Option<ScanJob>> {
    let row = sqlx::query(
        r#"
        select
            id,
            library_id,
            status,
            total_files,
            scanned_files,
            created_at,
            started_at,
            finished_at,
            error_message
        from scan_jobs
        where library_id = $1
          and status in ('pending', 'running')
        order by created_at desc
        limit 1
        "#,
    )
    .bind(library_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to get active scan job for library")?;

    Ok(row.map(map_scan_job_row))
}

fn map_scan_job_row(row: PgRow) -> ScanJob {
    ScanJob {
        id: row.get("id"),
        library_id: row.get("library_id"),
        status: row.get("status"),
        total_files: row.get("total_files"),
        scanned_files: row.get("scanned_files"),
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        error_message: row.get("error_message"),
    }
}
