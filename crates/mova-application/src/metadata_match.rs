use crate::{
    ensure_media_item_cast,
    error::{ApplicationError, ApplicationResult},
    invalidate_media_item_cast_cache,
    libraries::get_library,
    media_classification::metadata_lookup_type_for_media_type,
    media_enrichment::MetadataEnrichmentContext,
    media_items::get_media_item,
    metadata::{
        MetadataLookup, MetadataProvider, RemoteMetadataSearchResult, RemoteSeriesEpisodeOutline,
        TMDB_PROVIDER_NAME,
    },
};
use mova_domain::{
    MediaItem, METADATA_STATUS_MATCHED, REMOTE_MEDIA_TYPE_MOVIE, REMOTE_MEDIA_TYPE_SERIES,
};
use sqlx::postgres::PgPool;
use std::{path::PathBuf, sync::Arc};

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
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> ApplicationResult<MediaItem> {
    ensure_metadata_provider_enabled(metadata_provider.as_ref())?;
    let media_item = get_media_item(pool, media_item_id).await?;
    let lookup_type = metadata_lookup_type_for_media_item(&media_item)?;
    let library = get_library(pool, media_item.library_id).await?;
    let lookup = MetadataLookup {
        title: media_item.source_title.clone(),
        year: media_item.year,
        library_type: lookup_type.to_string(),
        language: Some(library.metadata_language.clone()),
        provider_item_id: Some(input.provider_item_id),
    };

    let remote_metadata = metadata_provider.lookup(&lookup).await.map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "failed to fetch selected metadata candidate for media item {}: {}",
            media_item_id,
            error
        ))
    })?;
    let mut remote_metadata = remote_metadata.ok_or_else(|| {
        ApplicationError::NotFound(format!(
            "metadata candidate {} was not found",
            input.provider_item_id
        ))
    })?;
    let mut enrichment = MetadataEnrichmentContext::new(
        artwork_cache_dir,
        metadata_provider.clone(),
        library.metadata_language.clone(),
    );
    enrichment
        .cache_remote_metadata_artwork(&mut remote_metadata)
        .await;

    let updated_media_item = mova_db::update_media_item_metadata(
        pool,
        media_item_id,
        mova_db::UpdateMediaItemMetadataParams {
            title: remote_metadata.title.unwrap_or(media_item.title),
            source_title: media_item.source_title,
            original_title: remote_metadata.original_title.or(media_item.original_title),
            // 手动匹配目前不会单独生成 sort title，因此保留现有值避免意外清空。
            sort_title: media_item.sort_title,
            metadata_provider: Some(TMDB_PROVIDER_NAME.to_string()),
            metadata_provider_item_id: Some(input.provider_item_id),
            metadata_status: METADATA_STATUS_MATCHED.to_string(),
            metadata_failure_reason: None,
            remote_media_type: remote_media_type_for_media_type(&media_item.media_type)
                .map(str::to_string),
            year: remote_metadata.year.or(media_item.year),
            imdb_rating: remote_metadata.imdb_rating.or(media_item.imdb_rating),
            country: remote_metadata.country.or(media_item.country),
            genres: remote_metadata.genres.or(media_item.genres),
            studio: remote_metadata.studio.or(media_item.studio),
            overview: remote_metadata.overview.or(media_item.overview),
            poster_path: remote_metadata.poster_path,
            backdrop_path: remote_metadata.backdrop_path,
        },
    )
    .await
    .map_err(ApplicationError::from)?
    .ok_or_else(|| {
        ApplicationError::NotFound(format!("media item not found: {}", media_item_id))
    })?;

    if media_item.media_type.eq_ignore_ascii_case("series") {
        apply_selected_series_episode_metadata(
            pool,
            media_item.id,
            &lookup,
            metadata_provider.as_ref(),
            &mut enrichment,
        )
        .await?;
    }
    invalidate_media_item_cast_cache(pool, media_item.id).await?;
    ensure_media_item_cast(pool, &updated_media_item, metadata_provider).await?;

    Ok(updated_media_item)
}

