use crate::{
    error::{ApplicationError, ApplicationResult},
    invalidate_media_item_cast_cache,
    libraries::get_library,
    media_classification::metadata_lookup_type_for_media_type,
    media_enrichment::MetadataEnrichmentContext,
    metadata::{MetadataLookup, MetadataProvider, RemoteSeriesEpisodeOutline},
};
use mova_domain::{Episode, MediaFile, MediaItem, PlaybackProgress, Season, SubtitleFile};
use sqlx::postgres::PgPool;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::Arc,
};
use time::{Duration, OffsetDateTime};

const DEFAULT_MEDIA_ITEMS_PAGE: i64 = 1;
const DEFAULT_MEDIA_ITEMS_PAGE_SIZE: i64 = 50;
const MAX_MEDIA_ITEMS_PAGE_SIZE: i64 = 100;
const SERIES_EPISODE_OUTLINE_CACHE_TTL_SECONDS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Default)]
pub struct ListMediaItemsForLibraryInput {
    pub query: Option<String>,
    pub year: Option<i32>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ListMediaItemsForLibraryOutput {
    pub items: Vec<MediaItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Clone, Default)]
pub struct SeriesEpisodeOutline {
    pub seasons: Vec<SeriesEpisodeOutlineSeason>,
}

#[derive(Debug, Clone)]
pub struct SeriesEpisodeOutlineSeason {
    pub season_id: Option<i64>,
    pub season_number: i32,
    pub title: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub episodes: Vec<SeriesEpisodeOutlineEpisode>,
}

#[derive(Debug, Clone)]
pub struct SeriesEpisodeOutlineEpisode {
    pub episode_number: i32,
    pub title: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub media_item_id: Option<i64>,
    pub is_available: bool,
    pub playback_progress: Option<PlaybackProgress>,
}

#[derive(Debug, Clone)]
struct LocalSeriesSeason {
    season_id: i64,
    title: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    episodes: BTreeMap<i32, LocalSeriesEpisode>,
}

