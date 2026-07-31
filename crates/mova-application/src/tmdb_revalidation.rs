use crate::{
    media_cast::normalize_remote_cast,
    media_classification::metadata_lookup_type_for_media_type,
    media_enrichment::MetadataEnrichmentContext,
    metadata::{
        MetadataLookup, MetadataProvider, RemoteMetadata, RemoteSeriesEpisode,
        RemoteSeriesEpisodeOutline, RemoteSeriesSeason, TMDB_PROVIDER_NAME,
    },
};
use mova_db::{
    BackgroundJobFence, CompleteTmdbMetadataRevalidationParams, ExpireTmdbMetadataRetentionParams,
    ReplaceMediaItemCastMember, ReplaceTmdbRevalidationEpisode, ReplaceTmdbRevalidationSeason,
    TmdbMetadataRevalidationTarget, TmdbRevalidationEpisode, TmdbRevalidationSeason,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

const REMOTE_SNAPSHOT_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TmdbMetadataRevalidationOutcome {
    Revalidated,
    NoLongerEligible,
    DeferredUntilRetention,
    RetentionExpired,
    Superseded,
}

#[derive(Debug, Clone)]
pub struct TmdbMetadataRevalidationInput {
    pub media_item_id: i64,
    pub expected_library_id: i64,
    pub expected_provider_item_id: String,
    pub scheduled_retention_expired: bool,
    pub artwork_cache_dir: PathBuf,
}

/// This contains only values returned by TMDB. A pre-existing value that
/// differs from the first direct-id response remains a local override. An
/// exact response echo, or artwork already inside the library TMDB namespace,
/// becomes provider-owned once this snapshot commits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct TmdbRemoteMetadataSnapshot {
    #[serde(default)]
    version: u8,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    original_title: Option<String>,
    #[serde(default)]
    year: Option<i32>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    genres: Option<String>,
    #[serde(default)]
    studio: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    poster_path: Option<String>,
    #[serde(default)]
    backdrop_path: Option<String>,
    #[serde(default)]
    logo_path: Option<String>,
    #[serde(default)]
    series_outline: Option<RemoteSeriesEpisodeOutline>,
}

#[derive(Debug)]
struct RevalidatedDisplayMetadata {
    title: String,
    original_title: Option<String>,
    year: Option<i32>,
    country: Option<String>,
    genres: Option<String>,
    studio: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    logo_path: Option<String>,
}