fn remote_media_type_for_media_type(media_type: &str) -> Option<&'static str> {
    if media_type.eq_ignore_ascii_case("series") || media_type.eq_ignore_ascii_case("episode") {
        return Some(REMOTE_MEDIA_TYPE_SERIES);
    }

    if media_type.eq_ignore_ascii_case("movie") {
        return Some(REMOTE_MEDIA_TYPE_MOVIE);
    }

    None
}

async fn apply_selected_series_episode_metadata(
    pool: &PgPool,
    series_media_item_id: i64,
    lookup: &MetadataLookup,
    metadata_provider: &dyn MetadataProvider,
    enrichment: &mut MetadataEnrichmentContext,
) -> ApplicationResult<()> {
    let mut remote_outline = match metadata_provider
        .lookup_series_episode_outline(lookup)
        .await
    {
        Ok(Some(remote_outline)) => remote_outline,
        Ok(None) => {
            crate::media_items::cache_remote_outline(
                pool,
                series_media_item_id,
                &RemoteSeriesEpisodeOutline { seasons: vec![] },
            )
            .await?;
            return Ok(());
        }
        Err(error) => {
            tracing::warn!(
                media_item_id = series_media_item_id,
                provider_item_id = lookup.provider_item_id,
                error = ?error,
                "failed to fetch selected series episode outline after metadata replacement"
            );
            mova_db::delete_series_episode_outline_cache(pool, series_media_item_id)
                .await
                .map_err(ApplicationError::from)?;
            return Ok(());
        }
    };

    enrichment
        .cache_remote_series_outline_artwork(&mut remote_outline)
        .await;
    persist_selected_series_episode_metadata(pool, series_media_item_id, &remote_outline).await?;
    crate::media_items::cache_remote_outline(pool, series_media_item_id, &remote_outline).await
}

async fn persist_selected_series_episode_metadata(
    pool: &PgPool,
    series_media_item_id: i64,
    remote_outline: &RemoteSeriesEpisodeOutline,
) -> ApplicationResult<()> {
    // Keep these writes strictly serial: one successful season or episode update must finish
    // before the next one starts, so a failed artwork write never races with later updates.
    for season in &remote_outline.seasons {
        mova_db::update_series_season_metadata(
            pool,
            mova_db::UpdateSeriesSeasonMetadataParams {
                series_id: series_media_item_id,
                season_number: season.season_number,
                title: season.title.clone(),
                overview: season.overview.clone(),
                poster_path: season.poster_path.clone(),
                backdrop_path: season.backdrop_path.clone(),
            },
        )
        .await
        .map_err(ApplicationError::from)?;

        for episode in &season.episodes {
            mova_db::update_series_episode_metadata(
                pool,
                mova_db::UpdateSeriesEpisodeMetadataParams {
                    series_id: series_media_item_id,
                    season_number: season.season_number,
                    episode_number: episode.episode_number,
                    title: episode.title.clone(),
                    overview: episode.overview.clone(),
                    poster_path: episode.poster_path.clone(),
                    backdrop_path: episode.backdrop_path.clone(),
                },
            )
            .await
            .map_err(ApplicationError::from)?;
        }
    }

    Ok(())
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
    use mova_domain::{MediaItem, METADATA_STATUS_SKIPPED};
    use time::OffsetDateTime;

    fn sample_media_item(media_type: &str) -> MediaItem {
        MediaItem {
            id: 1,
            library_id: 1,
            media_type: media_type.to_string(),
            title: "Sample".to_string(),
            source_title: "Sample".to_string(),
            original_title: None,
            sort_title: None,
            metadata_provider: None,
            metadata_provider_item_id: None,
            metadata_status: METADATA_STATUS_SKIPPED.to_string(),
            metadata_failure_reason: None,
            remote_media_type: None,
            year: Some(2025),
            imdb_rating: None,
            country: None,
            genres: None,
            studio: None,
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
