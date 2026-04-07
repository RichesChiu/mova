use anyhow::{Context, Result};
use mova_domain::Library;
use sqlx::{
    postgres::{PgPool, PgRow},
    Row,
};

/// 插入一条 library 记录时需要的参数。
#[derive(Debug)]
pub struct CreateLibraryParams {
    pub name: String,
    pub description: Option<String>,
    pub library_type: String,
    pub metadata_language: String,
    pub root_path: String,
    pub is_enabled: bool,
}

/// 更新媒体库基础配置时需要的字段。
#[derive(Debug)]
pub struct UpdateLibraryParams {
    pub library_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub metadata_language: String,
    pub is_enabled: bool,
}

/// 按创建时间顺序读取媒体库列表，保证接口返回顺序稳定。
pub async fn list_libraries(pool: &PgPool) -> Result<Vec<Library>> {
    let rows = sqlx::query(
        r#"
        select id, name, description, library_type, metadata_language, root_path, is_enabled, created_at, updated_at
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

/// 按主键读取单个媒体库，供扫描和详情类接口复用。
pub async fn get_library(pool: &PgPool, library_id: i64) -> Result<Option<Library>> {
    let row = sqlx::query(
        r#"
        select id, name, description, library_type, metadata_language, root_path, is_enabled, created_at, updated_at
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
        insert into libraries (name, description, library_type, metadata_language, root_path, is_enabled)
        values ($1, $2, $3, $4, $5, $6)
        returning id, name, description, library_type, metadata_language, root_path, is_enabled, created_at, updated_at
        "#,
    )
    .bind(params.name)
    .bind(params.description)
    .bind(params.library_type)
    .bind(params.metadata_language)
    .bind(params.root_path)
    .bind(params.is_enabled)
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
            is_enabled = $5,
            updated_at = now()
        where id = $1
        returning id, name, description, library_type, metadata_language, root_path, is_enabled, created_at, updated_at
        "#,
    )
    .bind(params.library_id)
    .bind(params.name)
    .bind(params.description)
    .bind(params.metadata_language)
    .bind(params.is_enabled)
    .fetch_optional(pool)
    .await
    .context("failed to update library")?;

    Ok(row.map(map_library_row))
}

/// 删除媒体库，并依赖外键级联删除它的扫描任务和媒体数据。
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

/// 把 SQL 查询结果映射成领域对象，供上层统一使用。
fn map_library_row(row: PgRow) -> Library {
    Library {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        library_type: row.get("library_type"),
        metadata_language: row.get("metadata_language"),
        root_path: row.get("root_path"),
        is_enabled: row.get("is_enabled"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
