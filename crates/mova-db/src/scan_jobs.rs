use crate::background_jobs::{
    complete_library_scan_background_job_tx, lock_library_scan_background_job_fence,
    BackgroundJobFence, LibraryScanFenceMode,
};
use anyhow::{Context, Result};
use mova_domain::{ScanJob, ScanNotificationSummary};
use serde_json::json;
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

    let result = enqueue_scan_job_tx(&mut tx, params.library_id).await?;

    tx.commit()
        .await
        .context("failed to commit scan job enqueue transaction")?;

    Ok(result)
}

pub(crate) async fn enqueue_scan_job_tx(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
) -> Result<EnqueueScanJobResult> {
    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(library_id)
        .fetch_one(&mut **tx)
        .await
        .context("failed to acquire scan job enqueue lock")?;

    if let Some(scan_job) = get_active_scan_job_for_library_tx(tx, library_id).await? {
        return Ok(EnqueueScanJobResult {
            scan_job,
            created: false,
        });
    }

    // A new scan can replace episode files while intro analysis is reading them. Pending intro
    // work is cancelled immediately; a running worker observes cancel_requested through its lease
    // heartbeat and terminates FFmpeg before the scan mutates the catalog.
    sqlx::query(
        r#"
        update background_jobs
        set status = case
                when status = 'pending' then 'cancelled'
                else 'cancel_requested'
            end,
            locked_by = case when status = 'pending' then null else locked_by end,
            locked_at = case when status = 'pending' then null else locked_at end,
            lease_expires_at = case when status = 'pending' then null else lease_expires_at end,
            last_error = 'intro_detection_cancelled_for_library_scan',
            updated_at = now(),
            finished_at = case when status = 'pending' then now() else null end
        where job_type = 'media.intro.detect'
          and scope_type = 'library'
          and scope_id = $1
          and status in ('pending', 'running')
        "#,
    )
    .bind(library_id)
    .execute(&mut **tx)
    .await
    .context("failed to cancel intro detection before library scan")?;

    let row = sqlx::query(
        r#"
        insert into scan_jobs (library_id, status, total_files, scanned_files, progress_percent)
        values ($1, 'pending', 0, 0, 0)
        returning
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
        "#,
    )
    .bind(library_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to create scan job")?;

    let scan_job_id: i64 = row.get("id");
    sqlx::query(
        r#"
        insert into background_jobs (
            job_type,
            scope_type,
            scope_id,
            related_scan_job_id,
            payload,
            status,
            max_attempts
        )
        values (
            'library.scan',
            'library',
            $1,
            $2,
            jsonb_build_object('library_id', $1, 'scan_job_id', $2),
            'pending',
            3
        )
        "#,
    )
    .bind(library_id)
    .bind(scan_job_id)
    .execute(&mut **tx)
    .await
    .context("failed to enqueue library scan background job")?;

    Ok(EnqueueScanJobResult {
        scan_job: map_scan_job_row(row),
        created: true,
    })
}

/// 把任务状态切到 running，并记录开始时间。
/// 如果任务在启动前已经被删除，则返回 `None`。
pub async fn mark_scan_job_running(
    pool: &PgPool,
    scan_job_id: i64,
    fence: &BackgroundJobFence,
) -> Result<Option<ScanJob>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start scan job transition transaction")?;
    lock_library_scan_background_job_fence(
        &mut tx,
        fence,
        scan_job_id,
        None,
        LibraryScanFenceMode::Active,
    )
    .await?;
    let row = sqlx::query(
        r#"
        update scan_jobs
        set status = 'running',
            phase = 'discovering',
            progress_percent = greatest(progress_percent, 1),
            started_at = coalesce(started_at, now()),
            finished_at = null,
            error_message = null
        where id = $1
          and status in ('pending', 'running')
        returning
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
        "#,
    )
    .bind(scan_job_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to mark scan job as running")?;
    tx.commit()
        .await
        .context("failed to commit scan job running transition")?;

    Ok(row.map(map_scan_job_row))
}

/// 在扫描过程中更新当前已扫描文件数。
/// `total_files` 为 `None` 时保留数据库中的当前值，等最终统计完成后再补上总数。
pub async fn update_scan_job_progress(
    pool: &PgPool,
    scan_job_id: i64,
    total_files: Option<i32>,
    scanned_files: i32,
    fence: &BackgroundJobFence,
) -> Result<Option<ScanJob>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start scan progress transaction")?;
    lock_library_scan_background_job_fence(
        &mut tx,
        fence,
        scan_job_id,
        None,
        LibraryScanFenceMode::Running,
    )
    .await?;
    let row = sqlx::query(
        r#"
        update scan_jobs
        set total_files = coalesce($2, total_files),
            scanned_files = $3
        where id = $1
          and status in ('pending', 'running')
        returning
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
        "#,
    )
    .bind(scan_job_id)
    .bind(total_files)
    .bind(scanned_files)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to update scan job progress")?;
    tx.commit()
        .await
        .context("failed to commit scan progress transaction")?;

    Ok(row.map(map_scan_job_row))
}

