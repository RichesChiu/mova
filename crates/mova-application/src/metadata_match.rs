use crate::{
    error::{ApplicationError, ApplicationResult},
    invalidate_media_item_cast_cache,
    libraries::get_library,
    media_classification::metadata_lookup_type_for_media_type,
    media_items::get_media_item,
    metadata::{MetadataLookup, MetadataProvider, RemoteMetadataSearchResult},
};
use mova_domain::MediaItem;
use sqlx::postgres::PgPool;
use std::sync::Arc;

const TMDB_PROVIDER_NAME: &str = "tmdb";

#[derive(Debug, Clone)]
pub struct SearchMetadataMatchesInput {
    pub query: String,
    pub year: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct MetadataMatchCandidate {
    pub provider_item_id: i64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ApplyMetadataMatchInput {
    pub provider_item_id: i64,
}

/// 管理员手动搜索候选元数据时，始终按当前媒体项类型和所属库语言去搜。
/// 这样搜索结果和后续真正应用时使用的是同一条 provider 语义。
pub async fn search_media_item_metadata_matches(
    pool: &PgPool,
    media_item_id: i64,
    input: SearchMetadataMatchesInput,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> ApplicationResult<Vec<MetadataMatchCandidate>> {
    ensure_metadata_provider_enabled(metadata_provider.as_ref())?;
    let media_item = get_media_item(pool, media_item_id).await?;
    let lookup_type = metadata_lookup_type_for_media_item(&media_item)?;
    let library = get_library(pool, media_item.library_id).await?;
    let query = normalize_match_query(input.query)?;
    let year = normalize_match_year(input.year)?;
    let lookup = MetadataLookup {
        title: query,
        year,
        library_type: lookup_type.to_string(),
        language: Some(library.metadata_language),
        provider_item_id: None,
    };

    metadata_provider
        .search(&lookup)
        .await
        .map(|results| {
            results
                .into_iter()
                .map(map_match_candidate)
                .collect::<Vec<_>>()
        })
        .map_err(ApplicationError::from)
}

/// 用户明确选中一个候选条目后，把绑定和元数据一起写回当前媒体项。
/// 绑定字段会让后续演员和剧集大纲走精确 TMDB id，而不是再依赖模糊搜索。
pub async fn apply_media_item_metadata_match(
    pool: &PgPool,
    media_item_id: i64,
    input: ApplyMetadataMatchInput,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> ApplicationResult<MediaItem> {
    ensure_metadata_provider_enabled(metadata_provider.as_ref())?;
    let media_item = get_media_item(pool, media_item_id).await?;
    let lookup_type = metadata_lookup_type_for_media_item(&media_item)?;
    let library = get_library(pool, media_item.library_id).await?;
    let lookup = MetadataLookup {
        title: media_item
            .original_title
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(media_item.title.as_str())
            .to_string(),
        year: media_item.year,
        library_type: lookup_type.to_string(),
        language: Some(library.metadata_language),
        provider_item_id: Some(input.provider_item_id),
    };

    let remote_metadata = metadata_provider.lookup(&lookup).await.map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "failed to fetch selected metadata candidate for media item {}: {}",
            media_item_id,
            error
        ))
    })?;
    let remote_metadata = remote_metadata.ok_or_else(|| {
        ApplicationError::NotFound(format!(
            "metadata candidate {} was not found",
            input.provider_item_id
        ))
    })?;

    let updated_media_item = mova_db::update_media_item_metadata(
        pool,
        media_item_id,
        mova_db::UpdateMediaItemMetadataParams {
            title: remote_metadata.title.unwrap_or(media_item.title),
            original_title: remote_metadata.original_title.or(media_item.original_title),
            // 手动匹配目前不会单独生成 sort title，因此保留现有值避免意外清空。
            sort_title: media_item.sort_title,
            metadata_provider: Some(TMDB_PROVIDER_NAME.to_string()),
            metadata_provider_item_id: Some(input.provider_item_id),
            year: remote_metadata.year.or(media_item.year),
            overview: remote_metadata.overview.or(media_item.overview),
            poster_path: remote_metadata.poster_path.or(media_item.poster_path),
            backdrop_path: remote_metadata.backdrop_path.or(media_item.backdrop_path),
        },
    )
    .await
    .map_err(ApplicationError::from)?
    .ok_or_else(|| {
        ApplicationError::NotFound(format!("media item not found: {}", media_item_id))
    })?;

