use crate::{
    error::{ApplicationError, ApplicationResult},
    metadata::{normalize_metadata_language, DEFAULT_TMDB_LANGUAGE},
};
use mova_domain::{Library, LibraryDetail, LibraryVisibility};
use sqlx::postgres::PgPool;
use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

/// 应用层创建媒体库时使用的命令对象。
/// 这个结构体和 HTTP 请求体解耦，方便后面接 CLI、任务或别的入口。
#[derive(Debug)]
pub struct CreateLibraryInput {
    pub name: String,
    pub description: Option<String>,
    pub metadata_language: Option<String>,
    pub root_path: String,
}

/// 应用层更新媒体库时使用的命令对象。
#[derive(Debug)]
pub struct UpdateLibraryInput {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub metadata_language: Option<String>,
}

/// 从持久层读取当前用户可见的媒体库配置。
pub async fn list_libraries(
    pool: &PgPool,
    visibility: LibraryVisibility<'_>,
) -> ApplicationResult<Vec<Library>> {
    mova_db::list_libraries(pool, visibility.restricted_library_ids())
        .await
        .map_err(ApplicationError::from)
}

/// 按 id 获取单个媒体库，供扫描和详情类接口复用。
pub async fn get_library(pool: &PgPool, library_id: i64) -> ApplicationResult<Library> {
    let library = mova_db::get_library(pool, library_id)
        .await
        .map_err(ApplicationError::from)?;

    library.ok_or_else(|| ApplicationError::NotFound(format!("library not found: {}", library_id)))
}

/// 获取媒体库详情页所需的首屏摘要数据。
/// 这里返回库本身信息、内容数量，以及最近一次扫描结果。
pub async fn get_library_detail(
    pool: &PgPool,
    library_id: i64,
) -> ApplicationResult<LibraryDetail> {
    let library = get_library(pool, library_id).await?;
    let media_count = mova_db::count_media_items_for_library(pool, library_id)
        .await
        .map_err(ApplicationError::from)?;
    let media_type_counts = mova_db::get_library_media_type_counts(pool, library_id)
        .await
        .map_err(ApplicationError::from)?;
    let last_scan = mova_db::get_latest_scan_job_for_library(pool, library_id)
        .await
        .map_err(ApplicationError::from)?;

    Ok(LibraryDetail {
        library,
        media_count,
        movie_count: media_type_counts.movie_count,
        series_count: media_type_counts.series_count,
        last_scan,
    })
}

/// 在真正写库表之前，先完成媒体库相关的业务校验。
/// 当前规则包括：
/// 1. 名称、类型、根路径不能为空
/// 2. root_path 必须已经存在
/// 3. root_path 必须是目录，不能是普通文件
pub async fn create_library(
    pool: &PgPool,
    input: CreateLibraryInput,
) -> ApplicationResult<Library> {
    let name = input.name.trim().to_string();
    let description = normalize_optional_text(input.description);
    let metadata_language =
        normalize_metadata_language(input.metadata_language, DEFAULT_TMDB_LANGUAGE)?;
    let root_path = input.root_path.trim().to_string();

    validate_required("library name", &name)?;
    validate_required("library root path", &root_path)?;
    // 媒体库引用的是一个现有目录，而不是在创建库时偷偷帮用户创建文件夹。
    validate_root_path(&root_path)?;

    mova_db::create_library(
        pool,
        mova_db::CreateLibraryParams {
            name,
            description,
            metadata_language,
            root_path,
        },
    )
    .await
    .map_err(ApplicationError::from)
}