/// 持久化扫描阶段，供重连后的 active scan 状态恢复。
pub async fn update_scan_job_phase(
    pool: &PgPool,
    scan_job_id: i64,
    phase: &str,
    progress_percent: i32,
    fence: &BackgroundJobFence,
) -> Result<Option<ScanJob>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start scan phase transaction")?;
    lock_library_scan_background_job_fence(
        &mut tx,
        fence,
        scan_job_id,
        None,
        LibraryScanFenceMode::Running,
    )
    .await?;
    let row = sqlx::query(
        r#"
        update scan_jobs
        set phase = $2,
            progress_percent = greatest(progress_percent, least(99, greatest(0, $3)))
        where id = $1
          and status in ('pending', 'running')
        returning
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
        "#,
    )
    .bind(scan_job_id)
    .bind(phase)
    .bind(progress_percent)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to update scan job phase")?;
    tx.commit()
        .await
        .context("failed to commit scan phase transaction")?;

    Ok(row.map(map_scan_job_row))
}

/// 记录一次可重试扫描尝试的失败上下文，不把父任务提前写成终态。
pub async fn record_scan_job_attempt_failure(
    pool: &PgPool,
    scan_job_id: i64,
    total_files: i32,
    scanned_files: i32,
    error_message: &str,
    fence: &BackgroundJobFence,
) -> Result<Option<ScanJob>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start scan attempt failure transaction")?;
    lock_library_scan_background_job_fence(
        &mut tx,
        fence,
        scan_job_id,
        None,
        LibraryScanFenceMode::Active,
    )
    .await?;
    let row = sqlx::query(
        r#"
        update scan_jobs
        set total_files = greatest(total_files, $2),
            scanned_files = greatest(scanned_files, $3),
            error_message = $4
        where id = $1
          and status in ('pending', 'running')
        returning
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
        "#,
    )
    .bind(scan_job_id)
    .bind(total_files.max(0))
    .bind(scanned_files.max(0))
    .bind(error_message)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to record scan job attempt failure")?;
    tx.commit()
        .await
        .context("failed to commit scan attempt failure transaction")?;

    Ok(row.map(map_scan_job_row))
}

/// 后台任务仍有重试额度时，把父扫描任务恢复为可见的 pending 状态。
pub async fn mark_scan_job_retry_pending(
    pool: &PgPool,
    scan_job_id: i64,
    error_message: &str,
    fence: &BackgroundJobFence,
) -> Result<Option<ScanJob>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start scan retry transition transaction")?;
    lock_library_scan_background_job_fence(
        &mut tx,
        fence,
        scan_job_id,
        None,
        LibraryScanFenceMode::Active,
    )
    .await?;
    let scan_job = mark_scan_job_retry_pending_tx(&mut tx, scan_job_id, error_message).await?;
    tx.commit()
        .await
        .context("failed to commit scan retry transition")?;
    Ok(scan_job)
}

pub(crate) async fn mark_scan_job_retry_pending_tx(
    tx: &mut Transaction<'_, Postgres>,
    scan_job_id: i64,
    error_message: &str,
) -> Result<Option<ScanJob>> {
    let row = sqlx::query(
        r#"
        update scan_jobs
        set status = 'pending',
            phase = null,
            finished_at = null,
            error_message = $2
        where id = $1
          and status in ('pending', 'running')
        returning
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
        "#,
    )
    .bind(scan_job_id)
    .bind(error_message)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to mark scan job as pending for retry")?;

    Ok(row.map(map_scan_job_row))
}