pub async fn execute_tmdb_metadata_revalidation(
    pool: &PgPool,
    input: TmdbMetadataRevalidationInput,
    fence: &BackgroundJobFence,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> anyhow::Result<TmdbMetadataRevalidationOutcome> {
    let Some(target) = mova_db::get_tmdb_metadata_revalidation_target(
        pool,
        input.media_item_id,
        &input.expected_provider_item_id,
    )
    .await?
    else {
        mova_db::discard_ineligible_tmdb_metadata_revalidation(
            pool,
            fence,
            input.media_item_id,
            &input.expected_provider_item_id,
        )
        .await?;
        return Ok(TmdbMetadataRevalidationOutcome::NoLongerEligible);
    };
    if target.library_id != input.expected_library_id {
        anyhow::bail!(
            "TMDB revalidation library {} did not match media item {} library {}",
            input.expected_library_id,
            input.media_item_id,
            target.library_id
        );
    }

    let previous_snapshot =
        serde_json::from_str::<TmdbRemoteMetadataSnapshot>(&target.remote_snapshot_json)
            .unwrap_or_default();
    let retention_expired = target.database_now >= target.retain_until;
    if retention_expired != input.scheduled_retention_expired {
        tracing::debug!(
            media_item_id = input.media_item_id,
            scheduled_retention_expired = input.scheduled_retention_expired,
            retention_expired,
            "TMDB revalidation queue hint differed from current database retention state"
        );
    }
    if retention_expired {
        let tmdb_artwork_root =
            crate::library_artwork_cache_dir(&input.artwork_cache_dir, target.library_id)
                .join("tmdb");
        let expired =
            expire_revalidated_display_metadata(&target, &previous_snapshot, &tmdb_artwork_root);
        let seasons =
            expire_revalidated_seasons(&target.seasons, &previous_snapshot, &tmdb_artwork_root);
        let episodes =
            expire_revalidated_episodes(&target.episodes, &previous_snapshot, &tmdb_artwork_root);
        let artwork_paths =
            expiring_provider_artwork_paths(&target, &previous_snapshot, &tmdb_artwork_root);
        let expired = mova_db::expire_tmdb_metadata_retention(
            pool,
            fence,
            ExpireTmdbMetadataRetentionParams {
                media_item_id: target.media_item_id,
                library_id: target.library_id,
                provider_item_id: target.provider_item_id,
                observed_media_item_updated_at: target.observed_media_item_updated_at,
                observed_revalidation_updated_at: target.observed_revalidation_updated_at,
                title: expired.title,
                original_title: expired.original_title,
                year: expired.year,
                country: expired.country,
                genres: expired.genres,
                studio: expired.studio,
                overview: expired.overview,
                poster_path: expired.poster_path,
                backdrop_path: expired.backdrop_path,
                logo_path: expired.logo_path,
                seasons,
                episodes,
                artwork_cleanup_paths: artwork_paths.into_iter().collect(),
            },
        )
        .await?;
        return Ok(if expired {
            TmdbMetadataRevalidationOutcome::RetentionExpired
        } else {
            TmdbMetadataRevalidationOutcome::Superseded
        });
    }

    if target.database_now < target.next_attempt_at {
        return Ok(TmdbMetadataRevalidationOutcome::Superseded);
    }

    if !metadata_provider.is_enabled() {
        let deferred = mova_db::defer_tmdb_revalidation_until_retention_deadline(
            pool,
            fence,
            target.media_item_id,
            &target.provider_item_id,
            target.observed_media_item_updated_at,
            target.observed_revalidation_updated_at,
        )
        .await?;
        return Ok(if deferred {
            TmdbMetadataRevalidationOutcome::DeferredUntilRetention
        } else {
            TmdbMetadataRevalidationOutcome::Superseded
        });
    }

    let lookup = revalidation_lookup(&target);
    let mut remote_metadata = metadata_provider.lookup(&lookup).await?.ok_or_else(|| {
        anyhow::anyhow!(
            "TMDB item {} was not found during metadata revalidation",
            input.expected_provider_item_id
        )
    })?;
    validate_tmdb_remote_identity(&remote_metadata, &input.expected_provider_item_id)?;

    let mut enrichment = MetadataEnrichmentContext::new(
        input.artwork_cache_dir.clone(),
        target.library_id,
        metadata_provider.clone(),
        target.metadata_language.clone(),
    );
    let artwork_publication =
        mova_db::TmdbArtworkPublicationGuard::acquire(pool, target.library_id).await?;
    enrichment
        .cache_remote_metadata_artwork(&mut remote_metadata)
        .await;
    let mut materialized_artwork_paths = remote_artwork_paths(&remote_metadata);
    let mut remote_series_outline = if target.media_type == "series" {
        match metadata_provider
            .lookup_complete_series_episode_outline(&lookup)
            .await
        {
            Ok(Some(outline)) => Some(outline),
            Ok(None) => {
                release_and_remove_unreferenced_tmdb_artwork_best_effort(
                    artwork_publication,
                    pool,
                    &input.artwork_cache_dir,
                    target.library_id,
                    materialized_artwork_paths,
                )
                .await;
                return Err(anyhow::anyhow!(
                    "TMDB series {} omitted its complete season and episode outline",
                    input.expected_provider_item_id
                ));
            }
            Err(error) => {
                release_and_remove_unreferenced_tmdb_artwork_best_effort(
                    artwork_publication,
                    pool,
                    &input.artwork_cache_dir,
                    target.library_id,
                    materialized_artwork_paths,
                )
                .await;
                return Err(error);
            }
        }
    } else {
        None
    };
    if let Some(outline) = remote_series_outline.as_mut() {
        enrichment
            .cache_remote_series_outline_artwork(outline)
            .await;
    }

    if let Some(outline) = remote_series_outline.as_ref() {
        materialized_artwork_paths.extend(remote_outline_artwork_paths(outline));
    }
    if let Err(error) = ensure_remote_artwork_was_materialized(&remote_metadata) {
        release_and_remove_unreferenced_tmdb_artwork_best_effort(
            artwork_publication,
            pool,
            &input.artwork_cache_dir,
            target.library_id,
            materialized_artwork_paths,
        )
        .await;
        return Err(error);
    }
    if let Some(outline) = remote_series_outline.as_ref() {
        if let Err(error) = ensure_remote_outline_artwork_was_materialized(outline) {
            release_and_remove_unreferenced_tmdb_artwork_best_effort(
                artwork_publication,
                pool,
                &input.artwork_cache_dir,
                target.library_id,
                materialized_artwork_paths,
            )
            .await;
            return Err(error);
        }
    }

    let cast_members = if target.has_cast_cache {
        let remote_cast = match metadata_provider.lookup_cast(&lookup).await {
            Ok(remote_cast) => remote_cast.unwrap_or_default(),
            Err(error) => {
                release_and_remove_unreferenced_tmdb_artwork_best_effort(
                    artwork_publication,
                    pool,
                    &input.artwork_cache_dir,
                    target.library_id,
                    materialized_artwork_paths,
                )
                .await;
                return Err(error);
            }
        };
        Some(
            normalize_remote_cast(target.media_item_id, remote_cast)
                .into_iter()
                .map(|member| ReplaceMediaItemCastMember {
                    person_id: member.person_id,
                    sort_order: member.sort_order,
                    name: member.name,
                    character_name: member.character_name,
                    profile_path: member.profile_path,
                })
                .collect(),
        )
    } else {
        None
    };

    let tmdb_artwork_root =
        crate::library_artwork_cache_dir(&input.artwork_cache_dir, target.library_id).join("tmdb");
    let display_metadata = merge_revalidated_display_metadata(
        &target,
        &previous_snapshot,
        &remote_metadata,
        &tmdb_artwork_root,
    );
    let seasons = merge_revalidated_seasons(
        &target.seasons,
        &previous_snapshot,
        remote_series_outline.as_ref(),
        &tmdb_artwork_root,
    );
    let episodes = merge_revalidated_episodes(
        &target.episodes,
        &previous_snapshot,
        remote_series_outline.as_ref(),
        &tmdb_artwork_root,
    );
    let artwork_cleanup_paths =
        expiring_provider_artwork_paths(&target, &previous_snapshot, &tmdb_artwork_root);
    let remote_snapshot =
        TmdbRemoteMetadataSnapshot::from_remote(&remote_metadata, remote_series_outline.clone());
    let remote_snapshot_json = match serde_json::to_string(&remote_snapshot) {
        Ok(remote_snapshot_json) => remote_snapshot_json,
        Err(error) => {
            release_and_remove_unreferenced_tmdb_artwork_best_effort(
                artwork_publication,
                pool,
                &input.artwork_cache_dir,
                target.library_id,
                materialized_artwork_paths,
            )
            .await;
            return Err(error.into());
        }
    };
    let series_outline_json = match remote_series_outline.as_ref().map(serde_json::to_string) {
        Some(Ok(series_outline_json)) => Some(series_outline_json),
        Some(Err(error)) => {
            release_and_remove_unreferenced_tmdb_artwork_best_effort(
                artwork_publication,
                pool,
                &input.artwork_cache_dir,
                target.library_id,
                materialized_artwork_paths,
            )
            .await;
            return Err(error.into());
        }
        None => None,
    };

    let completed = mova_db::complete_tmdb_metadata_revalidation(
        pool,
        fence,
        CompleteTmdbMetadataRevalidationParams {
            media_item_id: target.media_item_id,
            library_id: target.library_id,
            provider_item_id: target.provider_item_id,
            observed_media_item_updated_at: target.observed_media_item_updated_at,
            observed_revalidation_updated_at: target.observed_revalidation_updated_at,
            title: display_metadata.title,
            original_title: display_metadata.original_title,
            year: display_metadata.year,
            country: display_metadata.country,
            genres: display_metadata.genres,
            studio: display_metadata.studio,
            overview: display_metadata.overview,
            poster_path: display_metadata.poster_path,
            backdrop_path: display_metadata.backdrop_path,
            logo_path: display_metadata.logo_path,
            external_ids: remote_metadata.external_ids,
            ratings: remote_metadata.ratings,
            cast_members,
            seasons,
            episodes,
            series_outline_json,
            artwork_cleanup_paths: artwork_cleanup_paths.into_iter().collect(),
            remote_snapshot_json,
        },
    )
    .await;
    artwork_publication.release().await?;
    match completed {
        Ok(true) => Ok(TmdbMetadataRevalidationOutcome::Revalidated),
        Ok(false) => {
            remove_unreferenced_tmdb_artwork_best_effort(
                pool,
                &input.artwork_cache_dir,
                target.library_id,
                materialized_artwork_paths,
            )
            .await;
            Ok(TmdbMetadataRevalidationOutcome::Superseded)
        }
        Err(error) => {
            remove_unreferenced_tmdb_artwork_best_effort(
                pool,
                &input.artwork_cache_dir,
                target.library_id,
                materialized_artwork_paths,
            )
            .await;
            Err(error)
        }
    }
}

fn revalidation_lookup(target: &TmdbMetadataRevalidationTarget) -> MetadataLookup {
    MetadataLookup {
        title: target.source_title.clone(),
        year: target.year,
        season_air_year: None,
        library_type: metadata_lookup_type_for_media_type(&target.media_type).to_string(),
        language: Some(target.metadata_language.clone()),
        // Never search or rematch during compliance revalidation.
        provider_item_id: Some(target.provider_item_id.clone()),
    }
}

pub(crate) fn validate_tmdb_remote_identity(
    metadata: &RemoteMetadata,
    expected_provider_item_id: &str,
) -> anyhow::Result<()> {
    let returned_provider_item_id = metadata
        .provider_item_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("TMDB revalidation response omitted its provider id"))?;
    if returned_provider_item_id != expected_provider_item_id {
        anyhow::bail!(
            "TMDB revalidation returned provider id {} for expected id {}",
            returned_provider_item_id,
            expected_provider_item_id
        );
    }
    if !metadata.external_ids.iter().any(|external_id| {
        external_id.provider == TMDB_PROVIDER_NAME
            && external_id.external_id == expected_provider_item_id
    }) {
        anyhow::bail!(
            "TMDB revalidation response omitted the accepted provider identity {}",
            expected_provider_item_id
        );
    }
    Ok(())
}