/// 更新媒体库基础配置。
pub async fn update_library(
    pool: &PgPool,
    library_id: i64,
    input: UpdateLibraryInput,
) -> ApplicationResult<Library> {
    let existing = get_library(pool, library_id).await?;

    if input.name.is_none() && input.description.is_none() && input.metadata_language.is_none() {
        return Err(ApplicationError::Validation(
            "at least one library field must be provided".to_string(),
        ));
    }

    let name = match input.name {
        Some(value) => {
            let value = value.trim().to_string();
            validate_required("library name", &value)?;
            value
        }
        None => existing.name.clone(),
    };

    let description = match input.description {
        Some(value) => normalize_optional_text(value),
        None => existing.description.clone(),
    };

    let metadata_language = match input.metadata_language {
        Some(value) => normalize_metadata_language(Some(value), DEFAULT_TMDB_LANGUAGE)?,
        None => existing.metadata_language.clone(),
    };

    if name == existing.name
        && description == existing.description
        && metadata_language == existing.metadata_language
    {
        return Ok(existing);
    }

    mova_db::update_library(
        pool,
        mova_db::UpdateLibraryParams {
            library_id,
            name,
            description,
            metadata_language,
        },
    )
    .await
    .map_err(ApplicationError::from)?
    .ok_or_else(|| ApplicationError::NotFound(format!("library not found: {}", library_id)))
}

/// 元数据语言变化后，把库内所有条目放回远端补全队列。
/// 文件指纹未变化时，扫描仍可复用本地分析结果，但会重新请求每个条目的远端元数据。
pub async fn prepare_library_metadata_rescan(
    pool: &PgPool,
    library_id: i64,
) -> ApplicationResult<u64> {
    get_library(pool, library_id).await?;
    mova_db::mark_library_media_for_metadata_rescan(pool, library_id)
        .await
        .map_err(ApplicationError::from)
}

/// 删除媒体库。
/// 调用方需要先确保相关后台任务已经停止；这里负责业务存在性校验、持久化删除，
/// 以及清理删除后不再被任何记录引用的 TMDB 本地 artwork 缓存文件。
pub async fn delete_library(
    pool: &PgPool,
    library_id: i64,
    artwork_cache_dir: &Path,
) -> ApplicationResult<()> {
    get_library(pool, library_id).await?;

    let artwork_paths = mova_db::list_library_artwork_paths(pool, library_id)
        .await
        .map_err(ApplicationError::from)?;

    let deleted = mova_db::delete_library(pool, library_id)
        .await
        .map_err(ApplicationError::from)?;

    if !deleted {
        return Err(ApplicationError::NotFound(format!(
            "library not found: {}",
            library_id
        )));
    }

    cleanup_deleted_library_artwork_cache(pool, artwork_cache_dir, artwork_paths).await;

    Ok(())
}

async fn cleanup_deleted_library_artwork_cache(
    pool: &PgPool,
    artwork_cache_dir: &Path,
    artwork_paths: Vec<String>,
) {
    let cached_paths = cached_tmdb_artwork_file_candidates(artwork_cache_dir, artwork_paths);

    if cached_paths.is_empty() {
        return;
    }

    let cached_path_values = cached_paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let referenced_paths =
        match mova_db::list_referenced_artwork_paths(pool, &cached_path_values).await {
            Ok(paths) => paths.into_iter().collect::<HashSet<_>>(),
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    "failed to check remaining artwork references after library deletion"
                );
                return;
            }
        };

    for cached_path in cached_paths {
        let cached_path_value = cached_path.to_string_lossy().to_string();

        if referenced_paths.contains(&cached_path_value) {
            continue;
        }

        match tokio::fs::remove_file(&cached_path).await {
            Ok(()) => {
                tracing::debug!(
                    cached_path = %cached_path.display(),
                    "deleted orphaned tmdb artwork cache file"
                );
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(
                    cached_path = %cached_path.display(),
                    error = ?error,
                    "failed to delete orphaned tmdb artwork cache file"
                );
            }
        }
    }
}

fn cached_tmdb_artwork_file_candidates(
    artwork_cache_dir: &Path,
    artwork_paths: Vec<String>,
) -> Vec<PathBuf> {
    let cache_root = artwork_cache_dir.join("tmdb");
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    for artwork_path in artwork_paths {
        let artwork_path = artwork_path.trim();

        if artwork_path.is_empty()
            || artwork_path.contains("://")
            || artwork_path.starts_with("/api/")
        {
            continue;
        }

        let path = PathBuf::from(artwork_path);

        if !path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
            || !path.starts_with(&cache_root)
        {
            continue;
        }

        let path_key = path.to_string_lossy().to_string();
        if seen.insert(path_key) {
            candidates.push(path);
        }
    }

    candidates
}