    if media_item.media_type.eq_ignore_ascii_case("series") {
        mova_db::delete_series_episode_outline_cache(pool, media_item.id)
            .await
            .map_err(ApplicationError::from)?;
    }
    invalidate_media_item_cast_cache(pool, media_item.id).await?;

    Ok(updated_media_item)
}

fn ensure_metadata_provider_enabled(
    metadata_provider: &dyn MetadataProvider,
) -> ApplicationResult<()> {
    if metadata_provider.is_enabled() {
        return Ok(());
    }

    Err(ApplicationError::Conflict(
        "metadata provider is disabled".to_string(),
    ))
}

fn metadata_lookup_type_for_media_item(media_item: &MediaItem) -> ApplicationResult<&str> {
    if media_item.media_type.eq_ignore_ascii_case("episode") {
        return Err(ApplicationError::Validation(format!(
            "manual metadata matching is not supported for media type {}",
            media_item.media_type
        )));
    }

    let lookup_type = metadata_lookup_type_for_media_type(&media_item.media_type);
    if lookup_type.eq_ignore_ascii_case("series") || lookup_type.eq_ignore_ascii_case("movie") {
        return Ok(lookup_type);
    }

    Err(ApplicationError::Validation(format!(
        "manual metadata matching is not supported for media type {}",
        media_item.media_type
    )))
}

fn normalize_match_query(query: String) -> ApplicationResult<String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Err(ApplicationError::Validation(
            "metadata search query cannot be empty".to_string(),
        ));
    }

    Ok(query)
}

fn normalize_match_year(year: Option<i32>) -> ApplicationResult<Option<i32>> {
    match year {
        Some(value) if value <= 0 => Err(ApplicationError::Validation(
            "metadata search year must be a positive integer".to_string(),
        )),
        Some(value) => Ok(Some(value)),
        None => Ok(None),
    }
}

fn map_match_candidate(candidate: RemoteMetadataSearchResult) -> MetadataMatchCandidate {
    MetadataMatchCandidate {
        provider_item_id: candidate.provider_item_id,
        title: candidate.title,
        original_title: candidate.original_title,
        year: candidate.year,
        overview: candidate.overview,
        poster_path: candidate.poster_path,
        backdrop_path: candidate.backdrop_path,
    }
}

#[cfg(test)]
mod tests {
    use super::{metadata_lookup_type_for_media_item, normalize_match_query, normalize_match_year};
    use mova_domain::MediaItem;
    use time::OffsetDateTime;

    fn sample_media_item(media_type: &str) -> MediaItem {
        MediaItem {
            id: 1,
            library_id: 1,
            media_type: media_type.to_string(),
            title: "Sample".to_string(),
            original_title: None,
            sort_title: None,
            metadata_provider: None,
            metadata_provider_item_id: None,
            year: Some(2025),
            overview: None,
            poster_path: None,
            backdrop_path: None,
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn normalize_match_query_rejects_empty_values() {
        let error = normalize_match_query("   ".to_string()).expect_err("empty query should fail");
        assert!(error.to_string().contains("cannot be empty"));
    }

    #[test]
    fn normalize_match_year_rejects_non_positive_values() {
        let error = normalize_match_year(Some(0)).expect_err("year 0 should fail");
        assert!(error.to_string().contains("positive integer"));
    }

    #[test]
    fn metadata_lookup_type_for_media_item_only_allows_movie_and_series() {
        let movie = sample_media_item("movie");
        let series = sample_media_item("series");
        let episode = sample_media_item("episode");

        assert_eq!(
            metadata_lookup_type_for_media_item(&movie).expect("movie should be supported"),
            "movie"
        );
        assert_eq!(
            metadata_lookup_type_for_media_item(&series).expect("series should be supported"),
            "series"
        );
        assert!(metadata_lookup_type_for_media_item(&episode)
            .expect_err("episode should be rejected")
            .to_string()
            .contains("not supported"));
    }
}