fn ensure_remote_artwork_was_materialized(metadata: &RemoteMetadata) -> anyhow::Result<()> {
    for (kind, path) in [
        ("poster", metadata.poster_path.as_deref()),
        ("backdrop", metadata.backdrop_path.as_deref()),
        ("logo", metadata.logo_path.as_deref()),
    ] {
        let Some(path) = path else {
            continue;
        };
        if reqwest::Url::parse(path)
            .ok()
            .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
        {
            anyhow::bail!("TMDB {kind} could not be materialized in the local artwork cache");
        }
    }
    Ok(())
}

fn ensure_remote_outline_artwork_was_materialized(
    outline: &RemoteSeriesEpisodeOutline,
) -> anyhow::Result<()> {
    for path in remote_outline_artwork_paths(outline) {
        if reqwest::Url::parse(&path)
            .ok()
            .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
        {
            anyhow::bail!(
                "TMDB series outline artwork could not be materialized in the local artwork cache"
            );
        }
    }
    Ok(())
}

fn merge_revalidated_display_metadata(
    target: &TmdbMetadataRevalidationTarget,
    previous: &TmdbRemoteMetadataSnapshot,
    incoming: &RemoteMetadata,
    tmdb_artwork_root: &std::path::Path,
) -> RevalidatedDisplayMetadata {
    let has_previous_snapshot = previous.version == REMOTE_SNAPSHOT_VERSION;

    RevalidatedDisplayMetadata {
        title: merge_title(
            target.title.clone(),
            target.source_title.clone(),
            previous.title.as_ref(),
            incoming.title.clone(),
            has_previous_snapshot,
        ),
        original_title: merge_optional_provider_value(
            target.original_title.clone(),
            previous.original_title.as_ref(),
            incoming.original_title.clone(),
            has_previous_snapshot,
        ),
        year: merge_optional_provider_value(
            target.year,
            previous.year.as_ref(),
            incoming.year,
            has_previous_snapshot,
        ),
        country: merge_optional_provider_value(
            target.country.clone(),
            previous.country.as_ref(),
            incoming.country.clone(),
            has_previous_snapshot,
        ),
        genres: merge_optional_provider_value(
            target.genres.clone(),
            previous.genres.as_ref(),
            incoming.genres.clone(),
            has_previous_snapshot,
        ),
        studio: merge_optional_provider_value(
            target.studio.clone(),
            previous.studio.as_ref(),
            incoming.studio.clone(),
            has_previous_snapshot,
        ),
        overview: merge_optional_provider_value(
            target.overview.clone(),
            previous.overview.as_ref(),
            incoming.overview.clone(),
            has_previous_snapshot,
        ),
        poster_path: merge_optional_provider_artwork(
            target.poster_path.clone(),
            previous.poster_path.as_ref(),
            incoming.poster_path.clone(),
            has_previous_snapshot,
            tmdb_artwork_root,
        ),
        backdrop_path: merge_optional_provider_artwork(
            target.backdrop_path.clone(),
            previous.backdrop_path.as_ref(),
            incoming.backdrop_path.clone(),
            has_previous_snapshot,
            tmdb_artwork_root,
        ),
        logo_path: merge_optional_provider_artwork(
            target.logo_path.clone(),
            previous.logo_path.as_ref(),
            incoming.logo_path.clone(),
            has_previous_snapshot,
            tmdb_artwork_root,
        ),
    }
}

fn expire_revalidated_display_metadata(
    target: &TmdbMetadataRevalidationTarget,
    previous: &TmdbRemoteMetadataSnapshot,
    tmdb_artwork_root: &std::path::Path,
) -> RevalidatedDisplayMetadata {
    let has_previous_snapshot = previous.version == REMOTE_SNAPSHOT_VERSION;

    RevalidatedDisplayMetadata {
        title: expire_title(
            target.title.clone(),
            target.source_title.clone(),
            previous.title.as_ref(),
            has_previous_snapshot,
        ),
        original_title: expire_optional_provider_value(
            target.original_title.clone(),
            previous.original_title.as_ref(),
            has_previous_snapshot,
        ),
        year: expire_optional_provider_value(
            target.year,
            previous.year.as_ref(),
            has_previous_snapshot,
        ),
        country: expire_optional_provider_value(
            target.country.clone(),
            previous.country.as_ref(),
            has_previous_snapshot,
        ),
        genres: expire_optional_provider_value(
            target.genres.clone(),
            previous.genres.as_ref(),
            has_previous_snapshot,
        ),
        studio: expire_optional_provider_value(
            target.studio.clone(),
            previous.studio.as_ref(),
            has_previous_snapshot,
        ),
        overview: expire_optional_provider_value(
            target.overview.clone(),
            previous.overview.as_ref(),
            has_previous_snapshot,
        ),
        poster_path: expire_optional_provider_artwork(
            target.poster_path.clone(),
            previous.poster_path.as_ref(),
            has_previous_snapshot,
            tmdb_artwork_root,
        ),
        backdrop_path: expire_optional_provider_artwork(
            target.backdrop_path.clone(),
            previous.backdrop_path.as_ref(),
            has_previous_snapshot,
            tmdb_artwork_root,
        ),
        logo_path: expire_optional_provider_artwork(
            target.logo_path.clone(),
            previous.logo_path.as_ref(),
            has_previous_snapshot,
            tmdb_artwork_root,
        ),
    }
}

fn merge_title(
    current: String,
    fallback: String,
    previous_remote: Option<&String>,
    incoming_remote: Option<String>,
    has_previous_snapshot: bool,
) -> String {
    if current.trim().is_empty()
        || (has_previous_snapshot && previous_remote.is_some_and(|previous| previous == &current))
    {
        return incoming_remote
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(fallback);
    }

    current
}

fn expire_title(
    current: String,
    fallback: String,
    previous_remote: Option<&String>,
    has_previous_snapshot: bool,
) -> String {
    if !has_previous_snapshot || previous_remote.is_some_and(|previous| previous == &current) {
        return fallback;
    }
    current
}

fn expire_optional_provider_value<T>(
    current: Option<T>,
    previous_remote: Option<&T>,
    has_previous_snapshot: bool,
) -> Option<T>
where
    T: PartialEq,
{
    if !has_previous_snapshot
        || previous_remote.is_some_and(|previous| current.as_ref() == Some(previous))
    {
        return None;
    }

    current
}

fn merge_optional_provider_value<T>(
    current: Option<T>,
    previous_remote: Option<&T>,
    incoming_remote: Option<T>,
    has_previous_snapshot: bool,
) -> Option<T>
where
    T: Clone + PartialEq,
{
    if current.is_none()
        || (has_previous_snapshot
            && previous_remote.is_some_and(|previous| current.as_ref() == Some(previous)))
    {
        return incoming_remote;
    }

    current
}

fn merge_optional_provider_artwork(
    current: Option<String>,
    previous_remote: Option<&String>,
    incoming_remote: Option<String>,
    has_previous_snapshot: bool,
    tmdb_artwork_root: &std::path::Path,
) -> Option<String> {
    if current
        .as_deref()
        .is_some_and(|path| is_tmdb_cached_artwork_path(path, tmdb_artwork_root))
    {
        return incoming_remote;
    }

    merge_optional_provider_value(
        current,
        previous_remote,
        incoming_remote,
        has_previous_snapshot,
    )
}