#[derive(Debug, Clone)]
struct LocalSeriesEpisode {
    media_item_id: i64,
    title: String,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedRemoteOutline {
    outline: RemoteSeriesEpisodeOutline,
    is_fresh: bool,
}

/// 读取某个媒体库下已经扫描入库的媒体条目。
/// 先校验媒体库存在，避免对不存在的 id 返回误导性的空列表。
pub async fn list_media_items_for_library(
    pool: &PgPool,
    library_id: i64,
    input: ListMediaItemsForLibraryInput,
) -> ApplicationResult<ListMediaItemsForLibraryOutput> {
    get_library(pool, library_id).await?;
    let query = normalize_query(input.query);
    let year = normalize_year(input.year)?;
    let page = normalize_page(input.page)?;
    let page_size = normalize_page_size(input.page_size)?;
    let offset = (page - 1) * page_size;

    let result = mova_db::list_media_items_for_library(
        pool,
        mova_db::ListMediaItemsForLibraryParams {
            library_id,
            query,
            year,
            limit: page_size,
            offset,
        },
    )
    .await
    .map_err(ApplicationError::from)?;

    Ok(ListMediaItemsForLibraryOutput {
        items: result.items,
        total: result.total,
        page,
        page_size,
    })
}

/// 按 id 读取单个媒体条目。
pub async fn get_media_item(pool: &PgPool, media_item_id: i64) -> ApplicationResult<MediaItem> {
    let media_item = mova_db::get_media_item(pool, media_item_id)
        .await
        .map_err(ApplicationError::from)?;

    media_item.ok_or_else(|| {
        ApplicationError::NotFound(format!("media item not found: {}", media_item_id))
    })
}

/// 按 id 读取单个媒体文件。
pub async fn get_media_file(pool: &PgPool, media_file_id: i64) -> ApplicationResult<MediaFile> {
    let media_file = mova_db::get_media_file(pool, media_file_id)
        .await
        .map_err(ApplicationError::from)?;

    media_file.ok_or_else(|| {
        ApplicationError::NotFound(format!("media file not found: {}", media_file_id))
    })
}

/// 读取某个媒体条目关联的文件列表。
/// 先校验媒体条目存在，避免对不存在的 id 返回误导性的空列表。
pub async fn list_media_files_for_media_item(
    pool: &PgPool,
    media_item_id: i64,
) -> ApplicationResult<Vec<MediaFile>> {
    get_media_item(pool, media_item_id).await?;

    mova_db::list_media_files_for_media_item(pool, media_item_id)
        .await
        .map_err(ApplicationError::from)
}

/// 读取某个媒体文件可切换的字幕轨道。
/// 播放器切换字幕时按媒体文件维度查询，避免多版本文件误共享字幕列表。
pub async fn list_subtitle_files_for_media_file(
    pool: &PgPool,
    media_file_id: i64,
) -> ApplicationResult<Vec<SubtitleFile>> {
    get_media_file(pool, media_file_id).await?;

    mova_db::list_subtitle_files_for_media_file(pool, media_file_id)
        .await
        .map_err(ApplicationError::from)
}

pub async fn get_subtitle_file(
    pool: &PgPool,
    subtitle_file_id: i64,
) -> ApplicationResult<SubtitleFile> {
    mova_db::get_subtitle_file(pool, subtitle_file_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| {
            ApplicationError::NotFound(format!("subtitle file not found: {}", subtitle_file_id))
        })
}

pub async fn list_seasons_for_series(
    pool: &PgPool,
    series_id: i64,
) -> ApplicationResult<Vec<Season>> {
    let media_item = get_media_item(pool, series_id).await?;
    if !media_item.media_type.eq_ignore_ascii_case("series") {
        return Err(ApplicationError::Validation(format!(
            "media item {} is not a series",
            series_id
        )));
    }

    mova_db::list_seasons_for_series(pool, series_id)
        .await
        .map_err(ApplicationError::from)
}

pub async fn get_season(pool: &PgPool, season_id: i64) -> ApplicationResult<Season> {
    mova_db::get_season(pool, season_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| ApplicationError::NotFound(format!("season not found: {}", season_id)))
}

pub async fn list_episodes_for_season(
    pool: &PgPool,
    season_id: i64,
) -> ApplicationResult<Vec<Episode>> {
    mova_db::get_season(pool, season_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| ApplicationError::NotFound(format!("season not found: {}", season_id)))?;

    mova_db::list_episodes_for_season(pool, season_id)
        .await
        .map_err(ApplicationError::from)
}

pub async fn series_episode_outline_for_media_item(
    pool: &PgPool,
    user_id: i64,
    media_item_id: i64,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> ApplicationResult<SeriesEpisodeOutline> {
    let media_item = get_media_item(pool, media_item_id).await?;
    if !media_item.media_type.eq_ignore_ascii_case("series") {
        return Err(ApplicationError::Validation(format!(
            "media item {} is not a series",
            media_item_id
        )));
    }

    // 剧集 outline 的语言跟库配置走，避免同一部剧在不同库里混出中英双语季集信息。
    let library = get_library(pool, media_item.library_id).await?;
    let local_inventory = load_local_series_inventory(pool, media_item_id).await?;
    let playback_progress_by_media_item =
        load_series_episode_playback_progress(pool, user_id, &local_inventory).await?;
    let cached_remote_outline = load_cached_remote_outline(pool, media_item_id).await?;
    if let Some(cached_remote_outline) = cached_remote_outline.as_ref() {
        if cached_remote_outline.is_fresh {
            // 热缓存直接返回，避免剧集页和播放器页频繁击穿 TMDB。
            return Ok(merge_remote_outline_with_local(
                cached_remote_outline.outline.clone(),
                &local_inventory,
                &playback_progress_by_media_item,
            ));
        }
    }

    let lookup_title = media_item
        .original_title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(media_item.title.as_str())
        .to_string();
    let lookup = MetadataLookup {
        title: lookup_title,
        year: media_item.year,
        library_type: "series".to_string(),
        language: Some(library.metadata_language.clone()),
        provider_item_id: media_item.metadata_provider_item_id,
    };

    let remote_outline = match metadata_provider
        .lookup_series_episode_outline(&lookup)
        .await
    {
        Ok(remote_outline) => {
            if let Some(remote_outline) = remote_outline.as_ref() {
                cache_remote_outline(pool, media_item_id, remote_outline).await?;
            }

            remote_outline
        }
        Err(error) => {
            tracing::warn!(
                media_item_id,
                title = %lookup.title,
                error = ?error,
                "failed to query remote episode outline, falling back to local inventory"
            );
            None
        }
    };

    if let Some(remote_outline) = remote_outline {
        return Ok(merge_remote_outline_with_local(
            remote_outline,
            &local_inventory,
            &playback_progress_by_media_item,
        ));
    }

    if let Some(cached_remote_outline) = cached_remote_outline {
        return Ok(merge_remote_outline_with_local(
            cached_remote_outline.outline,
            &local_inventory,
            &playback_progress_by_media_item,
        ));
    }

    Ok(build_local_outline(
        &local_inventory,
        &playback_progress_by_media_item,
    ))
}

async fn load_cached_remote_outline(
    pool: &PgPool,
    media_item_id: i64,
) -> ApplicationResult<Option<CachedRemoteOutline>> {
    let cache_entry = match mova_db::get_series_episode_outline_cache(pool, media_item_id).await {
        Ok(cache_entry) => cache_entry,
        Err(error) => {
            if is_missing_series_outline_cache_table_error(&error) {
                tracing::warn!(
                    media_item_id,
                    "series episode outline cache table does not exist yet, skipping cache read"
                );
                return Ok(None);
            }

            return Err(ApplicationError::from(error));
        }
    };
    let Some(cache_entry) = cache_entry else {
        return Ok(None);
    };

    let outline =
        match serde_json::from_str::<RemoteSeriesEpisodeOutline>(&cache_entry.outline_json) {
            Ok(outline) => outline,
            Err(error) => {
                tracing::warn!(
                    media_item_id,
                    error = ?error,
                    "failed to parse series episode outline cache, deleting invalid cache entry"
                );

                if let Err(delete_error) =
                    mova_db::delete_series_episode_outline_cache(pool, media_item_id).await
                {
                    if !is_missing_series_outline_cache_table_error(&delete_error) {
                        tracing::warn!(
                            media_item_id,
                            error = ?delete_error,
                            "failed to delete invalid series episode outline cache entry"
                        );
                    }
                }

                return Ok(None);
            }
        };

    Ok(Some(CachedRemoteOutline {
        outline,
        is_fresh: cache_entry.expires_at > OffsetDateTime::now_utc(),
    }))
}

async fn cache_remote_outline(
    pool: &PgPool,
    media_item_id: i64,
    remote_outline: &RemoteSeriesEpisodeOutline,
) -> ApplicationResult<()> {
    let outline_json = serde_json::to_string(remote_outline).map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "failed to serialize series episode outline cache payload for media item {}: {}",
            media_item_id,
            error
        ))
    })?;

    let fetched_at = OffsetDateTime::now_utc();
    let expires_at = fetched_at
        .checked_add(Duration::seconds(SERIES_EPISODE_OUTLINE_CACHE_TTL_SECONDS))
        .unwrap_or(fetched_at);

    match mova_db::upsert_series_episode_outline_cache(
        pool,
        mova_db::UpsertSeriesEpisodeOutlineCacheParams {
            series_media_item_id: media_item_id,
            outline_json,
            fetched_at,
            expires_at,
        },
    )
    .await
    {
        Ok(_) => {}
        Err(error) if is_missing_series_outline_cache_table_error(&error) => {
            tracing::warn!(
                media_item_id,
                "series episode outline cache table does not exist yet, skipping cache write"
            );
        }
        Err(error) => return Err(ApplicationError::from(error)),
    }

    Ok(())
}