/// 统一更新任务的终态信息，成功、失败和取消都走这里。
/// 如果任务在终态写入前已经因为删库被级联删除，则返回 `None`。
#[allow(clippy::too_many_arguments)]
pub async fn finalize_scan_job(
    pool: &PgPool,
    library_id: i64,
    scan_job_id: i64,
    status: &str,
    total_files: i32,
    scanned_files: i32,
    error_message: Option<&str>,
    notification_summary: Option<&ScanNotificationSummary>,
    fence: &BackgroundJobFence,
) -> Result<Option<ScanJob>> {
    if !matches!(status, "success" | "cancelled") {
        anyhow::bail!("unsupported fenced scan terminal status: {status}");
    }
    let mut tx = pool
        .begin()
        .await
        .context("failed to start scan job finalization transaction")?;
    let background_status = lock_library_scan_background_job_fence(
        &mut tx,
        fence,
        scan_job_id,
        Some(library_id),
        LibraryScanFenceMode::Finalize,
    )
    .await?;
    let effective_status = if status == "cancelled" || background_status == "cancel_requested" {
        "cancelled"
    } else {
        "success"
    };
    let scan_job = finalize_scan_job_tx(
        &mut tx,
        scan_job_id,
        effective_status,
        total_files,
        scanned_files,
        error_message,
        notification_summary,
    )
    .await?;
    complete_library_scan_background_job_tx(&mut tx, fence, effective_status).await?;
    tx.commit()
        .await
        .context("failed to commit atomic scan job finalization transaction")?;

    Ok(scan_job)
}

pub(crate) async fn finalize_scan_job_tx(
    tx: &mut Transaction<'_, Postgres>,
    scan_job_id: i64,
    status: &str,
    total_files: i32,
    scanned_files: i32,
    error_message: Option<&str>,
    notification_summary: Option<&ScanNotificationSummary>,
) -> Result<Option<ScanJob>> {
    let row = sqlx::query(
        r#"
        update scan_jobs
        set status = $2,
            phase = 'finished',
            total_files = $3,
            scanned_files = $4,
            progress_percent = case when $2 = 'success' then 100 else progress_percent end,
            finished_at = now(),
            error_message = $5
        where id = $1
          and status in ('pending', 'running')
        returning
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
        "#,
    )
    .bind(scan_job_id)
    .bind(status)
    .bind(total_files)
    .bind(scanned_files)
    .bind(error_message)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to finalize scan job")?;

    let scan_job = row.map(map_scan_job_row);
    if let Some(scan_job) = scan_job.as_ref() {
        upsert_scan_job_notification(tx, scan_job, notification_summary).await?;
        sqlx::query("select mova_bump_realtime_revision($1)")
            .bind(format!("library:{}:notifications", scan_job.library_id))
            .fetch_one(&mut **tx)
            .await
            .context("failed to bump library notification revision")?;
    }

    Ok(scan_job)
}

async fn upsert_scan_job_notification(
    tx: &mut Transaction<'_, Postgres>,
    scan_job: &ScanJob,
    notification_summary: Option<&ScanNotificationSummary>,
) -> Result<()> {
    let context = sqlx::query(
        r#"
        select l.name as library_name, sj.reused_files
        from scan_jobs sj
        join libraries l on l.id = sj.library_id
        where sj.id = $1
        "#,
    )
    .bind(scan_job.id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to load scan notification context")?;
    let Some(context) = context else {
        return Ok(());
    };

    let summary_available = notification_summary.is_some();
    let empty_summary = ScanNotificationSummary::default();
    let summary = notification_summary.unwrap_or(&empty_summary);
    let has_issues = scan_notification_summary_has_issues(summary);
    let (notification_type, severity, reason_code) =
        scan_notification_presentation(&scan_job.status, has_issues);
    let payload = json!({
        "scan_job_id": scan_job.id,
        "library_id": scan_job.library_id,
        "library_name": context.get::<String, _>("library_name"),
        "status": scan_job.status,
        "summary_available": summary_available,
        "total_files": scan_job.total_files,
        "reused_files": context.get::<i32, _>("reused_files"),
        "matched_files": summary.matched_files,
        "unmatched_files": summary.unmatched_files,
        "failed_files": summary.failed_files,
        "skipped_files": summary.skipped_files,
        "probe_warning_count": summary.probe_warning_count,
        "issue_count": summary.issue_count,
        "reason_code": reason_code,
        "reason_params": {},
        "diagnostic_message": scan_job.error_message,
        "issues": summary.issues,
    });

    sqlx::query(
        r#"
        insert into notifications (
            category,
            notification_type,
            severity,
            audience,
            library_id,
            source_key,
            payload
        )
        values ('scan', $1, $2, 'library', $3, $4, $5)
        on conflict (source_key) do update
        set notification_type = excluded.notification_type,
            severity = excluded.severity,
            payload = excluded.payload,
            updated_at = now()
        "#,
    )
    .bind(notification_type)
    .bind(severity)
    .bind(scan_job.library_id)
    .bind(format!("scan-job:{}", scan_job.id))
    .bind(payload)
    .execute(&mut **tx)
    .await
    .context("failed to persist scan job notification")?;

    Ok(())
}

fn scan_notification_summary_has_issues(summary: &ScanNotificationSummary) -> bool {
    summary.issue_count > 0
        || summary.failed_files > 0
        || summary.unmatched_files > 0
        || summary.probe_warning_count > 0
}

fn scan_notification_presentation(
    status: &str,
    has_issues: bool,
) -> (&'static str, &'static str, Option<&'static str>) {
    match status {
        "failed" => ("scan.failed", "error", Some("scan_execution_failed")),
        "cancelled" => ("scan.cancelled", "info", None),
        _ if has_issues => ("scan.completed_with_issues", "warning", None),
        _ => ("scan.completed", "success", None),
    }
}

/// 读取某个媒体库的扫描历史，最新任务排在最前面。
pub async fn list_scan_jobs_for_library(pool: &PgPool, library_id: i64) -> Result<Vec<ScanJob>> {
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
        where id = $1
        "#,
    )
    .bind(scan_job_id)
    .fetch_optional(pool)
    .await
    .context("failed to get scan job")?;

    Ok(row.map(map_scan_job_row))
}