fn expire_optional_provider_artwork(
    current: Option<String>,
    previous_remote: Option<&String>,
    has_previous_snapshot: bool,
    tmdb_artwork_root: &std::path::Path,
) -> Option<String> {
    if current
        .as_deref()
        .is_some_and(|path| is_tmdb_cached_artwork_path(path, tmdb_artwork_root))
    {
        return None;
    }

    expire_optional_provider_value(current, previous_remote, has_previous_snapshot)
}

fn is_tmdb_cached_artwork_path(path: &str, tmdb_artwork_root: &std::path::Path) -> bool {
    PathBuf::from(path).starts_with(tmdb_artwork_root)
}

fn merge_revalidated_seasons(
    current_seasons: &[TmdbRevalidationSeason],
    previous: &TmdbRemoteMetadataSnapshot,
    incoming: Option<&RemoteSeriesEpisodeOutline>,
    tmdb_artwork_root: &std::path::Path,
) -> Vec<ReplaceTmdbRevalidationSeason> {
    let has_previous_snapshot = previous.version == REMOTE_SNAPSHOT_VERSION;
    current_seasons
        .iter()
        .map(|season| {
            let previous_remote = snapshot_season(previous, season.season_number);
            let incoming_remote = incoming.and_then(|outline| {
                outline
                    .seasons
                    .iter()
                    .find(|remote| remote.season_number == season.season_number)
            });
            ReplaceTmdbRevalidationSeason {
                season_id: season.season_id,
                observed_updated_at: season.observed_updated_at,
                title: merge_season_title(
                    season.title.clone(),
                    season.season_number,
                    previous_remote.and_then(|remote| remote.title.as_ref()),
                    incoming_remote.and_then(|remote| remote.title.clone()),
                    has_previous_snapshot,
                ),
                overview: merge_optional_provider_value(
                    season.overview.clone(),
                    previous_remote.and_then(|remote| remote.overview.as_ref()),
                    incoming_remote.and_then(|remote| remote.overview.clone()),
                    has_previous_snapshot,
                ),
                poster_path: merge_optional_provider_artwork(
                    season.poster_path.clone(),
                    previous_remote.and_then(|remote| remote.poster_path.as_ref()),
                    incoming_remote.and_then(|remote| remote.poster_path.clone()),
                    has_previous_snapshot,
                    tmdb_artwork_root,
                ),
                backdrop_path: merge_optional_provider_artwork(
                    season.backdrop_path.clone(),
                    previous_remote.and_then(|remote| remote.backdrop_path.as_ref()),
                    incoming_remote.and_then(|remote| remote.backdrop_path.clone()),
                    has_previous_snapshot,
                    tmdb_artwork_root,
                ),
            }
        })
        .collect()
}

fn merge_revalidated_episodes(
    current_episodes: &[TmdbRevalidationEpisode],
    previous: &TmdbRemoteMetadataSnapshot,
    incoming: Option<&RemoteSeriesEpisodeOutline>,
    tmdb_artwork_root: &std::path::Path,
) -> Vec<ReplaceTmdbRevalidationEpisode> {
    let has_previous_snapshot = previous.version == REMOTE_SNAPSHOT_VERSION;
    current_episodes
        .iter()
        .map(|episode| {
            let previous_remote =
                snapshot_episode(previous, episode.season_number, episode.episode_number);
            let incoming_remote =
                outline_episode(incoming, episode.season_number, episode.episode_number);
            ReplaceTmdbRevalidationEpisode {
                media_item_id: episode.media_item_id,
                observed_updated_at: episode.observed_updated_at,
                title: merge_title(
                    episode.title.clone(),
                    episode.source_title.clone(),
                    previous_remote.and_then(|remote| remote.title.as_ref()),
                    incoming_remote.and_then(|remote| remote.title.clone()),
                    has_previous_snapshot,
                ),
                overview: merge_optional_provider_value(
                    episode.overview.clone(),
                    previous_remote.and_then(|remote| remote.overview.as_ref()),
                    incoming_remote.and_then(|remote| remote.overview.clone()),
                    has_previous_snapshot,
                ),
                poster_path: merge_optional_provider_artwork(
                    episode.poster_path.clone(),
                    previous_remote.and_then(|remote| remote.poster_path.as_ref()),
                    incoming_remote.and_then(|remote| remote.poster_path.clone()),
                    has_previous_snapshot,
                    tmdb_artwork_root,
                ),
                backdrop_path: merge_optional_provider_artwork(
                    episode.backdrop_path.clone(),
                    previous_remote.and_then(|remote| remote.backdrop_path.as_ref()),
                    incoming_remote.and_then(|remote| remote.backdrop_path.clone()),
                    has_previous_snapshot,
                    tmdb_artwork_root,
                ),
            }
        })
        .collect()
}

fn expire_revalidated_seasons(
    current_seasons: &[TmdbRevalidationSeason],
    previous: &TmdbRemoteMetadataSnapshot,
    tmdb_artwork_root: &std::path::Path,
) -> Vec<ReplaceTmdbRevalidationSeason> {
    let has_previous_snapshot = previous.version == REMOTE_SNAPSHOT_VERSION;
    current_seasons
        .iter()
        .map(|season| {
            let previous_remote = snapshot_season(previous, season.season_number);
            ReplaceTmdbRevalidationSeason {
                season_id: season.season_id,
                observed_updated_at: season.observed_updated_at,
                title: expire_season_title(
                    season.title.clone(),
                    season.season_number,
                    previous_remote.and_then(|remote| remote.title.as_ref()),
                    has_previous_snapshot,
                ),
                overview: expire_optional_provider_value(
                    season.overview.clone(),
                    previous_remote.and_then(|remote| remote.overview.as_ref()),
                    has_previous_snapshot,
                ),
                poster_path: expire_optional_provider_artwork(
                    season.poster_path.clone(),
                    previous_remote.and_then(|remote| remote.poster_path.as_ref()),
                    has_previous_snapshot,
                    tmdb_artwork_root,
                ),
                backdrop_path: expire_optional_provider_artwork(
                    season.backdrop_path.clone(),
                    previous_remote.and_then(|remote| remote.backdrop_path.as_ref()),
                    has_previous_snapshot,
                    tmdb_artwork_root,
                ),
            }
        })
        .collect()
}