async fn invalidate_series_episode_outline_cache(
    pool: &PgPool,
    media_item_id: i64,
) -> ApplicationResult<()> {
    match mova_db::delete_series_episode_outline_cache(pool, media_item_id).await {
        Ok(()) => {}
        Err(error) if is_missing_series_outline_cache_table_error(&error) => {
            tracing::warn!(
                media_item_id,
                "series episode outline cache table does not exist yet, skipping cache delete"
            );
        }
        Err(error) => return Err(ApplicationError::from(error)),
    }

    Ok(())
}

fn is_missing_series_outline_cache_table_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("series_episode_outline_cache")
        && (message.contains("does not exist")
            || message.contains("undefined_table")
            || message.contains("42p01"))
}

async fn load_local_series_inventory(
    pool: &PgPool,
    series_id: i64,
) -> ApplicationResult<BTreeMap<i32, LocalSeriesSeason>> {
    let seasons = mova_db::list_seasons_for_series(pool, series_id)
        .await
        .map_err(ApplicationError::from)?;
    let mut inventory = BTreeMap::new();

    for season in seasons {
        let episodes = mova_db::list_episodes_for_season(pool, season.id)
            .await
            .map_err(ApplicationError::from)?;
        let mut season_episodes = BTreeMap::new();

        for episode in episodes {
            season_episodes.insert(
                episode.episode_number,
                LocalSeriesEpisode {
                    media_item_id: episode.media_item_id,
                    title: episode.title,
                    overview: episode.overview,
                    poster_path: episode.poster_path,
                    backdrop_path: episode.backdrop_path,
                },
            );
        }

        inventory.insert(
            season.season_number,
            LocalSeriesSeason {
                season_id: season.id,
                title: season.title,
                overview: season.overview,
                poster_path: season.poster_path,
                backdrop_path: season.backdrop_path,
                episodes: season_episodes,
            },
        );
    }

    Ok(inventory)
}