pub(crate) async fn get_scan_job_tx(
    tx: &mut Transaction<'_, Postgres>,
    scan_job_id: i64,
) -> Result<Option<ScanJob>> {
    let row = sqlx::query(
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
        where id = $1
        "#,
    )
    .bind(scan_job_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to get scan job in transaction")?;

    Ok(row.map(map_scan_job_row))
}

pub(crate) async fn get_active_scan_job_for_library_tx(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
) -> Result<Option<ScanJob>> {
    let row = sqlx::query(
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

/// 在文件树和浅层分组完成后建立本轮流水处理基线。
/// 未进入待处理集合的文件视为三个阶段都已完成；重试同一任务时会清空旧组检查点后重新规划。
pub async fn initialize_scan_job_work(
    pool: &PgPool,
    scan_job_id: i64,
    total_files: i32,
    pending_files: i32,
    fence: &BackgroundJobFence,
) -> Result<Option<ScanJob>> {
    let total_files = total_files.max(0);
    let pending_files = pending_files.clamp(0, total_files);
    let completed_files = total_files.saturating_sub(pending_files);
    let mut tx = pool
        .begin()
        .await
        .context("failed to start scan work initialization transaction")?;
    lock_library_scan_background_job_fence(
        &mut tx,
        fence,
        scan_job_id,
        None,
        LibraryScanFenceMode::Running,
    )
    .await?;

    sqlx::query("delete from scan_job_groups where scan_job_id = $1")
        .bind(scan_job_id)
        .execute(&mut *tx)
        .await
        .context("failed to reset scan group checkpoints")?;

    let row = sqlx::query(
        r#"
        update scan_jobs
        set phase = 'processing',
            total_files = $2,
            scanned_files = $2,
            reused_files = $3,
            local_analyzed_files = $3,
            local_committed_files = $3,
            remote_completed_files = $3,
            progress_percent = greatest(
                progress_percent,
                case
                    when $2 = 0 then 10
                    else least(99, floor(
                        10
                        + 20.0 * $3 / $2
                        + 20.0 * $3 / $2
                        + 49.0 * $3 / $2
                    )::integer)
                end
            )
        where id = $1
          and status in ('pending', 'running')
        returning
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
        "#,
    )
    .bind(scan_job_id)
    .bind(total_files)
    .bind(completed_files)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to initialize authoritative scan work counters")?;

    tx.commit()
        .await
        .context("failed to commit scan work initialization transaction")?;

    Ok(row.map(map_scan_job_row))
}

/// 幂等记录一个扫描组已经完成完整本地分析，并推进任务级权威进度。
/// pending 媒体数据尚未提交，因此这个检查点只写扫描工作状态，不写正式媒体表。
pub async fn mark_scan_group_analyzed(
    pool: &PgPool,
    scan_job_id: i64,
    group_key: &str,
    file_count: i32,
    fence: &BackgroundJobFence,
) -> Result<Option<ScanJob>> {
    let file_count = file_count.max(1);
    let mut tx = pool
        .begin()
        .await
        .context("failed to start analyzed scan group transaction")?;
    lock_library_scan_background_job_fence(
        &mut tx,
        fence,
        scan_job_id,
        None,
        LibraryScanFenceMode::Running,
    )
    .await?;
    let transitioned_file_count = sqlx::query_scalar::<_, i32>(
        r#"
        insert into scan_job_groups (
            scan_job_id,
            group_key,
            file_count,
            local_analyzed,
            local_committed,
            remote_completed
        )
        values ($1, $2, $3, true, false, false)
        on conflict (scan_job_id, group_key) do update
            set local_analyzed = true
        where not scan_job_groups.local_analyzed
          and scan_job_groups.file_count = excluded.file_count
        returning file_count
        "#,
    )
    .bind(scan_job_id)
    .bind(group_key)
    .bind(file_count)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to checkpoint analyzed scan group")?
    .unwrap_or(0);
    ensure_scan_group_file_count(&mut tx, scan_job_id, group_key, file_count).await?;

    let row = sqlx::query(
        r#"
        update scan_jobs
        set phase = 'processing',
            local_analyzed_files = least(
                total_files,
                local_analyzed_files + $2
            ),
            progress_percent = greatest(
                progress_percent,
                case
                    when total_files = 0 then 10
                    else least(99, floor(
                        10
                        + 20.0 * least(
                            total_files,
                            local_analyzed_files + $2
                        ) / total_files
                        + 20.0 * local_committed_files / total_files
                        + 49.0 * remote_completed_files / total_files
                    )::integer)
                end
            )
        where id = $1
          and status in ('pending', 'running')
        returning
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
        "#,
    )
    .bind(scan_job_id)
    .bind(transitioned_file_count)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to advance analyzed scan work counter")?;

    tx.commit()
        .await
        .context("failed to commit analyzed scan group transaction")?;

    Ok(row.map(map_scan_job_row))
}

async fn ensure_scan_group_file_count(
    tx: &mut Transaction<'_, Postgres>,
    scan_job_id: i64,
    group_key: &str,
    expected_file_count: i32,
) -> Result<()> {
    let stored_file_count = sqlx::query_scalar::<_, i32>(
        r#"
        select file_count
        from scan_job_groups
        where scan_job_id = $1
          and group_key = $2
        "#,
    )
    .bind(scan_job_id)
    .bind(group_key)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to validate analyzed scan group file count")?;

    match stored_file_count {
        Some(stored_file_count) if stored_file_count == expected_file_count => Ok(()),
        Some(stored_file_count) => anyhow::bail!(
            "scan group {group_key} file count changed from {stored_file_count} to {expected_file_count}"
        ),
        None => anyhow::bail!(
            "scan group {group_key} checkpoint disappeared from scan job {scan_job_id}"
        ),
    }
}

pub(crate) fn map_scan_job_row(row: PgRow) -> ScanJob {
    map_scan_job_row_ref(&row)
}

fn map_scan_job_row_ref(row: &PgRow) -> ScanJob {
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

#[cfg(test)]
mod tests {
    use super::{scan_notification_presentation, scan_notification_summary_has_issues};

    #[test]
    fn cancelled_scan_uses_a_distinct_non_failure_notification() {
        assert_eq!(
            scan_notification_presentation("cancelled", true),
            ("scan.cancelled", "info", None)
        );
    }

    #[test]
    fn failed_and_successful_scan_notifications_remain_distinct() {
        assert_eq!(
            scan_notification_presentation("failed", false),
            ("scan.failed", "error", Some("scan_execution_failed"))
        );
        assert_eq!(
            scan_notification_presentation("success", true),
            ("scan.completed_with_issues", "warning", None)
        );
        assert_eq!(
            scan_notification_presentation("success", false),
            ("scan.completed", "success", None)
        );
    }

    #[test]
    fn matched_provider_refresh_failure_still_completes_with_issues() {
        let summary = mova_domain::ScanNotificationSummary {
            matched_files: 1,
            issue_count: 1,
            ..Default::default()
        };

        assert!(scan_notification_summary_has_issues(&summary));
        assert_eq!(
            scan_notification_presentation(
                "success",
                scan_notification_summary_has_issues(&summary)
            ),
            ("scan.completed_with_issues", "warning", None)
        );
    }
}