fn expire_revalidated_episodes(
    current_episodes: &[TmdbRevalidationEpisode],
    previous: &TmdbRemoteMetadataSnapshot,
    tmdb_artwork_root: &std::path::Path,
) -> Vec<ReplaceTmdbRevalidationEpisode> {
    let has_previous_snapshot = previous.version == REMOTE_SNAPSHOT_VERSION;
    current_episodes
        .iter()
        .map(|episode| {
            let previous_remote =
                snapshot_episode(previous, episode.season_number, episode.episode_number);
            ReplaceTmdbRevalidationEpisode {
                media_item_id: episode.media_item_id,
                observed_updated_at: episode.observed_updated_at,
                title: if !has_previous_snapshot
                    || previous_remote
                        .and_then(|remote| remote.title.as_ref())
                        .is_some_and(|remote| remote == &episode.title)
                {
                    episode.source_title.clone()
                } else {
                    episode.title.clone()
                },
                overview: expire_optional_provider_value(
                    episode.overview.clone(),
                    previous_remote.and_then(|remote| remote.overview.as_ref()),
                    has_previous_snapshot,
                ),
                poster_path: expire_optional_provider_artwork(
                    episode.poster_path.clone(),
                    previous_remote.and_then(|remote| remote.poster_path.as_ref()),
                    has_previous_snapshot,
                    tmdb_artwork_root,
                ),
                backdrop_path: expire_optional_provider_artwork(
                    episode.backdrop_path.clone(),
                    previous_remote.and_then(|remote| remote.backdrop_path.as_ref()),
                    has_previous_snapshot,
                    tmdb_artwork_root,
                ),
            }
        })
        .collect()
}

fn merge_season_title(
    current: Option<String>,
    season_number: i32,
    previous_remote: Option<&String>,
    incoming_remote: Option<String>,
    has_previous_snapshot: bool,
) -> Option<String> {
    if current.is_none()
        || (has_previous_snapshot
            && previous_remote.is_some_and(|previous| current.as_ref() == Some(previous)))
    {
        return incoming_remote
            .filter(|title| !title.trim().is_empty())
            .or_else(|| Some(format!("Season {season_number:02}")));
    }
    current
}

fn expire_season_title(
    current: Option<String>,
    season_number: i32,
    previous_remote: Option<&String>,
    has_previous_snapshot: bool,
) -> Option<String> {
    if !has_previous_snapshot
        || previous_remote.is_some_and(|previous| current.as_ref() == Some(previous))
    {
        return Some(format!("Season {season_number:02}"));
    }
    current
}

fn snapshot_season(
    snapshot: &TmdbRemoteMetadataSnapshot,
    season_number: i32,
) -> Option<&RemoteSeriesSeason> {
    snapshot
        .series_outline
        .as_ref()?
        .seasons
        .iter()
        .find(|season| season.season_number == season_number)
}

fn snapshot_episode(
    snapshot: &TmdbRemoteMetadataSnapshot,
    season_number: i32,
    episode_number: i32,
) -> Option<&RemoteSeriesEpisode> {
    snapshot_season(snapshot, season_number)?
        .episodes
        .iter()
        .find(|episode| episode.episode_number == episode_number)
}

fn outline_episode(
    outline: Option<&RemoteSeriesEpisodeOutline>,
    season_number: i32,
    episode_number: i32,
) -> Option<&RemoteSeriesEpisode> {
    outline?
        .seasons
        .iter()
        .find(|season| season.season_number == season_number)?
        .episodes
        .iter()
        .find(|episode| episode.episode_number == episode_number)
}

fn provider_owned_artwork_paths(previous: &TmdbRemoteMetadataSnapshot) -> BTreeSet<String> {
    if previous.version != REMOTE_SNAPSHOT_VERSION {
        return BTreeSet::new();
    }

    let mut paths = [
        &previous.poster_path,
        &previous.backdrop_path,
        &previous.logo_path,
    ]
    .into_iter()
    .flatten()
    .cloned()
    .collect::<BTreeSet<_>>();
    if let Some(outline) = previous.series_outline.as_ref() {
        paths.extend(remote_outline_artwork_paths(outline));
    }
    paths
}

fn expiring_provider_artwork_paths(
    target: &TmdbMetadataRevalidationTarget,
    previous: &TmdbRemoteMetadataSnapshot,
    tmdb_artwork_root: &std::path::Path,
) -> BTreeSet<String> {
    let mut paths = provider_owned_artwork_paths(previous);
    for current in [
        target.poster_path.as_ref(),
        target.backdrop_path.as_ref(),
        target.logo_path.as_ref(),
    ]
    .into_iter()
    .flatten()
    .chain(
        target
            .seasons
            .iter()
            .flat_map(|season| [&season.poster_path, &season.backdrop_path])
            .flatten(),
    )
    .chain(
        target
            .episodes
            .iter()
            .flat_map(|episode| [&episode.poster_path, &episode.backdrop_path])
            .flatten(),
    ) {
        if is_tmdb_cached_artwork_path(current, tmdb_artwork_root) {
            paths.insert(current.clone());
        }
    }
    paths
}

fn remote_artwork_paths(metadata: &RemoteMetadata) -> BTreeSet<String> {
    [
        metadata.poster_path.as_ref(),
        metadata.backdrop_path.as_ref(),
        metadata.logo_path.as_ref(),
    ]
    .into_iter()
    .flatten()
    .cloned()
    .collect()
}

fn remote_outline_artwork_paths(outline: &RemoteSeriesEpisodeOutline) -> BTreeSet<String> {
    outline
        .seasons
        .iter()
        .flat_map(|season| {
            [season.poster_path.as_ref(), season.backdrop_path.as_ref()]
                .into_iter()
                .flatten()
                .chain(season.episodes.iter().flat_map(|episode| {
                    [episode.poster_path.as_ref(), episode.backdrop_path.as_ref()]
                        .into_iter()
                        .flatten()
                }))
        })
        .cloned()
        .collect()
}

pub(crate) fn materialized_tmdb_artwork_paths(
    metadata: &RemoteMetadata,
    outline: Option<&RemoteSeriesEpisodeOutline>,
    cache_dir: &std::path::Path,
    library_id: i64,
) -> BTreeSet<String> {
    let tmdb_root = crate::library_artwork_cache_dir(cache_dir, library_id).join("tmdb");
    remote_artwork_paths(metadata)
        .into_iter()
        .chain(
            outline
                .map(remote_outline_artwork_paths)
                .unwrap_or_default(),
        )
        .filter(|path| std::path::Path::new(path).starts_with(&tmdb_root))
        .collect()
}

pub(crate) fn materialized_tmdb_artwork_paths_from_files(
    files: &[mova_scan::DiscoveredMediaFile],
    cache_dir: &std::path::Path,
    library_id: i64,
) -> BTreeSet<String> {
    let tmdb_root = crate::library_artwork_cache_dir(cache_dir, library_id).join("tmdb");
    files
        .iter()
        .flat_map(|file| {
            [
                file.series_logo_path.as_ref(),
                file.series_poster_path.as_ref(),
                file.series_backdrop_path.as_ref(),
                file.season_poster_path.as_ref(),
                file.season_backdrop_path.as_ref(),
                file.poster_path.as_ref(),
                file.backdrop_path.as_ref(),
                file.logo_path.as_ref(),
            ]
            .into_iter()
            .flatten()
        })
        .filter(|path| std::path::Path::new(path).starts_with(&tmdb_root))
        .cloned()
        .collect()
}

pub(crate) async fn finish_tmdb_artwork_publication<T, E>(
    publication: mova_db::TmdbArtworkPublicationGuard,
    operation: Result<T, E>,
    pool: &PgPool,
    cache_dir: &std::path::Path,
    library_id: i64,
    artwork_paths: BTreeSet<String>,
    retain_on_success: bool,
) -> Result<T, E>
where
    E: From<anyhow::Error>,
{
    let release = publication.release().await;
    finish_tmdb_artwork_publication_after_release(
        operation,
        release,
        pool,
        cache_dir,
        library_id,
        artwork_paths,
        retain_on_success,
    )
    .await
}