async fn load_series_episode_playback_progress(
    pool: &PgPool,
    user_id: i64,
    local_inventory: &BTreeMap<i32, LocalSeriesSeason>,
) -> ApplicationResult<HashMap<i64, PlaybackProgress>> {
    let media_item_ids = local_inventory
        .values()
        .flat_map(|season| {
            season
                .episodes
                .values()
                .map(|episode| episode.media_item_id)
        })
        .collect::<Vec<_>>();

    mova_db::list_playback_progress_for_media_items(pool, user_id, &media_item_ids)
        .await
        .map_err(ApplicationError::from)
}

fn build_local_outline(
    local_inventory: &BTreeMap<i32, LocalSeriesSeason>,
    playback_progress_by_media_item: &HashMap<i64, PlaybackProgress>,
) -> SeriesEpisodeOutline {
    let seasons = local_inventory
        .iter()
        .map(|(season_number, season)| {
            let episodes = season
                .episodes
                .iter()
                .map(|(episode_number, episode)| SeriesEpisodeOutlineEpisode {
                    episode_number: *episode_number,
                    title: episode.title.clone(),
                    overview: episode.overview.clone(),
                    poster_path: episode.poster_path.clone(),
                    backdrop_path: episode.backdrop_path.clone(),
                    media_item_id: Some(episode.media_item_id),
                    is_available: true,
                    playback_progress: playback_progress_by_media_item
                        .get(&episode.media_item_id)
                        .cloned(),
                })
                .collect();

            SeriesEpisodeOutlineSeason {
                season_id: Some(season.season_id),
                season_number: *season_number,
                title: season.title.clone(),
                year: None,
                overview: season.overview.clone(),
                poster_path: season.poster_path.clone(),
                backdrop_path: season.backdrop_path.clone(),
                episodes,
            }
        })
        .collect();

    SeriesEpisodeOutline { seasons }
}

