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

/// 按创建时间顺序读取媒体库列表，保证接口返回顺序稳定。
pub async fn list_libraries(pool: &PgPool) -> Result<Vec<Library>> {
    let rows = sqlx::query(
        r#"
        select id, name, description, library_type, metadata_language, root_path, created_at, updated_at
        from libraries
        order by created_at asc
        "#,
    )
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

/// 列出指定媒体库当前引用的所有 artwork 路径，供删除库前收集文件系统清理候选。
pub async fn list_library_artwork_paths(pool: &PgPool, library_id: i64) -> Result<Vec<String>> {
    let rows = sqlx::query(
        r#"
        select distinct artwork_path
        from (
            select poster_path as artwork_path
            from media_items
            where library_id = $1
            union all
            select backdrop_path as artwork_path
            from media_items
            where library_id = $1
            union all
            select s.poster_path as artwork_path
            from seasons s
            join media_items mi on mi.id = s.series_id
            where mi.library_id = $1
            union all
            select s.backdrop_path as artwork_path
            from seasons s
            join media_items mi on mi.id = s.series_id
            where mi.library_id = $1
            union all
            select cast_member.profile_path as artwork_path
            from media_item_cast_members cast_member
            join media_items mi on mi.id = cast_member.media_item_id
            where mi.library_id = $1
        ) artwork_paths
        where artwork_path is not null
          and btrim(artwork_path) <> ''
        order by artwork_path asc
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .context("failed to list library artwork paths")?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("artwork_path"))
        .collect())
}

/// 从候选 artwork 路径中返回仍被任意数据库记录引用的路径。
pub async fn list_referenced_artwork_paths(
    pool: &PgPool,
    artwork_paths: &[String],
) -> Result<Vec<String>> {
    if artwork_paths.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        select distinct artwork_path
        from (
            select poster_path as artwork_path
            from media_items
            union all
            select backdrop_path as artwork_path
            from media_items
            union all
            select poster_path as artwork_path
            from seasons
            union all
            select backdrop_path as artwork_path
            from seasons
            union all
            select profile_path as artwork_path
            from media_item_cast_members
        ) artwork_paths
        where artwork_path = any($1)
        order by artwork_path asc
        "#,
    )
    .bind(artwork_paths)
    .fetch_all(pool)
    .await
    .context("failed to list referenced artwork paths")?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("artwork_path"))
        .collect())
}

/// 删除媒体库，并显式清理该库拥有的扫描任务和媒体数据。
/// 删除前复用与扫描入队相同的 advisory lock，避免和新的扫描任务创建并发交错。
pub async fn delete_library(pool: &PgPool, library_id: i64) -> Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start library deletion transaction")?;

    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(library_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to acquire library deletion lock")?;

    for (statement, error_context) in library_cleanup_statements() {
        sqlx::query(statement)
            .bind(library_id)
            .execute(&mut *tx)
            .await
            .context(error_context)?;
    }

    let result = sqlx::query(
        r#"
        delete from libraries
        where id = $1
        "#,
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await
    .context("failed to delete library")?;

    tx.commit()
        .await
        .context("failed to commit library deletion transaction")?;

    Ok(result.rows_affected() > 0)
}

fn library_cleanup_statements() -> [(&'static str, &'static str); 15] {
    [
        (
            r#"
            delete from continue_watching cw
            using media_files mf
            where cw.media_file_id = mf.id
              and mf.library_id = $1
            "#,
            "failed to delete library continue watching items by media file",
        ),
        (
            r#"
            delete from continue_watching cw
            using media_items mi
            where (cw.media_item_id = mi.id or cw.last_played_media_item_id = mi.id)
              and mi.library_id = $1
            "#,
            "failed to delete library continue watching items by media item",
        ),
        (
            r#"
            delete from playback_progress pp
            using media_files mf
            where pp.media_file_id = mf.id
              and mf.library_id = $1
            "#,
            "failed to delete library playback progress by media file",
        ),
        (
            r#"
            delete from playback_progress pp
            using media_items mi
            where pp.media_item_id = mi.id
              and mi.library_id = $1
            "#,
            "failed to delete library playback progress by media item",
        ),
        (
            r#"
            delete from subtitle_files sf
            using media_files mf
            where sf.media_file_id = mf.id
              and mf.library_id = $1
            "#,
            "failed to delete library subtitles",
        ),
        (
            r#"
            delete from audio_tracks at
            using media_files mf
            where at.media_file_id = mf.id
              and mf.library_id = $1
            "#,
            "failed to delete library audio tracks",
        ),
        (
            r#"
            delete from media_files
            where library_id = $1
            "#,
            "failed to delete library media files",
        ),
        (
            r#"
            delete from series_episode_outline_cache outline
            using media_items mi
            where outline.series_media_item_id = mi.id
              and mi.library_id = $1
            "#,
            "failed to delete library series outline cache",
        ),
        (
            r#"
            delete from media_item_cast_members cast_member
            using media_items mi
            where cast_member.media_item_id = mi.id
              and mi.library_id = $1
            "#,
            "failed to delete library cast members",
        ),
        (
            r#"
            delete from media_item_cast_cache cast_cache
            using media_items mi
            where cast_cache.media_item_id = mi.id
              and mi.library_id = $1
            "#,
            "failed to delete library cast cache",
        ),
        (
            r#"
            delete from episodes episode
            using media_items mi
            where (episode.media_item_id = mi.id or episode.series_id = mi.id)
              and mi.library_id = $1
            "#,
            "failed to delete library episodes",
        ),
        (
            r#"
            delete from seasons season
            using media_items mi
            where season.series_id = mi.id
              and mi.library_id = $1
            "#,
            "failed to delete library seasons",
        ),
        (
            r#"
            delete from media_items
            where library_id = $1
            "#,
            "failed to delete library media items",
        ),
        (
            r#"
            delete from scan_jobs
            where library_id = $1
            "#,
            "failed to delete library scan jobs",
        ),
        (
            r#"
            delete from user_library_access
            where library_id = $1
            "#,
            "failed to delete library user access rows",
        ),
    ]
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