async fn finish_tmdb_artwork_publication_after_release<T, E>(
    operation: Result<T, E>,
    release: anyhow::Result<()>,
    pool: &PgPool,
    cache_dir: &std::path::Path,
    library_id: i64,
    artwork_paths: BTreeSet<String>,
    retain_on_success: bool,
) -> Result<T, E>
where
    E: From<anyhow::Error>,
{
    match (operation, release) {
        (Ok(value), Ok(())) => {
            if !retain_on_success {
                remove_unreferenced_tmdb_artwork_best_effort(
                    pool,
                    cache_dir,
                    library_id,
                    artwork_paths,
                )
                .await;
            }
            Ok(value)
        }
        (Err(error), Ok(())) => {
            remove_unreferenced_tmdb_artwork_best_effort(
                pool,
                cache_dir,
                library_id,
                artwork_paths,
            )
            .await;
            Err(error)
        }
        (Ok(_), Err(release_error)) => {
            remove_unreferenced_tmdb_artwork_best_effort(
                pool,
                cache_dir,
                library_id,
                artwork_paths,
            )
            .await;
            Err(E::from(release_error))
        }
        (Err(error), Err(release_error)) => {
            tracing::warn!(
                library_id,
                error = ?release_error,
                "failed to release TMDB artwork publication guard after an operation error; preserving the operation error"
            );
            remove_unreferenced_tmdb_artwork_best_effort(
                pool,
                cache_dir,
                library_id,
                artwork_paths,
            )
            .await;
            Err(error)
        }
    }
}

async fn release_and_remove_unreferenced_tmdb_artwork_best_effort(
    publication: mova_db::TmdbArtworkPublicationGuard,
    pool: &PgPool,
    cache_dir: &std::path::Path,
    library_id: i64,
    artwork_paths: BTreeSet<String>,
) {
    if let Err(error) = publication.release().await {
        tracing::warn!(
            library_id,
            error = ?error,
            "failed to release TMDB artwork publication guard; attempting orphan cleanup after consuming the guard"
        );
    }

    remove_unreferenced_tmdb_artwork_best_effort(pool, cache_dir, library_id, artwork_paths).await;
}

async fn remove_unreferenced_tmdb_artwork_best_effort(
    pool: &PgPool,
    cache_dir: &std::path::Path,
    library_id: i64,
    artwork_paths: BTreeSet<String>,
) {
    if let Err(error) =
        execute_tmdb_artwork_cleanup(pool, cache_dir, library_id, artwork_paths).await
    {
        tracing::warn!(
            library_id,
            error = ?error,
            "failed to clean uncommitted TMDB artwork"
        );
    }
}

