use anyhow::{Context, Result};
use mova_domain::{Library, LibraryDetail, ScanJob};
use sqlx::{
    postgres::{PgPool, PgRow},
    Row,
};

/// 插入一条 library 记录时需要的参数。
#[derive(Debug)]
pub struct CreateLibraryParams {
    pub name: String,
    pub description: Option<String>,
    pub metadata_language: String,
    pub root_path: String,
}

/// 更新媒体库基础配置时需要的字段。
#[derive(Debug)]
pub struct UpdateLibraryParams {
    pub library_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub metadata_language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteLibraryResult {
    pub cache_cleanup_job_id: i64,
}

/// 按创建时间顺序读取可见媒体库列表，保证接口返回顺序稳定。
pub async fn list_libraries(
    pool: &PgPool,
    visible_library_ids: Option<&[i64]>,
) -> Result<Vec<Library>> {
    if visible_library_ids.is_some_and(|ids| ids.is_empty()) {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        select id, name, description, library_type, metadata_language, root_path, created_at, updated_at
        from libraries
        where $1::bigint[] is null or id = any($1)
        order by created_at asc
        "#,
    )
    .bind(visible_library_ids)
    .fetch_all(pool)
    .await
    .context("failed to list libraries")?;

    let libraries = rows.into_iter().map(map_library_row).collect();

    Ok(libraries)
}

/// 批量读取首页需要的媒体库摘要，避免首页按库重复查询统计和最近扫描。
pub async fn list_library_details(
    pool: &PgPool,
    visible_library_ids: Option<&[i64]>,
) -> Result<Vec<LibraryDetail>> {
    if visible_library_ids.is_some_and(|ids| ids.is_empty()) {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        select
            l.id,
            l.name,
            l.description,
            l.library_type,
            l.metadata_language,
            l.root_path,
            l.created_at,
            l.updated_at,
            count(mi.id) filter (where mi.media_type in ('movie', 'series')) as media_count,
            count(mi.id) filter (where mi.media_type = 'movie') as movie_count,
            count(mi.id) filter (where mi.media_type = 'series') as series_count,
            latest_scan.id as scan_id,
            latest_scan.status as scan_status,
            latest_scan.phase as scan_phase,
            latest_scan.total_files as scan_total_files,
            latest_scan.scanned_files as scan_scanned_files,
            latest_scan.local_analyzed_files as scan_local_analyzed_files,
            latest_scan.local_committed_files as scan_local_committed_files,
            latest_scan.remote_completed_files as scan_remote_completed_files,
            latest_scan.progress_percent as scan_progress_percent,
            latest_scan.created_at as scan_created_at,
            latest_scan.started_at as scan_started_at,
            latest_scan.finished_at as scan_finished_at,
            latest_scan.error_message as scan_error_message
        from libraries l
        left join media_items mi on mi.library_id = l.id
        left join lateral (
            select
                id,
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
            where library_id = l.id
            order by created_at desc, id desc
            limit 1
        ) latest_scan on true
        where $1::bigint[] is null or l.id = any($1)
        group by
            l.id,
            latest_scan.id,
            latest_scan.status,
            latest_scan.phase,
            latest_scan.total_files,
            latest_scan.scanned_files,
            latest_scan.local_analyzed_files,
            latest_scan.local_committed_files,
            latest_scan.remote_completed_files,
            latest_scan.progress_percent,
            latest_scan.created_at,
            latest_scan.started_at,
            latest_scan.finished_at,
            latest_scan.error_message
        order by l.created_at asc, l.id asc
        "#,
    )
    .bind(visible_library_ids)
    .fetch_all(pool)
    .await
    .context("failed to list library details")?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let library_id = row.get("id");
            let media_count = row.get("media_count");
            let movie_count = row.get("movie_count");
            let series_count = row.get("series_count");
            let last_scan = row.get::<Option<i64>, _>("scan_id").map(|id| ScanJob {
                id,
                library_id,
                status: row.get("scan_status"),
                phase: row.get("scan_phase"),
                total_files: row.get("scan_total_files"),
                scanned_files: row.get("scan_scanned_files"),
                local_analyzed_files: row.get("scan_local_analyzed_files"),
                local_committed_files: row.get("scan_local_committed_files"),
                remote_completed_files: row.get("scan_remote_completed_files"),
                progress_percent: row.get("scan_progress_percent"),
                created_at: row.get("scan_created_at"),
                started_at: row.get("scan_started_at"),
                finished_at: row.get("scan_finished_at"),
                error_message: row.get("scan_error_message"),
            });
            LibraryDetail {
                library: map_library_row(row),
                media_count,
                movie_count,
                series_count,
                last_scan,
            }
        })
        .collect())
}

