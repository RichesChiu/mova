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
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub metadata_language: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateLibraryResult {
    pub library: Library,
    pub metadata_language_changed: bool,
    pub media_items_marked_pending: u64,
    pub scan_job: Option<ScanJob>,
    pub scan_job_created: bool,
}

#[derive(Debug, Clone)]
pub enum UpdateLibraryOutcome {
    Updated(UpdateLibraryResult),
    ActiveScan(ScanJob),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilityResult<T> {
    Visible(T),
    Forbidden { library_id: i64 },
    Missing,
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
        select id, name, description, metadata_language, root_path, created_at, updated_at
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
        select id, name, description, metadata_language, root_path, created_at, updated_at
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

/// 在 SQL 中应用媒体库可见范围，并保留“资源不存在”和“存在但无权访问”的区别。
pub async fn get_library_with_visibility(
    pool: &PgPool,
    library_id: i64,
    visible_library_ids: Option<&[i64]>,
) -> Result<VisibilityResult<Library>> {
    let row = sqlx::query(
        r#"
        select
            id,
            name,
            description,
            metadata_language,
            root_path,
            created_at,
            updated_at,
            ($2::bigint[] is null or id = any($2)) as is_visible
        from libraries
        where id = $1
        "#,
    )
    .bind(library_id)
    .bind(visible_library_ids)
    .fetch_optional(pool)
    .await
    .context("failed to get visible library")?;

    Ok(match row {
        None => VisibilityResult::Missing,
        Some(row) if row.get("is_visible") => VisibilityResult::Visible(map_library_row(row)),
        Some(_) => VisibilityResult::Forbidden { library_id },
    })
}

/// 把已经通过业务校验的媒体库配置写入数据库，并返回新记录。
pub async fn create_library(pool: &PgPool, params: CreateLibraryParams) -> Result<Library> {
    let row = sqlx::query(
        r#"
        insert into libraries (name, description, metadata_language, root_path)
        values ($1, $2, $3, $4)
        returning id, name, description, metadata_language, root_path, created_at, updated_at
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

/// 原子更新媒体库配置。
///
/// 元数据语言变化时，库配置、条目待处理状态、语言相关缓存失效和 catalog revision
/// 会在同一个事务中提交，避免客户端看到一半已经更新、一半仍沿用旧语言的状态。
pub async fn update_library(
    pool: &PgPool,
    params: UpdateLibraryParams,
) -> Result<Option<UpdateLibraryOutcome>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start library update transaction")?;
    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(params.library_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to acquire library update lock")?;
    let existing = sqlx::query(
        r#"
        select id, name, description, metadata_language, root_path, created_at, updated_at
        from libraries
        where id = $1
        for update
        "#,
    )
    .bind(params.library_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to lock library for update")?;
    let Some(existing) = existing else {
        tx.commit()
            .await
            .context("failed to commit missing library update transaction")?;
        return Ok(None);
    };
    let existing = map_library_row(existing);
    let name = params.name.unwrap_or_else(|| existing.name.clone());
    let description = params
        .description
        .unwrap_or_else(|| existing.description.clone());
    let metadata_language = params
        .metadata_language
        .unwrap_or_else(|| existing.metadata_language.clone());
    let metadata_language_changed = metadata_language != existing.metadata_language;

    if name == existing.name
        && description == existing.description
        && metadata_language == existing.metadata_language
    {
        tx.commit()
            .await
            .context("failed to commit unchanged library update transaction")?;
        return Ok(Some(UpdateLibraryOutcome::Updated(UpdateLibraryResult {
            library: existing,
            metadata_language_changed: false,
            media_items_marked_pending: 0,
            scan_job: None,
            scan_job_created: false,
        })));
    }

    let scan_enqueue_result = if metadata_language_changed {
        let enqueue_result =
            crate::scan_jobs::enqueue_scan_job_tx(&mut tx, params.library_id).await?;
        if !enqueue_result.created {
            tx.commit()
                .await
                .context("failed to finish library update blocked by active scan")?;
            return Ok(Some(UpdateLibraryOutcome::ActiveScan(
                enqueue_result.scan_job,
            )));
        }
        Some(enqueue_result)
    } else {
        None
    };

    if metadata_language_changed {
        sqlx::query("select set_config('mova.defer_catalog_revision', 'on', true)")
            .fetch_one(&mut *tx)
            .await
            .context("failed to defer catalog revision during library language update")?;
    }

    let row = sqlx::query(
        r#"
        update libraries
        set name = $2,
            description = $3,
            metadata_language = $4,
            updated_at = now()
        where id = $1
        returning id, name, description, metadata_language, root_path, created_at, updated_at
        "#,
    )
    .bind(params.library_id)
    .bind(name)
    .bind(description)
    .bind(metadata_language)
    .fetch_one(&mut *tx)
    .await
    .context("failed to update library")?;
    let library = map_library_row(row);

    let (media_items_marked_pending, scan_job, scan_job_created) = if metadata_language_changed {
        let affected = sqlx::query(
            r#"
            update media_items
            set metadata_status = 'pending',
                metadata_failure_reason = null,
                updated_at = now()
            where library_id = $1
            "#,
        )
        .bind(params.library_id)
        .execute(&mut *tx)
        .await
        .context("failed to mark library media for metadata rescan")?
        .rows_affected();

        sqlx::query("delete from series_episode_outline_cache where library_id = $1")
            .bind(params.library_id)
            .execute(&mut *tx)
            .await
            .context("failed to invalidate series outline cache after language update")?;
        sqlx::query(
            r#"
            delete from media_item_cast_cache cache
            using media_items item
            where cache.media_item_id = item.id
              and item.library_id = $1
            "#,
        )
        .bind(params.library_id)
        .execute(&mut *tx)
        .await
        .context("failed to invalidate cast cache after language update")?;
        sqlx::query("select mova_bump_realtime_revision($1)")
            .bind(format!("library:{}:catalog", params.library_id))
            .fetch_one(&mut *tx)
            .await
            .context("failed to bump catalog revision after library language update")?;
        let enqueue_result = scan_enqueue_result
            .expect("metadata language changes always reserve a scan job in this transaction");
        (
            affected,
            Some(enqueue_result.scan_job),
            enqueue_result.created,
        )
    } else {
        (0, None, false)
    };

    tx.commit()
        .await
        .context("failed to commit library update transaction")?;

    Ok(Some(UpdateLibraryOutcome::Updated(UpdateLibraryResult {
        library,
        metadata_language_changed,
        media_items_marked_pending,
        scan_job,
        scan_job_created,
    })))
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
        where scope_type = 'library'
          and scope_id = $1
          and job_type in ('library.scan', 'media.intro.detect')
          and status in ('pending', 'running')
        "#,
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await
    .context("failed to cancel library-scoped analysis jobs")?;

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
        metadata_language: row.get("metadata_language"),
        root_path: row.get("root_path"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        create_library, list_libraries, update_library, CreateLibraryParams, UpdateLibraryOutcome,
        UpdateLibraryParams,
    };

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

    async fn seed_language_dependent_media(pool: &sqlx::postgres::PgPool, library_id: i64) -> i64 {
        let media_item_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (
                library_id,
                media_type,
                title,
                source_title,
                metadata_provider,
                metadata_provider_item_id,
                remote_media_type,
                metadata_status
            )
            values ($1, 'series', 'Series', 'Series', 'tmdb', '100', 'series', 'matched')
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into series_episode_outline_cache (
                series_media_item_id,
                library_id,
                outline_json,
                expires_at
            )
            values ($1, $2, '{}', now() + interval '1 day')
            "#,
        )
        .bind(media_item_id)
        .bind(library_id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_item_cast_cache (media_item_id, expires_at)
            values ($1, now() + interval '1 day')
            "#,
        )
        .bind(media_item_id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_item_cast_members (media_item_id, sort_order, name)
            values ($1, 0, 'Actor')
            "#,
        )
        .bind(media_item_id)
        .execute(pool)
        .await
        .unwrap();
        media_item_id
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn metadata_language_update_is_one_atomic_rescan_request(pool: sqlx::postgres::PgPool) {
        let library_id = seed_library(&pool, "Series").await;
        let media_item_id = seed_language_dependent_media(&pool, library_id).await;
        let revision_before = sqlx::query_scalar::<_, i64>(
            "select revision from realtime_revisions where resource_key = $1",
        )
        .bind(format!("library:{library_id}:catalog"))
        .fetch_one(&pool)
        .await
        .unwrap();

        let outcome = update_library(
            &pool,
            UpdateLibraryParams {
                library_id,
                name: Some("Series HD".to_string()),
                description: None,
                metadata_language: Some("en-US".to_string()),
            },
        )
        .await
        .unwrap()
        .unwrap();
        let UpdateLibraryOutcome::Updated(result) = outcome else {
            panic!("language update should reserve a new scan job");
        };

        assert_eq!(result.library.name, "Series HD");
        assert_eq!(result.library.metadata_language, "en-US");
        assert!(result.metadata_language_changed);
        assert_eq!(result.media_items_marked_pending, 1);
        assert!(result.scan_job_created);
        assert_eq!(result.scan_job.as_ref().unwrap().library_id, library_id);
        let metadata_status = sqlx::query_scalar::<_, String>(
            "select metadata_status from media_items where id = $1",
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(metadata_status, "pending");
        let remaining_cache_rows = sqlx::query_scalar::<_, i64>(
            r#"
            select
                (select count(*) from series_episode_outline_cache)
                + (select count(*) from media_item_cast_cache)
                + (select count(*) from media_item_cast_members)
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining_cache_rows, 0);
        let scan_background_jobs = sqlx::query_scalar::<_, i64>(
            "select count(*) from background_jobs where job_type = 'library.scan'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(scan_background_jobs, 1);
        let revision_after = sqlx::query_scalar::<_, i64>(
            "select revision from realtime_revisions where resource_key = $1",
        )
        .bind(format!("library:{library_id}:catalog"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(revision_after, revision_before + 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn scan_enqueue_failure_rolls_back_the_entire_language_update(
        pool: sqlx::postgres::PgPool,
    ) {
        let library_id = seed_library(&pool, "Series").await;
        let media_item_id = seed_language_dependent_media(&pool, library_id).await;
        sqlx::query(
            r#"
            create function reject_test_scan_enqueue()
            returns trigger
            language plpgsql
            as $$
            begin
                if new.job_type = 'library.scan' then
                    raise exception 'test scan enqueue failure';
                end if;
                return new;
            end;
            $$
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            create trigger reject_test_scan_enqueue
            before insert on background_jobs
            for each row execute function reject_test_scan_enqueue()
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let error = update_library(
            &pool,
            UpdateLibraryParams {
                library_id,
                name: Some("Series HD".to_string()),
                description: None,
                metadata_language: Some("en-US".to_string()),
            },
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("scan background job"));

        let library = super::get_library(&pool, library_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(library.name, "Series");
        assert_eq!(library.metadata_language, "zh-CN");
        let metadata_status = sqlx::query_scalar::<_, String>(
            "select metadata_status from media_items where id = $1",
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(metadata_status, "matched");
        let remaining_cache_rows = sqlx::query_scalar::<_, i64>(
            r#"
            select
                (select count(*) from series_episode_outline_cache)
                + (select count(*) from media_item_cast_cache)
                + (select count(*) from media_item_cast_members)
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining_cache_rows, 3);
        let scan_jobs = sqlx::query_scalar::<_, i64>("select count(*) from scan_jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(scan_jobs, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn active_scan_prevents_a_partial_language_update(pool: sqlx::postgres::PgPool) {
        let library_id = seed_library(&pool, "Series").await;
        let media_item_id = seed_language_dependent_media(&pool, library_id).await;
        let active_scan = crate::enqueue_scan_job(&pool, crate::CreateScanJobParams { library_id })
            .await
            .unwrap()
            .scan_job;

        let outcome = update_library(
            &pool,
            UpdateLibraryParams {
                library_id,
                name: None,
                description: None,
                metadata_language: Some("en-US".to_string()),
            },
        )
        .await
        .unwrap()
        .unwrap();

        assert!(matches!(
            outcome,
            UpdateLibraryOutcome::ActiveScan(scan_job) if scan_job.id == active_scan.id
        ));
        let library = super::get_library(&pool, library_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(library.metadata_language, "zh-CN");
        let metadata_status = sqlx::query_scalar::<_, String>(
            "select metadata_status from media_items where id = $1",
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(metadata_status, "matched");
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn concurrent_manual_scan_and_language_update_share_one_enqueue_lock(
        pool: sqlx::postgres::PgPool,
    ) {
        let library_id = seed_library(&pool, "Series").await;
        seed_language_dependent_media(&pool, library_id).await;
        let update_pool = pool.clone();
        let enqueue_pool = pool.clone();

        let (update_result, enqueue_result) = tokio::join!(
            async move {
                update_library(
                    &update_pool,
                    UpdateLibraryParams {
                        library_id,
                        name: None,
                        description: None,
                        metadata_language: Some("en-US".to_string()),
                    },
                )
                .await
            },
            async move {
                crate::enqueue_scan_job(&enqueue_pool, crate::CreateScanJobParams { library_id })
                    .await
            }
        );

        let update_outcome = update_result.unwrap().unwrap();
        let enqueue_outcome = enqueue_result.unwrap();
        assert!(
            enqueue_outcome.created
                || matches!(
                    &update_outcome,
                    UpdateLibraryOutcome::Updated(result) if result.scan_job_created
                ),
            "one caller must create the authoritative scan job"
        );
        let active_scan_count = sqlx::query_scalar::<_, i64>(
            "select count(*) from scan_jobs where library_id = $1 and status in ('pending', 'running')",
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(active_scan_count, 1);
        let language = super::get_library(&pool, library_id)
            .await
            .unwrap()
            .unwrap()
            .metadata_language;
        match update_outcome {
            UpdateLibraryOutcome::Updated(_) => assert_eq!(language, "en-US"),
            UpdateLibraryOutcome::ActiveScan(_) => assert_eq!(language, "zh-CN"),
        }
    }
}