pub async fn execute_tmdb_artwork_cleanup(
    pool: &PgPool,
    cache_dir: &std::path::Path,
    library_id: i64,
    artwork_paths: impl IntoIterator<Item = String>,
) -> anyhow::Result<()> {
    let artwork_paths = artwork_paths.into_iter().collect::<BTreeSet<_>>();
    if artwork_paths.is_empty() {
        return Ok(());
    }

    let tmdb_root = crate::library_artwork_cache_dir(cache_dir, library_id).join("tmdb");
    let canonical_root = match tokio::fs::canonicalize(&tmdb_root).await {
        Ok(root) => root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    let mut candidates = Vec::new();
    for artwork_path in artwork_paths {
        let candidate = PathBuf::from(&artwork_path);
        if !candidate.starts_with(&tmdb_root) {
            tracing::warn!(
                library_id,
                "refusing to remove expired artwork outside the TMDB cache namespace"
            );
            continue;
        }

        let canonical_candidate = match tokio::fs::canonicalize(&candidate).await {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if !canonical_candidate.starts_with(&canonical_root) {
            tracing::warn!(
                library_id,
                "refusing to remove expired artwork through a cache symlink"
            );
            continue;
        }
        candidates.push((artwork_path, canonical_candidate));
    }
    if candidates.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    mova_db::lock_library_tmdb_artwork(&mut tx, library_id).await?;
    for (artwork_path, canonical_candidate) in candidates {
        if mova_db::is_artwork_path_referenced_tx(&mut tx, &artwork_path).await? {
            continue;
        }
        if let Err(error) = tokio::fs::remove_file(&canonical_candidate).await {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(error.into());
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Removes crash-orphaned TMDB artwork after the provider retention window.
///
/// Normal write failures clean their materialized files immediately. This
/// sweep covers the narrower process-exit window after atomic publication but
/// before the database reference commits. It never follows symlinks and the
/// final cleanup re-checks every database reference under the same exclusive
/// library locks used by regular cleanup jobs.
pub async fn execute_tmdb_artwork_orphan_sweep(
    pool: &PgPool,
    cache_dir: &std::path::Path,
    library_id: i64,
) -> anyhow::Result<usize> {
    let retention_seconds = u64::try_from(mova_db::TMDB_ARTWORK_RETENTION_DAYS)
        .unwrap_or(180)
        .saturating_mul(24 * 60 * 60);
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(retention_seconds))
        .unwrap_or(std::time::UNIX_EPOCH);
    execute_tmdb_artwork_orphan_sweep_before(pool, cache_dir, library_id, cutoff).await
}

async fn execute_tmdb_artwork_orphan_sweep_before(
    pool: &PgPool,
    cache_dir: &std::path::Path,
    library_id: i64,
    cutoff: std::time::SystemTime,
) -> anyhow::Result<usize> {
    let tmdb_root = crate::library_artwork_cache_dir(cache_dir, library_id).join("tmdb");
    match tokio::fs::symlink_metadata(&tmdb_root).await {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            anyhow::bail!("TMDB artwork cache root must not be a symlink");
        }
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => anyhow::bail!("TMDB artwork cache root must be a directory"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    }

    let mut directories = vec![tmdb_root];
    let mut candidates = BTreeSet::new();
    while let Some(directory) = directories.pop() {
        let mut entries = match tokio::fs::read_dir(&directory).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                directories.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let metadata = entry.metadata().await?;
            let modified = metadata.modified()?;
            if modified <= cutoff {
                candidates.insert(entry.path().to_string_lossy().to_string());
            }
        }
    }

    let candidate_count = candidates.len();
    execute_tmdb_artwork_cleanup(pool, cache_dir, library_id, candidates).await?;
    Ok(candidate_count)
}

impl TmdbRemoteMetadataSnapshot {
    fn from_remote(
        metadata: &RemoteMetadata,
        series_outline: Option<RemoteSeriesEpisodeOutline>,
    ) -> Self {
        Self {
            version: REMOTE_SNAPSHOT_VERSION,
            title: metadata.title.clone(),
            original_title: metadata.original_title.clone(),
            year: metadata.year,
            country: metadata.country.clone(),
            genres: metadata.genres.clone(),
            studio: metadata.studio.clone(),
            overview: metadata.overview.clone(),
            poster_path: metadata.poster_path.clone(),
            backdrop_path: metadata.backdrop_path.clone(),
            logo_path: metadata.logo_path.clone(),
            series_outline,
        }
    }
}

pub(crate) fn serialize_tmdb_remote_snapshot(
    metadata: &RemoteMetadata,
    series_outline: Option<RemoteSeriesEpisodeOutline>,
) -> anyhow::Result<String> {
    serde_json::to_string(&TmdbRemoteMetadataSnapshot::from_remote(
        metadata,
        series_outline,
    ))
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::{
        execute_tmdb_artwork_orphan_sweep_before, expire_optional_provider_artwork,
        expire_optional_provider_value, expire_revalidated_episodes, expire_revalidated_seasons,
        expire_title, finish_tmdb_artwork_publication,
        finish_tmdb_artwork_publication_after_release, merge_optional_provider_artwork,
        merge_optional_provider_value, merge_title, TmdbRemoteMetadataSnapshot,
        REMOTE_SNAPSHOT_VERSION,
    };
    use crate::metadata::{RemoteSeriesEpisode, RemoteSeriesEpisodeOutline, RemoteSeriesSeason};
    use mova_db::{TmdbRevalidationEpisode, TmdbRevalidationSeason};
    use std::{collections::BTreeSet, path::Path};
    use time::OffsetDateTime;
    use uuid::Uuid;

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn failed_publication_removes_materialized_unreferenced_artwork(pool: sqlx::PgPool) {
        let library_id = sqlx::query_scalar::<_, i64>(
            "insert into libraries (name, root_path) values ('Artwork', '/media') returning id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let cache_dir =
            std::env::temp_dir().join(format!("mova-artwork-publication-{}", Uuid::new_v4()));
        let artwork_path = crate::library_artwork_cache_dir(&cache_dir, library_id)
            .join("tmdb")
            .join("poster")
            .join("orphan.jpg");
        tokio::fs::create_dir_all(artwork_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&artwork_path, b"complete image")
            .await
            .unwrap();

        let publication = mova_db::TmdbArtworkPublicationGuard::acquire(&pool, library_id)
            .await
            .unwrap();
        let operation = Err::<(), anyhow::Error>(anyhow::anyhow!("reference commit failed"));
        let result = finish_tmdb_artwork_publication(
            publication,
            operation,
            &pool,
            &cache_dir,
            library_id,
            BTreeSet::from([artwork_path.to_string_lossy().to_string()]),
            true,
        )
        .await;

        assert!(result.is_err());
        assert!(tokio::fs::metadata(&artwork_path).await.is_err());
        let _ = tokio::fs::remove_dir_all(cache_dir).await;
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn release_failure_still_cleans_artwork_and_preserves_the_primary_error(
        pool: sqlx::PgPool,
    ) {
        let library_id = sqlx::query_scalar::<_, i64>(
            "insert into libraries (name, root_path) values ('Release failure', '/media') returning id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let cache_dir =
            std::env::temp_dir().join(format!("mova-artwork-release-{}", Uuid::new_v4()));
        let tmdb_dir = crate::library_artwork_cache_dir(&cache_dir, library_id)
            .join("tmdb")
            .join("poster");
        tokio::fs::create_dir_all(&tmdb_dir).await.unwrap();
        let release_only_path = tmdb_dir.join("release-only.jpg");
        let combined_error_path = tmdb_dir.join("combined-error.jpg");
        tokio::fs::write(&release_only_path, b"complete image")
            .await
            .unwrap();
        tokio::fs::write(&combined_error_path, b"complete image")
            .await
            .unwrap();

        let release_only = finish_tmdb_artwork_publication_after_release(
            Ok::<(), anyhow::Error>(()),
            Err(anyhow::anyhow!("publication release failed")),
            &pool,
            &cache_dir,
            library_id,
            BTreeSet::from([release_only_path.to_string_lossy().to_string()]),
            true,
        )
        .await
        .unwrap_err();
        assert!(release_only
            .to_string()
            .contains("publication release failed"));
        assert!(tokio::fs::metadata(&release_only_path).await.is_err());

        let combined = finish_tmdb_artwork_publication_after_release(
            Err::<(), anyhow::Error>(anyhow::anyhow!("reference commit failed")),
            Err(anyhow::anyhow!("publication release failed")),
            &pool,
            &cache_dir,
            library_id,
            BTreeSet::from([combined_error_path.to_string_lossy().to_string()]),
            true,
        )
        .await
        .unwrap_err();
        assert!(combined.to_string().contains("reference commit failed"));
        assert!(tokio::fs::metadata(&combined_error_path).await.is_err());
        let _ = tokio::fs::remove_dir_all(cache_dir).await;
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn orphan_sweep_removes_only_old_unreferenced_tmdb_artwork(pool: sqlx::PgPool) {
        let library_id = sqlx::query_scalar::<_, i64>(
            "insert into libraries (name, root_path) values ('Orphan sweep', '/media') returning id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let cache_dir = std::env::temp_dir().join(format!("mova-orphan-sweep-{}", Uuid::new_v4()));
        let tmdb_dir = crate::library_artwork_cache_dir(&cache_dir, library_id)
            .join("tmdb")
            .join("poster");
        tokio::fs::create_dir_all(&tmdb_dir).await.unwrap();
        let referenced_path = tmdb_dir.join("referenced.jpg");
        let orphan_path = tmdb_dir.join("orphan.jpg");
        tokio::fs::write(&referenced_path, b"referenced image")
            .await
            .unwrap();
        tokio::fs::write(&orphan_path, b"orphan image")
            .await
            .unwrap();

        sqlx::query(
            r#"
            insert into media_items (
                library_id,
                media_type,
                title,
                source_title,
                poster_path
            )
            values ($1, 'movie', 'Referenced', 'Referenced', $2)
            "#,
        )
        .bind(library_id)
        .bind(referenced_path.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .unwrap();

        let reviewed = execute_tmdb_artwork_orphan_sweep_before(
            &pool,
            &cache_dir,
            library_id,
            std::time::SystemTime::now() + std::time::Duration::from_secs(1),
        )
        .await
        .unwrap();

        assert_eq!(reviewed, 2);
        assert!(tokio::fs::metadata(&referenced_path).await.is_ok());
        assert!(tokio::fs::metadata(&orphan_path).await.is_err());
        let _ = tokio::fs::remove_dir_all(cache_dir).await;
    }

    #[test]
    fn first_revalidation_does_not_claim_nonempty_local_values() {
        assert_eq!(
            merge_title(
                "NFO title".to_string(),
                "Source title".to_string(),
                None,
                Some("TMDB title".to_string()),
                false,
            ),
            "NFO title"
        );
        assert_eq!(
            merge_optional_provider_value(
                Some("Local overview".to_string()),
                None,
                Some("Remote overview".to_string()),
                false,
            ),
            Some("Local overview".to_string())
        );
        assert_eq!(
            merge_optional_provider_value::<String>(
                None,
                None,
                Some("Remote overview".to_string()),
                false,
            ),
            Some("Remote overview".to_string())
        );
    }

    #[test]
    fn first_exact_remote_echo_establishes_ownership_for_later_expiry() {
        let incoming = "Same overview".to_string();
        let merged = merge_optional_provider_value(
            Some(incoming.clone()),
            None,
            Some(incoming.clone()),
            false,
        );

        assert_eq!(merged, Some(incoming.clone()));
        assert_eq!(
            expire_optional_provider_value(merged, Some(&incoming), true),
            None
        );
    }

    #[test]
    fn first_revalidation_claims_only_artwork_inside_tmdb_cache_namespace() {
        let root = Path::new("/cache/libraries/7/artwork/tmdb");
        let old_cached = "/cache/libraries/7/artwork/tmdb/poster/old.jpg".to_string();
        let incoming = "/cache/libraries/7/artwork/tmdb/poster/new.jpg".to_string();
        let merged = merge_optional_provider_artwork(
            Some(old_cached),
            None,
            Some(incoming.clone()),
            false,
            root,
        );

        assert_eq!(merged, Some(incoming.clone()));
        assert_eq!(
            expire_optional_provider_artwork(merged, Some(&incoming), true, root),
            None
        );

        let sidecar = "/media/show/poster.jpg".to_string();
        assert_eq!(
            merge_optional_provider_artwork(
                Some(sidecar.clone()),
                None,
                Some(incoming),
                false,
                root,
            ),
            Some(sidecar)
        );
    }

    #[test]
    fn later_revalidation_only_updates_values_still_owned_by_tmdb() {
        let previous_remote = "Previous remote".to_string();
        assert_eq!(
            merge_optional_provider_value(
                Some(previous_remote.clone()),
                Some(&previous_remote),
                Some("New remote".to_string()),
                true,
            ),
            Some("New remote".to_string())
        );
        assert_eq!(
            merge_optional_provider_value(
                Some("NFO override".to_string()),
                Some(&previous_remote),
                Some("New remote".to_string()),
                true,
            ),
            Some("NFO override".to_string())
        );
    }

    #[test]
    fn provider_owned_value_can_be_cleared_when_tmdb_removes_it() {
        let previous_remote = "Previous remote".to_string();
        assert_eq!(
            merge_optional_provider_value(
                Some(previous_remote.clone()),
                Some(&previous_remote),
                None,
                true,
            ),
            None
        );
    }

    #[test]
    fn retention_expiry_clears_owned_values_and_legacy_unknown_enrichment() {
        let previous_remote = "Remote overview".to_string();
        assert_eq!(
            expire_optional_provider_value(
                Some(previous_remote.clone()),
                Some(&previous_remote),
                true,
            ),
            None
        );
        assert_eq!(
            expire_optional_provider_value(
                Some("NFO override".to_string()),
                Some(&previous_remote),
                true,
            ),
            Some("NFO override".to_string())
        );
        assert_eq!(
            expire_optional_provider_value(
                Some(previous_remote.clone()),
                Some(&previous_remote),
                false,
            ),
            None
        );
        assert_eq!(
            expire_title(
                "Legacy display title".to_string(),
                "Source title".to_string(),
                None,
                false,
            ),
            "Source title"
        );
    }

    #[test]
    fn legacy_empty_snapshot_expires_unknown_season_and_episode_enrichment() {
        let now = OffsetDateTime::now_utc();
        let root = Path::new("/cache/libraries/7/artwork/tmdb");
        let seasons = expire_revalidated_seasons(
            &[TmdbRevalidationSeason {
                season_id: 11,
                season_number: 1,
                title: Some("Unknown season title".to_string()),
                overview: Some("Unknown season overview".to_string()),
                poster_path: Some("/media/show/season-poster.jpg".to_string()),
                backdrop_path: Some("/media/show/season-backdrop.jpg".to_string()),
                observed_updated_at: now,
            }],
            &TmdbRemoteMetadataSnapshot::default(),
            root,
        );
        let episodes = expire_revalidated_episodes(
            &[TmdbRevalidationEpisode {
                media_item_id: 12,
                season_number: 1,
                episode_number: 1,
                title: "Unknown episode title".to_string(),
                source_title: "Episode 01".to_string(),
                overview: Some("Unknown episode overview".to_string()),
                poster_path: Some("/media/show/episode-poster.jpg".to_string()),
                backdrop_path: Some("/media/show/episode-backdrop.jpg".to_string()),
                observed_updated_at: now,
            }],
            &TmdbRemoteMetadataSnapshot::default(),
            root,
        );

        assert_eq!(seasons[0].title.as_deref(), Some("Season 01"));
        assert_eq!(seasons[0].overview, None);
        assert_eq!(seasons[0].poster_path, None);
        assert_eq!(seasons[0].backdrop_path, None);
        assert_eq!(episodes[0].title, "Episode 01");
        assert_eq!(episodes[0].overview, None);
        assert_eq!(episodes[0].poster_path, None);
        assert_eq!(episodes[0].backdrop_path, None);
    }

    #[test]
    fn retention_expiry_clears_owned_season_and_episode_values_but_keeps_overrides() {
        let root = Path::new("/cache/libraries/7/artwork/tmdb");
        let now = OffsetDateTime::now_utc();
        let snapshot = TmdbRemoteMetadataSnapshot {
            version: REMOTE_SNAPSHOT_VERSION,
            series_outline: Some(RemoteSeriesEpisodeOutline {
                seasons: vec![RemoteSeriesSeason {
                    season_number: 1,
                    title: Some("TMDB season".to_string()),
                    overview: Some("TMDB season overview".to_string()),
                    poster_path: Some(
                        "/cache/libraries/7/artwork/tmdb/poster/season.jpg".to_string(),
                    ),
                    backdrop_path: Some("TMDB season backdrop".to_string()),
                    episodes: vec![RemoteSeriesEpisode {
                        episode_number: 1,
                        title: Some("TMDB episode".to_string()),
                        overview: Some("TMDB episode overview".to_string()),
                        poster_path: Some(
                            "/cache/libraries/7/artwork/tmdb/poster/episode.jpg".to_string(),
                        ),
                        backdrop_path: Some("TMDB episode backdrop".to_string()),
                    }],
                    ..RemoteSeriesSeason::default()
                }],
            }),
            ..TmdbRemoteMetadataSnapshot::default()
        };
        let seasons = expire_revalidated_seasons(
            &[TmdbRevalidationSeason {
                season_id: 11,
                season_number: 1,
                title: Some("TMDB season".to_string()),
                overview: Some("TMDB season overview".to_string()),
                poster_path: Some("/cache/libraries/7/artwork/tmdb/poster/season.jpg".to_string()),
                backdrop_path: Some("Local season backdrop".to_string()),
                observed_updated_at: now,
            }],
            &snapshot,
            root,
        );
        let episodes = expire_revalidated_episodes(
            &[TmdbRevalidationEpisode {
                media_item_id: 12,
                season_number: 1,
                episode_number: 1,
                title: "TMDB episode".to_string(),
                source_title: "Episode 01".to_string(),
                overview: Some("TMDB episode overview".to_string()),
                poster_path: Some("/cache/libraries/7/artwork/tmdb/poster/episode.jpg".to_string()),
                backdrop_path: Some("Local episode backdrop".to_string()),
                observed_updated_at: now,
            }],
            &snapshot,
            root,
        );

        assert_eq!(seasons[0].title.as_deref(), Some("Season 01"));
        assert_eq!(seasons[0].overview, None);
        assert_eq!(seasons[0].poster_path, None);
        assert_eq!(
            seasons[0].backdrop_path.as_deref(),
            Some("Local season backdrop")
        );
        assert_eq!(episodes[0].title, "Episode 01");
        assert_eq!(episodes[0].overview, None);
        assert_eq!(episodes[0].poster_path, None);
        assert_eq!(
            episodes[0].backdrop_path.as_deref(),
            Some("Local episode backdrop")
        );
    }
}