/// 按主键读取单个媒体库，供扫描和详情类接口复用。
pub async fn get_library(pool: &PgPool, library_id: i64) -> Result<Option<Library>> {
    let row = sqlx::query(
        r#"
        select id, name, description, library_type, metadata_language, root_path, created_at, updated_at
        from libraries
        where id = $1
        "#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .context("failed to get library")?;

    Ok(row.map(map_library_row))
}

/// 把已经通过业务校验的媒体库配置写入数据库，并返回新记录。
pub async fn create_library(pool: &PgPool, params: CreateLibraryParams) -> Result<Library> {
    let row = sqlx::query(
        r#"
        insert into libraries (name, description, library_type, metadata_language, root_path)
        values ($1, $2, 'mixed', $3, $4)
        returning id, name, description, library_type, metadata_language, root_path, created_at, updated_at
        "#,
    )
    .bind(params.name)
    .bind(params.description)
    .bind(params.metadata_language)
    .bind(params.root_path)
    .fetch_one(pool)
    .await
    .context("failed to create library")?;

    Ok(map_library_row(row))
}

/// 更新媒体库配置，并返回最新记录。
pub async fn update_library(pool: &PgPool, params: UpdateLibraryParams) -> Result<Option<Library>> {
    let row = sqlx::query(
        r#"
        update libraries
        set name = $2,
            description = $3,
            metadata_language = $4,
            updated_at = now()
        where id = $1
        returning id, name, description, library_type, metadata_language, root_path, created_at, updated_at
        "#,
    )
    .bind(params.library_id)
    .bind(params.name)
    .bind(params.description)
    .bind(params.metadata_language)
    .fetch_optional(pool)
    .await
    .context("failed to update library")?;

    Ok(row.map(map_library_row))
}

/// 把库内所有条目标记为等待元数据重扫。
/// 保留现有远端绑定和展示数据，让后续扫描可以按新语言精确刷新且不出现空白闪烁。
pub async fn mark_library_media_for_metadata_rescan(pool: &PgPool, library_id: i64) -> Result<u64> {
    let result = sqlx::query(
        r#"
        update media_items
        set metadata_status = 'pending',
            metadata_failure_reason = null,
            updated_at = now()
        where library_id = $1
        "#,
    )
    .bind(library_id)
    .execute(pool)
    .await
    .context("failed to mark library media for metadata rescan")?;

    Ok(result.rows_affected())
}

/// 删除媒体库的权威数据库记录，并在同一个事务中持久化独立缓存清理任务。
/// 所有库归属数据都依靠外键级联删除；扫描后台任务会先进入取消状态并保留到执行器退出，
/// 从而保证缓存清理不会与仍在运行的扫描并发写入同一个库命名空间。
pub async fn delete_library(pool: &PgPool, library_id: i64) -> Result<Option<DeleteLibraryResult>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start library deletion transaction")?;

    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(library_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to acquire library deletion lock")?;

    let library_name = sqlx::query_scalar::<_, String>(
        r#"
        select name
        from libraries
        where id = $1
        for update
        "#,
    )
    .bind(library_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to lock library for deletion")?;
    let Some(library_name) = library_name else {
        tx.commit()
            .await
            .context("failed to commit missing library deletion transaction")?;
        return Ok(None);
    };

    sqlx::query(
        r#"
        update background_jobs
        set status = case
                when status = 'pending' then 'cancelled'
                else 'cancel_requested'
            end,
            finished_at = case
                when status = 'pending' then now()
                else finished_at
            end,
            updated_at = now()
        where job_type = 'library.scan'
          and scope_type = 'library'
          and scope_id = $1
          and status in ('pending', 'running')
        "#,
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await
    .context("failed to cancel library scan background jobs")?;

    sqlx::query("delete from libraries where id = $1")
        .bind(library_id)
        .execute(&mut *tx)
        .await
        .context("failed to cascade delete library")?;

    let cache_cleanup_job_id = sqlx::query_scalar::<_, i64>(
        r#"
        insert into background_jobs (
            job_type,
            scope_type,
            scope_id,
            payload,
            status,
            max_attempts
        )
        values (
            'library.cache.cleanup',
            'library',
            $1,
            jsonb_build_object('library_id', $1, 'library_name', $2),
            'pending',
            10
        )
        returning id
        "#,
    )
    .bind(library_id)
    .bind(library_name)
    .fetch_one(&mut *tx)
    .await
    .context("failed to enqueue library cache cleanup job")?;

    tx.commit()
        .await
        .context("failed to commit library deletion transaction")?;

    Ok(Some(DeleteLibraryResult {
        cache_cleanup_job_id,
    }))
}

/// 把 SQL 查询结果映射成领域对象，供上层统一使用。
fn map_library_row(row: PgRow) -> Library {
    Library {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        library_type: "mixed".to_string(),
        metadata_language: row.get("metadata_language"),
        root_path: row.get("root_path"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::{create_library, list_libraries, CreateLibraryParams};

    async fn seed_library(pool: &sqlx::postgres::PgPool, name: &str) -> i64 {
        create_library(
            pool,
            CreateLibraryParams {
                name: name.to_string(),
                description: None,
                metadata_language: "zh-CN".to_string(),
                root_path: format!("/media/{}", name.to_lowercase()),
            },
        )
        .await
        .unwrap()
        .id
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn list_libraries_applies_all_restricted_and_empty_visibility(
        pool: sqlx::postgres::PgPool,
    ) {
        let first_id = seed_library(&pool, "Movies").await;
        let second_id = seed_library(&pool, "Series").await;
        let third_id = seed_library(&pool, "Documentaries").await;

        let all_ids = list_libraries(&pool, None)
            .await
            .unwrap()
            .into_iter()
            .map(|library| library.id)
            .collect::<Vec<_>>();
        let restricted_ids = list_libraries(&pool, Some(&[third_id, first_id]))
            .await
            .unwrap()
            .into_iter()
            .map(|library| library.id)
            .collect::<Vec<_>>();
        let empty = list_libraries(&pool, Some(&[])).await.unwrap();

        assert_eq!(all_ids, vec![first_id, second_id, third_id]);
        assert_eq!(restricted_ids, vec![first_id, third_id]);
        assert!(empty.is_empty());
    }
}