/// 通用的非空校验，避免出现只有空格的配置值。
fn validate_required(field_name: &str, value: &str) -> ApplicationResult<()> {
    if value.is_empty() {
        return Err(ApplicationError::Validation(format!(
            "{} cannot be empty",
            field_name
        )));
    }

    Ok(())
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 校验媒体库根路径是否存在，并且确实是一个目录。
fn validate_root_path(root_path: &str) -> ApplicationResult<()> {
    let metadata = match fs::metadata(Path::new(root_path)) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(ApplicationError::Validation(format!(
                "library root path does not exist: {}",
                root_path
            )));
        }
        Err(error) => {
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(
                "failed to inspect library root path {}: {}",
                root_path,
                error
            )));
        }
    };

    if !metadata.is_dir() {
        return Err(ApplicationError::Validation(format!(
            "library root path must be a directory: {}",
            root_path
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{cached_tmdb_artwork_file_candidates, validate_root_path};
    use crate::error::ApplicationError;
    use std::{env, fs, path::PathBuf};
    use uuid::Uuid;

    /// 为每个测试生成独立的临时路径，避免用例互相污染。
    fn unique_temp_path(kind: &str) -> PathBuf {
        env::temp_dir().join(format!("mova-library-{kind}-{}", Uuid::new_v4()))
    }

    #[test]
    fn validate_root_path_accepts_existing_directory() {
        let dir = unique_temp_path("dir");

        let result = (|| {
            fs::create_dir_all(&dir).unwrap();
            validate_root_path(dir.to_str().unwrap())
        })();

        let _ = fs::remove_dir_all(&dir);

        assert!(result.is_ok());
    }

    #[test]
    fn validate_root_path_rejects_missing_directory() {
        let path = unique_temp_path("missing");

        let result = validate_root_path(path.to_str().unwrap());

        assert!(matches!(
            result,
            Err(ApplicationError::Validation(message))
                if message.contains("does not exist")
        ));
    }

    #[test]
    fn validate_root_path_rejects_file_path() {
        let path = unique_temp_path("file");

        let result = (|| {
            fs::write(&path, b"not a directory").unwrap();
            validate_root_path(path.to_str().unwrap())
        })();

        let _ = fs::remove_file(&path);

        assert!(matches!(
            result,
            Err(ApplicationError::Validation(message))
                if message.contains("must be a directory")
        ));
    }

    #[test]
    fn cached_tmdb_artwork_file_candidates_accepts_only_tmdb_cache_files() {
        let cache_dir = unique_temp_path("cache");
        let cached_poster = cache_dir.join("tmdb").join("poster").join("abc123.jpg");
        let cached_backdrop = cache_dir.join("tmdb").join("backdrop").join("def456.webp");
        let sidecar_artwork = unique_temp_path("sidecar").join("poster.jpg");

        let candidates = cached_tmdb_artwork_file_candidates(
            &cache_dir,
            vec![
                cached_poster.to_string_lossy().to_string(),
                cached_poster.to_string_lossy().to_string(),
                format!("  {}  ", cached_backdrop.to_string_lossy()),
                sidecar_artwork.to_string_lossy().to_string(),
                "https://image.tmdb.org/t/p/original/poster.jpg".to_string(),
                "/api/media-items/42/poster?v=1".to_string(),
                "tmdb/poster/relative.jpg".to_string(),
                cache_dir
                    .join("tmdb")
                    .join("poster")
                    .join("..")
                    .join("escaped.jpg")
                    .to_string_lossy()
                    .to_string(),
            ],
        );

        assert_eq!(candidates, vec![cached_poster, cached_backdrop]);
    }
}