fn merge_remote_outline_with_local(
    remote_outline: RemoteSeriesEpisodeOutline,
    local_inventory: &BTreeMap<i32, LocalSeriesSeason>,
    playback_progress_by_media_item: &HashMap<i64, PlaybackProgress>,
) -> SeriesEpisodeOutline {
    // 远端负责补齐季集标题/简介/远端封面，本地负责决定“哪些季集真实可播”以及
    // 当前用户的播放进度。最终结果始终以本地库存为边界，不展示完全不存在的季。
    let mut merged_seasons = BTreeMap::new();
    let mut seen_remote_seasons = BTreeSet::new();

    for remote_season in remote_outline.seasons {
        let season_number = remote_season.season_number;
        if season_number < 1 {
            continue;
        }

        let Some(local_season) = local_inventory.get(&season_number) else {
            // 仅返回至少有本地资源的季；纯远端季不出现在结果中。
            continue;
        };

        seen_remote_seasons.insert(season_number);
        let mut seen_remote_episodes = BTreeSet::new();
        let mut episodes = Vec::new();

        for remote_episode in remote_season.episodes {
            let episode_number = remote_episode.episode_number;
            if episode_number < 1 {
                continue;
            }

            seen_remote_episodes.insert(episode_number);
            let local_episode = local_season.episodes.get(&episode_number);
            let title = remote_episode
                .title
                .and_then(|title| normalize_non_empty(title))
                .or_else(|| local_episode.map(|episode| episode.title.clone()))
                .unwrap_or_else(|| default_episode_title(episode_number));

            episodes.push(SeriesEpisodeOutlineEpisode {
                episode_number,
                title,
                overview: remote_episode
                    .overview
                    .or_else(|| local_episode.and_then(|episode| episode.overview.clone())),
                poster_path: local_episode
                    .and_then(|episode| episode.poster_path.clone())
                    .or(remote_episode.poster_path),
                backdrop_path: local_episode
                    .and_then(|episode| episode.backdrop_path.clone())
                    .or(remote_episode.backdrop_path),
                media_item_id: local_episode.map(|episode| episode.media_item_id),
                is_available: local_episode.is_some(),
                playback_progress: local_episode
                    .and_then(|episode| playback_progress_by_media_item.get(&episode.media_item_id))
                    .cloned(),
            });
        }

        for (episode_number, local_episode) in &local_season.episodes {
            if seen_remote_episodes.contains(episode_number) {
                continue;
            }

            // 允许本地多出一集：比如 TMDB 尚未同步、文件命名更早，或者是用户私有内容。
            episodes.push(SeriesEpisodeOutlineEpisode {
                episode_number: *episode_number,
                title: local_episode.title.clone(),
                overview: local_episode.overview.clone(),
                poster_path: local_episode.poster_path.clone(),
                backdrop_path: local_episode.backdrop_path.clone(),
                media_item_id: Some(local_episode.media_item_id),
                is_available: true,
                playback_progress: playback_progress_by_media_item
                    .get(&local_episode.media_item_id)
                    .cloned(),
            });
        }

        episodes.sort_by_key(|episode| episode.episode_number);
        merged_seasons.insert(
            season_number,
            SeriesEpisodeOutlineSeason {
                season_id: Some(local_season.season_id),
                season_number,
                title: normalize_non_empty_opt(remote_season.title)
                    .or_else(|| local_season.title.clone()),
                year: remote_season.year,
                overview: remote_season
                    .overview
                    .or_else(|| local_season.overview.clone()),
                poster_path: local_season
                    .poster_path
                    .clone()
                    .or(remote_season.poster_path),
                backdrop_path: local_season
                    .backdrop_path
                    .clone()
                    .or(remote_season.backdrop_path),
                episodes,
            },
        );
    }

    for (season_number, local_season) in local_inventory {
        if seen_remote_seasons.contains(season_number) {
            continue;
        }

        let episodes = local_season
            .episodes
            .iter()
            .map(
                |(episode_number, local_episode)| SeriesEpisodeOutlineEpisode {
                    episode_number: *episode_number,
                    title: local_episode.title.clone(),
                    overview: local_episode.overview.clone(),
                    poster_path: local_episode.poster_path.clone(),
                    backdrop_path: local_episode.backdrop_path.clone(),
                    media_item_id: Some(local_episode.media_item_id),
                    is_available: true,
                    playback_progress: playback_progress_by_media_item
                        .get(&local_episode.media_item_id)
                        .cloned(),
                },
            )
            .collect();

        merged_seasons.insert(
            *season_number,
            SeriesEpisodeOutlineSeason {
                season_id: Some(local_season.season_id),
                season_number: *season_number,
                title: local_season.title.clone(),
                year: None,
                overview: local_season.overview.clone(),
                poster_path: local_season.poster_path.clone(),
                backdrop_path: local_season.backdrop_path.clone(),
                episodes,
            },
        );
    }

    SeriesEpisodeOutline {
        seasons: merged_seasons.into_values().collect(),
    }
}

fn normalize_non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_non_empty_opt(value: Option<String>) -> Option<String> {
    value.and_then(normalize_non_empty)
}

fn default_episode_title(episode_number: i32) -> String {
    format!("Episode {}", episode_number)
}

/// 手动重拉单个媒体条目的本地 sidecar 与远程元数据。
pub async fn refresh_media_item_metadata(
    pool: &PgPool,
    media_item_id: i64,
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> ApplicationResult<MediaItem> {
    let media_item = get_media_item(pool, media_item_id).await?;
    let source_file = mova_db::list_media_files_for_media_item(pool, media_item_id)
        .await
        .map_err(ApplicationError::from)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            ApplicationError::Conflict(format!(
                "media item {} has no source file to refresh from",
                media_item_id
            ))
        })?;

    let mut discovered_file = inspect_media_file_path(&source_file.file_path)
        .await
        .map_err(|error| map_refresh_source_error(media_item_id, &source_file.file_path, error))?;

    let file_size = i64::try_from(discovered_file.file_size).map_err(|_| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "file is too large to store in database: {}",
            source_file.file_path
        ))
    })?;

    let lookup_type = metadata_lookup_type_for_media_type(&media_item.media_type);
    let library = get_library(pool, media_item.library_id).await?;
    let mut enrichment = MetadataEnrichmentContext::new(
        artwork_cache_dir,
        metadata_provider,
        library.metadata_language,
    );
    enrichment
        .enrich_file(lookup_type, &mut discovered_file)
        .await;

    mova_db::update_media_file_metadata(
        pool,
        source_file.id,
        mova_db::UpdateMediaFileMetadataParams {
            file_path: source_file.file_path.clone(),
            container: discovered_file.container.clone(),
            file_size,
            duration_seconds: discovered_file.duration_seconds,
            video_codec: discovered_file.video_codec.clone(),
            audio_codec: discovered_file.audio_codec.clone(),
            width: discovered_file.width,
            height: discovered_file.height,
            bitrate: discovered_file.bitrate,
        },
    )
    .await
    .map_err(ApplicationError::from)?
    .ok_or_else(|| {
        ApplicationError::NotFound(format!("media file not found: {}", source_file.id))
    })?;

    let refreshed_media_item = mova_db::update_media_item_metadata(
        pool,
        media_item_id,
        mova_db::UpdateMediaItemMetadataParams {
            title: discovered_file.title,
            source_title: discovered_file.source_title,
            original_title: discovered_file.original_title,
            sort_title: discovered_file.sort_title,
            metadata_provider: media_item.metadata_provider.clone(),
            metadata_provider_item_id: media_item.metadata_provider_item_id,
            year: discovered_file.year,
            overview: discovered_file.overview,
            poster_path: discovered_file.poster_path,
            backdrop_path: discovered_file.backdrop_path,
        },
    )
    .await
    .map_err(ApplicationError::from)?
    .ok_or_else(|| {
        ApplicationError::NotFound(format!("media item not found: {}", media_item_id))
    })?;

    if media_item.media_type.eq_ignore_ascii_case("series") {
        invalidate_series_episode_outline_cache(pool, media_item_id).await?;
    }
    invalidate_media_item_cast_cache(pool, media_item_id).await?;

    Ok(refreshed_media_item)
}

async fn inspect_media_file_path(path: &str) -> io::Result<mova_scan::DiscoveredMediaFile> {
    let path_string = path.to_string();
    let join_path = path_string.clone();
    tokio::task::spawn_blocking(move || mova_scan::inspect_media_file(Path::new(&path_string)))
        .await
        .map_err(|error| {
            io::Error::other(format!(
                "metadata refresh worker failed to join for {}: {}",
                join_path, error
            ))
        })?
}

fn map_refresh_source_error(
    media_item_id: i64,
    file_path: &str,
    error: std::io::Error,
) -> ApplicationError {
    match error.kind() {
        ErrorKind::NotFound => ApplicationError::Conflict(format!(
            "source media file for media item {} does not exist: {}. If it was renamed, moved, or deleted, rescan the library instead",
            media_item_id, file_path
        )),
        ErrorKind::InvalidInput => ApplicationError::Validation(format!(
            "source media file for media item {} is invalid: {}",
            media_item_id, file_path
        )),
        _ => ApplicationError::Unexpected(anyhow::anyhow!(
            "failed to inspect source media file {} for media item {}: {}",
            file_path,
            media_item_id,
            error
        )),
    }
}

fn normalize_query(query: Option<String>) -> Option<String> {
    query.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_year(year: Option<i32>) -> ApplicationResult<Option<i32>> {
    match year {
        Some(value) if value <= 0 => Err(ApplicationError::Validation(
            "year filter must be a positive integer".to_string(),
        )),
        other => Ok(other),
    }
}

fn normalize_page(page: Option<i64>) -> ApplicationResult<i64> {
    match page.unwrap_or(DEFAULT_MEDIA_ITEMS_PAGE) {
        value if value <= 0 => Err(ApplicationError::Validation(
            "page must be a positive integer".to_string(),
        )),
        value => Ok(value),
    }
}

fn normalize_page_size(page_size: Option<i64>) -> ApplicationResult<i64> {
    match page_size.unwrap_or(DEFAULT_MEDIA_ITEMS_PAGE_SIZE) {
        value if value <= 0 => Err(ApplicationError::Validation(
            "page_size must be a positive integer".to_string(),
        )),
        value if value > MAX_MEDIA_ITEMS_PAGE_SIZE => Ok(MAX_MEDIA_ITEMS_PAGE_SIZE),
        value => Ok(value),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        merge_remote_outline_with_local, normalize_page, normalize_page_size, normalize_query,
        normalize_year, LocalSeriesEpisode, LocalSeriesSeason,
    };
    use crate::ApplicationError;
    use crate::{RemoteSeriesEpisode, RemoteSeriesEpisodeOutline, RemoteSeriesSeason};
    use std::collections::{BTreeMap, HashMap};

    #[test]
    fn normalize_query_discards_blank_strings() {
        assert_eq!(normalize_query(Some("   ".to_string())), None);
        assert_eq!(
            normalize_query(Some(" dragon ".to_string())),
            Some("dragon".to_string())
        );
    }

    #[test]
    fn normalize_year_rejects_non_positive_values() {
        assert!(matches!(
            normalize_year(Some(0)),
            Err(ApplicationError::Validation(message))
                if message.contains("positive integer")
        ));
    }

    #[test]
    fn normalize_page_rejects_non_positive_values() {
        assert!(matches!(
            normalize_page(Some(0)),
            Err(ApplicationError::Validation(message))
                if message.contains("positive integer")
        ));
    }

    #[test]
    fn normalize_page_size_caps_large_values() {
        assert_eq!(normalize_page_size(Some(500)).unwrap(), 100);
    }

    #[test]
    fn merge_remote_outline_marks_missing_episodes_as_unavailable() {
        let mut season_1_episodes = BTreeMap::new();
        season_1_episodes.insert(
            1,
            LocalSeriesEpisode {
                media_item_id: 11,
                title: "Local Episode 1".to_string(),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
        );
        season_1_episodes.insert(
            3,
            LocalSeriesEpisode {
                media_item_id: 13,
                title: "Local Episode 3".to_string(),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
        );
        let mut local_inventory = BTreeMap::new();
        local_inventory.insert(
            1,
            LocalSeriesSeason {
                season_id: 101,
                title: Some("Season One".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
                episodes: season_1_episodes,
            },
        );

        let merged = merge_remote_outline_with_local(
            RemoteSeriesEpisodeOutline {
                seasons: vec![RemoteSeriesSeason {
                    season_number: 1,
                    title: Some("Season 1".to_string()),
                    year: Some(2021),
                    overview: None,
                    poster_path: None,
                    backdrop_path: None,
                    episodes: vec![
                        RemoteSeriesEpisode {
                            episode_number: 1,
                            title: Some("Pilot".to_string()),
                            overview: None,
                            poster_path: None,
                            backdrop_path: None,
                        },
                        RemoteSeriesEpisode {
                            episode_number: 2,
                            title: Some("Second".to_string()),
                            overview: None,
                            poster_path: None,
                            backdrop_path: None,
                        },
                    ],
                }],
            },
            &local_inventory,
            &HashMap::new(),
        );

        assert_eq!(merged.seasons.len(), 1);
        let episodes = &merged.seasons[0].episodes;
        assert_eq!(episodes.len(), 3);
        assert_eq!(episodes[0].episode_number, 1);
        assert_eq!(episodes[0].title, "Pilot");
        assert_eq!(episodes[0].media_item_id, Some(11));
        assert!(episodes[0].is_available);

        assert_eq!(episodes[1].episode_number, 2);
        assert_eq!(episodes[1].title, "Second");
        assert_eq!(episodes[1].media_item_id, None);
        assert!(!episodes[1].is_available);

        assert_eq!(episodes[2].episode_number, 3);
        assert_eq!(episodes[2].title, "Local Episode 3");
        assert_eq!(episodes[2].media_item_id, Some(13));
        assert!(episodes[2].is_available);
        assert_eq!(merged.seasons[0].year, Some(2021));
    }

    #[test]
    fn merge_remote_outline_prefers_local_artwork_for_available_entries() {
        let mut season_1_episodes = BTreeMap::new();
        season_1_episodes.insert(
            1,
            LocalSeriesEpisode {
                media_item_id: 11,
                title: "Local Episode 1".to_string(),
                overview: None,
                poster_path: Some("/cache/local-episode-poster.jpg".to_string()),
                backdrop_path: Some("/cache/local-episode-backdrop.jpg".to_string()),
            },
        );
        let mut local_inventory = BTreeMap::new();
        local_inventory.insert(
            1,
            LocalSeriesSeason {
                season_id: 101,
                title: Some("Season One".to_string()),
                overview: None,
                poster_path: Some("/cache/local-season-poster.jpg".to_string()),
                backdrop_path: Some("/cache/local-season-backdrop.jpg".to_string()),
                episodes: season_1_episodes,
            },
        );

        let merged = merge_remote_outline_with_local(
            RemoteSeriesEpisodeOutline {
                seasons: vec![RemoteSeriesSeason {
                    season_number: 1,
                    title: Some("Season 1".to_string()),
                    year: Some(2021),
                    overview: None,
                    poster_path: Some(
                        "https://image.tmdb.org/remote-season-poster.jpg".to_string(),
                    ),
                    backdrop_path: Some(
                        "https://image.tmdb.org/remote-season-backdrop.jpg".to_string(),
                    ),
                    episodes: vec![RemoteSeriesEpisode {
                        episode_number: 1,
                        title: Some("Pilot".to_string()),
                        overview: None,
                        poster_path: Some(
                            "https://image.tmdb.org/remote-episode-poster.jpg".to_string(),
                        ),
                        backdrop_path: Some(
                            "https://image.tmdb.org/remote-episode-backdrop.jpg".to_string(),
                        ),
                    }],
                }],
            },
            &local_inventory,
            &HashMap::new(),
        );

        assert_eq!(
            merged.seasons[0].poster_path.as_deref(),
            Some("/cache/local-season-poster.jpg")
        );
        assert_eq!(
            merged.seasons[0].backdrop_path.as_deref(),
            Some("/cache/local-season-backdrop.jpg")
        );
        assert_eq!(
            merged.seasons[0].episodes[0].poster_path.as_deref(),
            Some("/cache/local-episode-poster.jpg")
        );
        assert_eq!(
            merged.seasons[0].episodes[0].backdrop_path.as_deref(),
            Some("/cache/local-episode-backdrop.jpg")
        );
    }

    #[test]
    fn merge_remote_outline_ignores_remote_only_seasons() {
        let mut season_1_episodes = BTreeMap::new();
        season_1_episodes.insert(
            1,
            LocalSeriesEpisode {
                media_item_id: 11,
                title: "Local Episode 1".to_string(),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
        );

        let mut local_inventory = BTreeMap::new();
        local_inventory.insert(
            1,
            LocalSeriesSeason {
                season_id: 101,
                title: Some("Season One".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
                episodes: season_1_episodes,
            },
        );

        let merged = merge_remote_outline_with_local(
            RemoteSeriesEpisodeOutline {
                seasons: vec![
                    RemoteSeriesSeason {
                        season_number: 1,
                        title: Some("Season 1".to_string()),
                        year: Some(2021),
                        overview: None,
                        poster_path: None,
                        backdrop_path: None,
                        episodes: vec![RemoteSeriesEpisode {
                            episode_number: 1,
                            title: Some("Pilot".to_string()),
                            overview: None,
                            poster_path: None,
                            backdrop_path: None,
                        }],
                    },
                    RemoteSeriesSeason {
                        season_number: 2,
                        title: Some("Season 2".to_string()),
                        year: Some(2024),
                        overview: None,
                        poster_path: None,
                        backdrop_path: None,
                        episodes: vec![RemoteSeriesEpisode {
                            episode_number: 1,
                            title: Some("S2E1".to_string()),
                            overview: None,
                            poster_path: None,
                            backdrop_path: None,
                        }],
                    },
                ],
            },
            &local_inventory,
            &HashMap::new(),
        );

        assert_eq!(merged.seasons.len(), 1);
        assert_eq!(merged.seasons[0].season_number, 1);
    }
}
