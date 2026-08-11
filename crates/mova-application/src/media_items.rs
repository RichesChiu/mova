use crate::{
    ensure_media_item_cast,
    error::{ApplicationError, ApplicationResult},
    libraries::get_library,
    media_cast::validated_tmdb_binding,
    media_classification::{apply_root_aware_media_identity, metadata_lookup_type_for_media_type},
    media_enrichment::MetadataEnrichmentContext,
    metadata::{MetadataLookup, MetadataProvider, RemoteSeriesEpisodeOutline, TMDB_PROVIDER_NAME},
};
use mova_domain::{
    AudioTrack, Library, MediaExternalIdRecord, MediaFile, MediaItem, MediaItemCredit,
    MediaLocalMetadataSource, MediaLocalMetadataSourceSummary, MediaRating, MediaSourceKind,
    PlaybackProgress, Season, SubtitleFile, METADATA_FAILURE_NO_REMOTE_MATCH,
    METADATA_FAILURE_PROVIDER_DISABLED, METADATA_STATUS_MATCHED, METADATA_STATUS_SKIPPED,
    METADATA_STATUS_UNMATCHED, REMOTE_MEDIA_TYPE_MOVIE, REMOTE_MEDIA_TYPE_SERIES,
};
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
const DEFAULT_RECENTLY_ADDED_ITEM_LIMIT: i64 = 8;
const MAX_RECENTLY_ADDED_ITEM_LIMIT: i64 = 50;
const MAX_RECENTLY_ADDED_DAYS: i64 = 365;
const DEFAULT_GLOBAL_SEARCH_LIMIT: i64 = 12;
const MAX_GLOBAL_SEARCH_LIMIT: i64 = 30;
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

/// Source-aware metadata attached to one media item.
///
/// Local metadata payloads are already normalized by the scan/persistence
/// boundary. Raw NFO XML never crosses into this application contract.
#[derive(Debug, Clone)]
pub struct MediaItemMetadataSources {
    pub external_ids: Vec<MediaExternalIdRecord>,
    pub credits: Vec<MediaItemCredit>,
    pub local_metadata_sources: Vec<MediaLocalMetadataSourceSummary>,
}

#[derive(Debug, Clone)]
pub struct LocalMetadataSourceInspection {
    pub source: MediaLocalMetadataSource,
    pub observation_status: String,
    pub observation_error_code: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ListRecentlyAddedByLibraryInput {
    pub visible_library_ids: Option<Vec<i64>>,
    pub days: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RecentlyAddedLibraryMediaItems {
    pub library: Library,
    pub items: Vec<MediaItem>,
    pub total: i64,
}

#[derive(Debug, Clone, Default)]
pub struct GlobalSearchInput {
    pub query: Option<String>,
    pub visible_library_ids: Option<Vec<i64>>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct GlobalSearchResult {
    pub kind: String,
    pub library_id: i64,
    pub library_name: String,
    pub media_item_id: i64,
    pub series_media_item_id: Option<i64>,
    pub media_type: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub ratings: Vec<MediaRating>,
    pub updated_at: OffsetDateTime,
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
    pub intro_start_seconds: Option<i32>,
    pub intro_end_seconds: Option<i32>,
    pub episodes: Vec<SeriesEpisodeOutlineEpisode>,
}

#[derive(Debug, Clone)]
pub struct SeriesEpisodeOutlineEpisode {
    pub episode_number: i32,
    pub title: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub intro_start_seconds: Option<i32>,
    pub intro_end_seconds: Option<i32>,
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
    intro_start_seconds: Option<i32>,
    intro_end_seconds: Option<i32>,
    episodes: BTreeMap<i32, LocalSeriesEpisode>,
}

#[derive(Debug, Clone)]
struct LocalSeriesEpisode {
    media_item_id: i64,
    title: String,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    intro_start_seconds: Option<i32>,
    intro_end_seconds: Option<i32>,
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

/// 首页“最新添加”使用的按库聚合数据。
/// 这里按 `media_items.created_at` 倒序取数，不复用库页按标题排序的列表接口。
pub async fn list_recently_added_media_items_by_library(
    pool: &PgPool,
    input: ListRecentlyAddedByLibraryInput,
) -> ApplicationResult<Vec<RecentlyAddedLibraryMediaItems>> {
    let item_limit = normalize_recently_added_limit(input.limit)?;
    let created_since = normalize_recently_added_days(input.days)?
        .map(|days| OffsetDateTime::now_utc() - Duration::days(days));
    let visible_library_ids = input.visible_library_ids;

    if visible_library_ids
        .as_ref()
        .map(|ids| ids.is_empty())
        .unwrap_or(false)
    {
        return Ok(Vec::new());
    }

    let groups = mova_db::list_recently_added_media_items_by_library(
        pool,
        visible_library_ids.as_deref(),
        item_limit,
        created_since,
    )
    .await
    .map_err(ApplicationError::from)?;

    Ok(groups
        .into_iter()
        .map(|group| RecentlyAddedLibraryMediaItems {
            library: group.library,
            items: group.items,
            total: group.total,
        })
        .collect())
}

/// 搜索当前用户可见库下的电影、剧集和本地集条目。
pub async fn global_search(
    pool: &PgPool,
    input: GlobalSearchInput,
) -> ApplicationResult<Vec<GlobalSearchResult>> {
    let query = match normalize_query(input.query) {
        Some(query) => query,
        None => return Ok(Vec::new()),
    };
    let limit = normalize_global_search_limit(input.limit)?;
    let visible_library_ids = input.visible_library_ids;

    if visible_library_ids
        .as_ref()
        .map(|ids| ids.is_empty())
        .unwrap_or(false)
    {
        return Ok(Vec::new());
    }

    let results = mova_db::global_search(
        pool,
        mova_db::GlobalSearchParams {
            query,
            visible_library_ids,
            limit,
        },
    )
    .await
    .map_err(ApplicationError::from)?;

    Ok(results
        .into_iter()
        .map(|result| GlobalSearchResult {
            kind: result.kind,
            library_id: result.library_id,
            library_name: result.library_name,
            media_item_id: result.media_item_id,
            series_media_item_id: result.series_media_item_id,
            media_type: result.media_type,
            title: result.title,
            subtitle: result.subtitle,
            year: result.year,
            overview: result.overview,
            poster_path: result.poster_path,
            backdrop_path: result.backdrop_path,
            season_number: result.season_number,
            episode_number: result.episode_number,
            ratings: result.ratings,
            updated_at: result.updated_at,
        })
        .collect())
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

/// Load source-aware metadata headers after the transport layer has authorized
/// access. The collection deliberately avoids both JSON payload reads and live
/// filesystem inspection; those are deferred to one selected source.
pub async fn get_media_item_metadata_sources(
    pool: &PgPool,
    media_item_id: i64,
) -> ApplicationResult<MediaItemMetadataSources> {
    get_media_item(pool, media_item_id).await?;
    let (external_ids, credits, local_metadata_sources) = tokio::try_join!(
        mova_db::list_media_item_external_ids(pool, media_item_id),
        mova_db::list_media_item_credits(pool, media_item_id),
        mova_db::list_media_local_metadata_source_summaries_for_item(pool, media_item_id),
    )
    .map_err(ApplicationError::from)?;

    Ok(MediaItemMetadataSources {
        external_ids,
        credits,
        local_metadata_sources,
    })
}

/// Load and observe one persisted local metadata source after the transport
/// layer has authorized administrative access to the media item.
pub async fn get_media_item_metadata_source(
    pool: &PgPool,
    media_item_id: i64,
    source_id: i64,
) -> ApplicationResult<LocalMetadataSourceInspection> {
    let media_item = get_media_item(pool, media_item_id).await?;
    let library = get_library(pool, media_item.library_id).await?;
    let source = mova_db::get_media_local_metadata_source_for_item(pool, media_item_id, source_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| {
            ApplicationError::NotFound(format!(
                "local metadata source not found for media item {media_item_id}: {source_id}"
            ))
        })?;

    inspect_local_metadata_source(source, PathBuf::from(library.root_path)).await
}

async fn inspect_local_metadata_source(
    source: MediaLocalMetadataSource,
    library_root: PathBuf,
) -> ApplicationResult<LocalMetadataSourceInspection> {
    let path = source.source_path.clone();
    let document_type = source.document_type.clone();
    let observation = tokio::task::spawn_blocking(move || {
        let expected_kind = match document_type.as_str() {
            "movie" => Some(mova_scan::LocalNfoKind::Movie),
            "tvshow" => Some(mova_scan::LocalNfoKind::TvShow),
            "episodedetails" => Some(mova_scan::LocalNfoKind::Episode),
            _ => None,
        };
        expected_kind.map(|kind| {
            mova_scan::observe_nfo_file_within_root(Path::new(&path), kind, &library_root)
        })
    })
    .await
    .map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "local metadata inspection worker failed: {error}"
        ))
    })?;

    let (observation_status, observation_error_code) = match observation {
        Some(mova_scan::LocalNfoObservation::Valid(_)) => ("valid", None),
        Some(mova_scan::LocalNfoObservation::Invalid { error_code, .. }) => {
            ("invalid", Some(local_nfo_error_code(error_code)))
        }
        Some(mova_scan::LocalNfoObservation::Absent { .. }) => ("missing", None),
        None => ("invalid", Some("unsupported_document_type".to_string())),
    };

    Ok(LocalMetadataSourceInspection {
        source,
        observation_status: observation_status.to_string(),
        observation_error_code,
    })
}

fn local_nfo_error_code(error: mova_scan::LocalNfoErrorCode) -> String {
    match error {
        mova_scan::LocalNfoErrorCode::OpenFailed => "open_failed",
        mova_scan::LocalNfoErrorCode::InspectFailed => "inspect_failed",
        mova_scan::LocalNfoErrorCode::NotRegularFile => "not_regular_file",
        mova_scan::LocalNfoErrorCode::TooLarge => "too_large",
        mova_scan::LocalNfoErrorCode::ReadFailed => "read_failed",
        mova_scan::LocalNfoErrorCode::GrewBeyondLimit => "grew_beyond_limit",
        mova_scan::LocalNfoErrorCode::InvalidUtf8 => "invalid_utf8",
        mova_scan::LocalNfoErrorCode::ForbiddenXmlDeclaration => "forbidden_xml_declaration",
        mova_scan::LocalNfoErrorCode::MalformedXml => "malformed_xml",
        mova_scan::LocalNfoErrorCode::UnsupportedRoot => "unsupported_root",
        mova_scan::LocalNfoErrorCode::UnexpectedRootKind => "unexpected_root_kind",
        mova_scan::LocalNfoErrorCode::OutsideLibraryRoot => "outside_library_root",
        mova_scan::LocalNfoErrorCode::SymlinkNotAllowed => "symlink_not_allowed",
        mova_scan::LocalNfoErrorCode::SecureOpenUnavailable => "secure_open_unavailable",
        mova_scan::LocalNfoErrorCode::ResourceLimitExceeded => "resource_limit_exceeded",
    }
    .to_string()
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

/// 读取某个媒体文件可切换的内嵌音轨。
/// 当前播放器切换语言音轨时按媒体文件维度查询，避免跨版本文件误复用轨道列表。
pub async fn list_audio_tracks_for_media_file(
    pool: &PgPool,
    media_file_id: i64,
) -> ApplicationResult<Vec<AudioTrack>> {
    get_media_file(pool, media_file_id).await?;

    mova_db::list_audio_tracks_for_media_file(pool, media_file_id)
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

pub async fn get_audio_track(pool: &PgPool, audio_track_id: i64) -> ApplicationResult<AudioTrack> {
    mova_db::get_audio_track(pool, audio_track_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| {
            ApplicationError::NotFound(format!("audio track not found: {}", audio_track_id))
        })
}

pub async fn get_season(pool: &PgPool, season_id: i64) -> ApplicationResult<Season> {
    mova_db::get_season(pool, season_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| ApplicationError::NotFound(format!("season not found: {}", season_id)))
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
    let Some(provider_item_id) = validated_tmdb_binding(&media_item).map(str::to_string) else {
        return Ok(build_local_outline(
            &local_inventory,
            &playback_progress_by_media_item,
        ));
    };
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
        season_air_year: None,
        library_type: "series".to_string(),
        language: Some(library.metadata_language.clone()),
        provider_item_id: Some(provider_item_id),
    };

    let remote_outline = match metadata_provider
        .lookup_series_episode_outline(&lookup)
        .await
    {
        Ok(remote_outline) => {
            if let Some(remote_outline) = remote_outline.as_ref() {
                if !cache_remote_outline(pool, &media_item, remote_outline).await? {
                    return Err(ApplicationError::Conflict(
                        "series metadata changed while its TMDB outline was loading".to_string(),
                    ));
                }
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

pub(crate) async fn cache_remote_outline(
    pool: &PgPool,
    media_item: &MediaItem,
    remote_outline: &RemoteSeriesEpisodeOutline,
) -> ApplicationResult<bool> {
    let Some(provider_item_id) = validated_tmdb_binding(media_item) else {
        return Ok(false);
    };
    let media_item_id = media_item.id;
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
            expected_provider_item_id: provider_item_id.to_string(),
            expected_media_item_updated_at: media_item.updated_at,
            outline_json,
            fetched_at,
            expires_at,
        },
    )
    .await
    {
        Ok(Some(_)) => return Ok(true),
        Ok(None) => return Ok(false),
        Err(error) if is_missing_series_outline_cache_table_error(&error) => {
            tracing::warn!(
                media_item_id,
                "series episode outline cache table does not exist yet, skipping cache write"
            );
        }
        Err(error) => return Err(ApplicationError::from(error)),
    }

    Ok(false)
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
                    intro_start_seconds: episode.intro_start_seconds,
                    intro_end_seconds: episode.intro_end_seconds,
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
                intro_start_seconds: season.intro_start_seconds,
                intro_end_seconds: season.intro_end_seconds,
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
                    intro_start_seconds: episode.intro_start_seconds,
                    intro_end_seconds: episode.intro_end_seconds,
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
                intro_start_seconds: season.intro_start_seconds,
                intro_end_seconds: season.intro_end_seconds,
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
                .and_then(normalize_non_empty)
                .or_else(|| local_episode.map(|episode| episode.title.clone()))
                .unwrap_or_else(|| default_episode_title(episode_number));

            episodes.push(SeriesEpisodeOutlineEpisode {
                episode_number,
                title,
                overview: remote_episode
                    .overview
                    .or_else(|| local_episode.and_then(|episode| episode.overview.clone())),
                poster_path: remote_episode.poster_path,
                backdrop_path: remote_episode.backdrop_path,
                intro_start_seconds: local_episode.and_then(|episode| episode.intro_start_seconds),
                intro_end_seconds: local_episode.and_then(|episode| episode.intro_end_seconds),
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
                intro_start_seconds: local_episode.intro_start_seconds,
                intro_end_seconds: local_episode.intro_end_seconds,
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
                poster_path: remote_season.poster_path,
                backdrop_path: remote_season.backdrop_path,
                intro_start_seconds: local_season.intro_start_seconds,
                intro_end_seconds: local_season.intro_end_seconds,
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
                    intro_start_seconds: local_episode.intro_start_seconds,
                    intro_end_seconds: local_episode.intro_end_seconds,
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
                intro_start_seconds: local_season.intro_start_seconds,
                intro_end_seconds: local_season.intro_end_seconds,
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
    let library = get_library(pool, media_item.library_id).await?;
    let source_files = mova_db::list_media_item_metadata_refresh_source_files(pool, media_item_id)
        .await
        .map_err(ApplicationError::from)?;
    if source_files.is_empty() {
        return Err(ApplicationError::Conflict(format!(
            "media item {} has no source file to refresh from",
            media_item_id
        )));
    }

    let mut library_file_paths = mova_db::list_library_media_file_memberships(pool, library.id)
        .await
        .map_err(ApplicationError::from)?
        .into_iter()
        .map(|membership| membership.file_path)
        .collect::<BTreeSet<_>>();
    library_file_paths.extend(
        source_files
            .iter()
            .map(|source_file| source_file.file_path.clone()),
    );
    let shallow_library_files = library_file_paths
        .into_iter()
        .filter_map(|file_path| {
            mova_scan::inspect_media_file_inventory_shallow(
                mova_scan::DiscoveredMediaFileInventory {
                    source_kind: source_kind_for_carrier_path(Path::new(&file_path)),
                    stream_reference_hash: None,
                    file_path: PathBuf::from(file_path),
                    file_size: 0,
                    file_modified_at_ms: None,
                    sidecar_fingerprint: String::new(),
                },
            )
            .ok()
        })
        .collect::<Vec<_>>();
    let refresh_nfo_observations = crate::scan_jobs::eligible_local_nfo_observations_for_refresh(
        &shallow_library_files,
        Path::new(&library.root_path),
    );
    let mut refresh_nfo_observations_by_path = shallow_library_files
        .iter()
        .map(|file| file.file_path.clone())
        .zip(refresh_nfo_observations)
        .collect::<HashMap<_, _>>();

    let mut discovered_files = Vec::with_capacity(source_files.len());
    let mut invalid_local_nfo_paths = BTreeSet::new();
    for (index, source_file) in source_files.iter().enumerate() {
        let (media_nfo_observation, series_nfo_observation) = refresh_nfo_observations_by_path
            .remove(Path::new(&source_file.file_path))
            .unwrap_or((None, None));
        let allow_generic_movie_nfo = media_nfo_observation
            .as_ref()
            .and_then(local_nfo_observation_source_path)
            .is_some_and(|path| {
                path.file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("movie.nfo"))
            });
        let inspection = if index == 0 {
            inspect_media_file_path(
                &source_file.file_path,
                &library.root_path,
                allow_generic_movie_nfo,
            )
            .await
        } else {
            inspect_media_file_sidecar_only_path(
                &source_file.file_path,
                &library.root_path,
                allow_generic_movie_nfo,
            )
            .await
        };
        let mut discovered_file = inspection.map_err(|error| {
            map_refresh_source_error(media_item_id, &source_file.file_path, error)
        })?;
        apply_explicit_nfo_observation_for_refresh(
            &mut discovered_file,
            media_item.media_type.as_str(),
            media_nfo_observation,
            series_nfo_observation,
            &mut invalid_local_nfo_paths,
        );
        discovered_files.push(discovered_file);
    }

    let lookup_type = metadata_lookup_type_for_media_type(&media_item.media_type);
    let existing_local_sources =
        mova_db::list_media_local_metadata_sources_for_item(pool, media_item_id)
            .await
            .map_err(ApplicationError::from)?;
    let mut root_metadata_lookup_hints = BTreeSet::new();
    for discovered_file in &mut discovered_files {
        if let Some(hint) =
            apply_root_aware_media_identity(discovered_file, Path::new(&library.root_path))
        {
            root_metadata_lookup_hints.insert(hint);
        }
    }
    let source_paths = source_files
        .iter()
        .map(|source_file| source_file.file_path.clone())
        .collect::<Vec<_>>();
    let existing_by_path =
        mova_db::list_existing_media_metadata_for_file_paths(pool, library.id, &source_paths)
            .await
            .map_err(ApplicationError::from)?
            .into_iter()
            .map(|summary| (summary.file_path.clone(), summary))
            .collect::<HashMap<_, _>>();
    let is_episode = media_item.media_type.eq_ignore_ascii_case("episode");
    let media_item_binding_hint = if is_episode {
        let series_bindings = existing_by_path
            .values()
            .filter(|summary| {
                summary
                    .series_metadata_provider
                    .as_deref()
                    .is_some_and(|provider| provider.eq_ignore_ascii_case(TMDB_PROVIDER_NAME))
            })
            .filter_map(|summary| summary.series_metadata_provider_item_id.clone())
            .collect::<BTreeSet<_>>();
        (series_bindings.len() == 1)
            .then(|| series_bindings.first().cloned())
            .flatten()
    } else {
        accepted_tmdb_lookup_hint_for_identity(
            media_item.metadata_status.as_str(),
            media_item.metadata_provider.as_deref(),
            media_item.metadata_provider_item_id.as_deref(),
        )
    };
    for discovered_file in &mut discovered_files {
        if is_episode {
            if let Some(series_provider_item_id) = media_item_binding_hint.as_ref() {
                discovered_file.metadata_provider = Some(TMDB_PROVIDER_NAME.to_string());
                discovered_file.metadata_provider_item_id = Some(series_provider_item_id.clone());
            }
        } else {
            seed_accepted_tmdb_binding(discovered_file, &media_item);
        }
        if let Some(existing_metadata) =
            existing_by_path.get(discovered_file.file_path.to_string_lossy().as_ref())
        {
            crate::scan_jobs::apply_existing_media_metadata_for_refresh(
                discovered_file,
                existing_metadata,
            );
        }
        if media_item_binding_hint.is_some() {
            if is_episode {
                discovered_file.metadata_provider = Some(TMDB_PROVIDER_NAME.to_string());
                discovered_file
                    .metadata_provider_item_id
                    .clone_from(&media_item_binding_hint);
            } else {
                seed_accepted_tmdb_binding(discovered_file, &media_item);
            }
        }
    }
    let accepted_metadata_lookup_hint = media_item_binding_hint
        .or_else(|| unique_accepted_tmdb_lookup_hint(discovered_files.as_slice()));
    let local_selection = crate::local_metadata::apply_group_local_metadata(
        discovered_files.as_mut_slice(),
        lookup_type,
    );
    let metadata_lookup_hint = accepted_metadata_lookup_hint.or_else(|| {
        let root_hint = (root_metadata_lookup_hints.len() == 1)
            .then(|| root_metadata_lookup_hints.first().cloned())
            .flatten();
        if root_metadata_lookup_hints.len() > 1 {
            None
        } else {
            crate::local_metadata::merge_lookup_hints(root_hint, &local_selection)
        }
    });
    let metadata_provider_for_cast = metadata_provider.clone();
    let metadata_provider_enabled = metadata_provider.is_enabled();
    let mut enrichment = MetadataEnrichmentContext::new(
        artwork_cache_dir.clone(),
        library.id,
        metadata_provider,
        library.metadata_language,
    );
    // Remote enrichment may mutate fields before a later request fails (for
    // example, series details can succeed before the complete outline times
    // out). Preserve the fully inspected local/NFO state and restore it with
    // the same trusted-binding policy used by a full scan.
    let mut files_before_enrichment = discovered_files.clone();
    for file in &mut files_before_enrichment {
        enrichment.sanitize_file_artwork_sources(file);
    }
    let artwork_publication = mova_db::TmdbArtworkPublicationGuard::acquire(pool, library.id)
        .await
        .map_err(ApplicationError::from)?;
    let enrichment_result = enrichment
        .refresh_group_with_lookup_hint_and_progress(
            lookup_type,
            discovered_files.as_mut_slice(),
            None,
            metadata_lookup_hint.as_deref(),
            |_, _| {},
        )
        .await;
    let materialized_artwork = crate::tmdb_revalidation::materialized_tmdb_artwork_paths_from_files(
        discovered_files.as_slice(),
        &artwork_cache_dir,
        library.id,
    );
    let enrichment_outcome = match enrichment_result {
        Ok(outcome) => Some(outcome),
        Err(error) => {
            tracing::warn!(
                media_item_id,
                library_id = library.id,
                title = %media_item.title,
                media_type = %media_item.media_type,
                error = ?error,
                "metadata enrichment failed during manual refresh; committing local changes with the trusted remote fallback"
            );
            crate::scan_jobs::restore_group_after_provider_error(
                &mut discovered_files,
                files_before_enrichment,
                remote_media_type_for_lookup_type(lookup_type),
            );
            None
        }
    };
    let remote_metadata_applied = enrichment_outcome
        .as_ref()
        .is_some_and(|outcome| outcome.remote_metadata_applied);

    let refresh_result: ApplicationResult<MediaItem> = async {
        if enrichment_outcome.is_some() {
            for discovered_file in &mut discovered_files {
                finalize_refreshed_file_metadata_status(
                    discovered_file,
                    metadata_provider_enabled,
                    remote_media_type_for_lookup_type(lookup_type),
                );
            }
        }
        crate::local_metadata::apply_group_local_metadata(
            discovered_files.as_mut_slice(),
            lookup_type,
        );
        let metadata_status = discovered_files[0]
            .metadata_status
            .clone()
            .unwrap_or_else(|| media_item.metadata_status.clone());
        let replace_remote_data = should_replace_remote_data(remote_metadata_applied, is_episode);

        let source_file = &source_files[0];
        let representative_file = &discovered_files[0];
        let file_size = i64::try_from(representative_file.file_size).map_err(|_| {
            ApplicationError::Unexpected(anyhow::anyhow!(
                "file is too large to store in database: {}",
                source_file.file_path
            ))
        })?;
        mova_db::update_media_file_metadata(
            pool,
            source_file.id,
            mova_db::UpdateMediaFileMetadataParams {
                file_path: source_file.file_path.clone(),
                source_kind: representative_file.source_kind,
                stream_reference_hash: representative_file.stream_reference_hash.clone(),
                container: representative_file.container.clone(),
                file_size,
                duration_seconds: representative_file.duration_seconds,
                video_title: representative_file.video_title.clone(),
                video_codec: representative_file.video_codec.clone(),
                video_profile: representative_file.video_profile.clone(),
                video_level: representative_file.video_level.clone(),
                audio_codec: representative_file.audio_codec.clone(),
                width: representative_file.width,
                height: representative_file.height,
                bitrate: representative_file.bitrate,
                video_bitrate: representative_file.video_bitrate,
                video_frame_rate: representative_file.video_frame_rate,
                video_aspect_ratio: representative_file.video_aspect_ratio.clone(),
                video_scan_type: representative_file.video_scan_type.clone(),
                video_color_primaries: representative_file.video_color_primaries.clone(),
                video_color_space: representative_file.video_color_space.clone(),
                video_color_transfer: representative_file.video_color_transfer.clone(),
                video_bit_depth: representative_file.video_bit_depth,
                video_pixel_format: representative_file.video_pixel_format.clone(),
                video_reference_frames: representative_file.video_reference_frames,
                technical_tags: representative_file.technical_tags.clone(),
            },
        )
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| {
            ApplicationError::NotFound(format!("media file not found: {}", source_file.id))
        })?;

        let local_nfos = local_metadata_snapshots_for_refresh(
            media_item.media_type.as_str(),
            discovered_files.as_slice(),
        );
        let current_source_paths = local_nfos
            .iter()
            .map(|snapshot| snapshot.source_path.clone())
            .collect::<BTreeSet<_>>();
        let removed_local_nfo_source_paths = existing_local_sources
            .iter()
            .filter(|source| {
                !current_source_paths
                    .iter()
                    .any(|current| local_metadata_source_paths_equal(&source.source_path, current))
                    && !invalid_local_nfo_paths.iter().any(|invalid| {
                        local_metadata_source_paths_equal(&source.source_path, invalid)
                    })
            })
            .map(|source| source.source_path.clone())
            .collect::<Vec<_>>();
        let is_series = media_item.media_type.eq_ignore_ascii_case("series");
        let refreshed_seasons = if is_series {
            discovered_files
                .iter()
                .filter_map(|file| {
                    let season_number = file.season_number?;
                    Some((
                        season_number,
                        mova_db::UpdateSeasonMetadataParams {
                            season_number,
                            title: file
                                .season_title
                                .clone()
                                .unwrap_or_else(|| format!("Season {season_number:02}")),
                            overview: file.season_overview.clone(),
                            poster_path: file.season_poster_path.clone(),
                            backdrop_path: file.season_backdrop_path.clone(),
                        },
                    ))
                })
                .fold(BTreeMap::new(), |mut seasons, (season_number, season)| {
                    seasons.entry(season_number).or_insert(season);
                    seasons
                })
                .into_values()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let discovered_file = discovered_files
            .into_iter()
            .next()
            .expect("metadata refresh requires at least one discovered file");

        let refreshed_title = if is_episode {
            discovered_file
                .episode_title
                .clone()
                .unwrap_or_else(|| media_item.title.clone())
        } else {
            discovered_file.title.clone()
        };
        let refreshed_source_title = if is_episode {
            discovered_file
                .episode_title
                .clone()
                .unwrap_or_else(|| media_item.source_title.clone())
        } else {
            discovered_file.source_title.clone()
        };
        let refreshed_original_title = if is_episode {
            discovered_file.episode_original_title.clone()
        } else {
            discovered_file.original_title.clone()
        };
        let refreshed_sort_title = if is_episode {
            discovered_file.episode_sort_title.clone()
        } else {
            discovered_file.sort_title.clone()
        };
        let refreshed_year = if is_episode {
            discovered_file.episode_year
        } else {
            discovered_file.year
        };
        let refreshed_tagline = if is_episode {
            discovered_file.episode_tagline.clone()
        } else {
            discovered_file.tagline.clone()
        };
        let refreshed_premiere_date = crate::local_metadata::parse_nfo_date(if is_episode {
            discovered_file.episode_premiere_date.as_deref()
        } else {
            discovered_file.premiere_date.as_deref()
        });
        let refreshed_content_rating = if is_episode {
            discovered_file.episode_content_rating.clone()
        } else {
            discovered_file.content_rating.clone()
        };
        let refreshed_overview = if is_episode {
            discovered_file.episode_overview.clone()
        } else {
            discovered_file.overview.clone()
        };
        let refreshed_metadata_provider = if is_episode {
            media_item.metadata_provider.clone()
        } else {
            discovered_file
                .metadata_provider
                .clone()
                .or_else(|| media_item.metadata_provider.clone())
        };
        let refreshed_metadata_provider_item_id = if is_episode {
            media_item.metadata_provider_item_id.clone()
        } else {
            discovered_file
                .metadata_provider_item_id
                .clone()
                .or_else(|| media_item.metadata_provider_item_id.clone())
        };
        let refreshed_poster_path = if is_series {
            discovered_file.series_poster_path.clone()
        } else {
            discovered_file.poster_path.clone()
        };
        let refreshed_backdrop_path = if is_series {
            discovered_file.series_backdrop_path.clone()
        } else {
            discovered_file.backdrop_path.clone()
        };
        let refreshed_logo_path = if is_series {
            discovered_file.series_logo_path.clone()
        } else {
            discovered_file.logo_path.clone()
        };
        let refreshed_metadata_failure_reason = resolve_refreshed_metadata_failure_reason(
            &metadata_status,
            discovered_file.metadata_failure_reason,
            media_item.metadata_failure_reason.clone(),
        );
        let outcome = mova_db::update_media_item_metadata(
            pool,
            media_item_id,
            mova_db::UpdateMediaItemMetadataParams {
                expected_updated_at: media_item.updated_at,
                title: refreshed_title,
                source_title: refreshed_source_title,
                original_title: refreshed_original_title,
                sort_title: refreshed_sort_title,
                metadata_provider: refreshed_metadata_provider,
                metadata_provider_item_id: refreshed_metadata_provider_item_id,
                metadata_status,
                metadata_failure_reason: refreshed_metadata_failure_reason,
                replace_remote_data,
                tmdb_remote_snapshot_json: replace_remote_data
                    .then(|| {
                        enrichment_outcome
                            .as_ref()
                            .and_then(|outcome| outcome.tmdb_remote_snapshot_json.clone())
                    })
                    .flatten(),
                tmdb_remote_snapshot_renews_retention: replace_remote_data
                    && enrichment_outcome
                        .as_ref()
                        .is_some_and(|outcome| outcome.tmdb_remote_snapshot_renews_retention),
                remote_media_type: discovered_file
                    .remote_media_type
                    .or(media_item.remote_media_type),
                year: refreshed_year,
                tagline: refreshed_tagline,
                premiere_date: refreshed_premiere_date,
                content_rating: refreshed_content_rating,
                seasons: refreshed_seasons,
                local_nfos,
                removed_local_nfo_source_paths,
                external_ids: if is_episode {
                    Vec::new()
                } else {
                    discovered_file.external_ids
                },
                ratings: if is_episode {
                    Vec::new()
                } else {
                    discovered_file.ratings
                },
                country: if is_episode {
                    media_item.country.clone()
                } else {
                    discovered_file.country
                },
                genres: if is_episode {
                    media_item.genres.clone()
                } else {
                    discovered_file.genres
                },
                studio: if is_episode {
                    media_item.studio.clone()
                } else {
                    discovered_file.studio
                },
                overview: refreshed_overview,
                poster_path: refreshed_poster_path,
                backdrop_path: refreshed_backdrop_path,
                logo_path: refreshed_logo_path,
            },
        )
        .await
        .map_err(ApplicationError::from)?;
        resolve_manual_metadata_update_outcome(outcome, media_item_id)
    }
    .await;
    let refreshed_media_item = crate::tmdb_revalidation::finish_tmdb_artwork_publication(
        artwork_publication,
        refresh_result,
        pool,
        &artwork_cache_dir,
        library.id,
        materialized_artwork,
        remote_metadata_applied,
    )
    .await?;

    ensure_media_item_cast(pool, &refreshed_media_item, metadata_provider_for_cast).await?;

    Ok(refreshed_media_item)
}

pub(crate) fn resolve_manual_metadata_update_outcome(
    outcome: mova_db::UpdateMediaItemMetadataOutcome,
    media_item_id: i64,
) -> ApplicationResult<MediaItem> {
    match outcome {
        mova_db::UpdateMediaItemMetadataOutcome::Updated(media_item) => Ok(*media_item),
        mova_db::UpdateMediaItemMetadataOutcome::Missing => Err(ApplicationError::NotFound(
            format!("media item not found: {media_item_id}"),
        )),
        mova_db::UpdateMediaItemMetadataOutcome::Stale => Err(ApplicationError::Conflict(format!(
            "media item {media_item_id} changed while metadata was being prepared"
        ))),
        mova_db::UpdateMediaItemMetadataOutcome::ActiveScan(scan_job) => {
            Err(ApplicationError::Conflict(format!(
                "library {} is being scanned by job {}",
                scan_job.library_id, scan_job.id
            )))
        }
    }
}

fn source_kind_for_carrier_path(path: &Path) -> MediaSourceKind {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("strm"))
    {
        MediaSourceKind::Strm
    } else {
        MediaSourceKind::LocalFile
    }
}

async fn inspect_media_file_path(
    path: &str,
    library_root: &str,
    allow_generic_movie_nfo: bool,
) -> io::Result<mova_scan::DiscoveredMediaFile> {
    let path_string = path.to_string();
    let root_path = PathBuf::from(library_root);
    let join_path = path_string.clone();
    tokio::task::spawn_blocking(move || {
        mova_scan::inspect_media_file_within_root_and_nfo_policy(
            Path::new(&path_string),
            &root_path,
            allow_generic_movie_nfo,
        )
    })
    .await
    .map_err(|error| {
        io::Error::other(format!(
            "metadata refresh worker failed to join for {}: {}",
            join_path, error
        ))
    })?
}

async fn inspect_media_file_sidecar_only_path(
    path: &str,
    library_root: &str,
    allow_generic_movie_nfo: bool,
) -> io::Result<mova_scan::DiscoveredMediaFile> {
    let path_string = path.to_string();
    let root_path = PathBuf::from(library_root);
    let join_path = path_string.clone();
    tokio::task::spawn_blocking(move || {
        mova_scan::inspect_media_file_sidecar_only_within_root_and_nfo_policy(
            Path::new(&path_string),
            &root_path,
            allow_generic_movie_nfo,
        )
    })
    .await
    .map_err(|error| {
        io::Error::other(format!(
            "sidecar metadata refresh worker failed to join for {}: {}",
            join_path, error
        ))
    })?
}

fn apply_explicit_nfo_observation_for_refresh(
    file: &mut mova_scan::DiscoveredMediaFile,
    media_type: &str,
    media_observation: Option<mova_scan::LocalNfoObservation>,
    series_observation: Option<mova_scan::LocalNfoObservation>,
    invalid_paths: &mut BTreeSet<String>,
) {
    let observation = if media_type.eq_ignore_ascii_case("series") {
        series_observation
    } else {
        media_observation
    };

    match observation {
        Some(mova_scan::LocalNfoObservation::Valid(metadata)) => {
            if media_type.eq_ignore_ascii_case("series") {
                file.series_local_nfo = Some(*metadata);
                file.invalid_series_local_nfo_source_path = None;
            } else {
                file.local_nfo = Some(*metadata);
                file.invalid_local_nfo_source_path = None;
            }
        }
        Some(mova_scan::LocalNfoObservation::Invalid { candidate_path, .. }) => {
            if media_type.eq_ignore_ascii_case("series") {
                file.series_local_nfo = None;
                file.invalid_series_local_nfo_source_path = Some(candidate_path.clone());
            } else {
                file.local_nfo = None;
                file.invalid_local_nfo_source_path = Some(candidate_path.clone());
            }
            invalid_paths.insert(candidate_path.to_string_lossy().to_string());
        }
        Some(mova_scan::LocalNfoObservation::Absent { .. }) | None => {
            if media_type.eq_ignore_ascii_case("series") {
                file.series_local_nfo = None;
                file.invalid_series_local_nfo_source_path = None;
            } else {
                file.local_nfo = None;
                file.invalid_local_nfo_source_path = None;
            }
        }
    }
}

fn local_nfo_observation_source_path(
    observation: &mova_scan::LocalNfoObservation,
) -> Option<&Path> {
    match observation {
        mova_scan::LocalNfoObservation::Valid(metadata) => Some(metadata.source_path.as_path()),
        mova_scan::LocalNfoObservation::Invalid { candidate_path, .. } => {
            Some(candidate_path.as_path())
        }
        mova_scan::LocalNfoObservation::Absent { .. } => None,
    }
}

fn local_metadata_source_paths_equal(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let Ok(left) = std::fs::canonicalize(left) else {
        return false;
    };
    let Ok(right) = std::fs::canonicalize(right) else {
        return false;
    };
    left == right
}

fn finalize_refreshed_file_metadata_status(
    file: &mut mova_scan::DiscoveredMediaFile,
    metadata_provider_enabled: bool,
    remote_media_type: Option<&'static str>,
) {
    if !metadata_provider_enabled {
        if file.metadata_provider_item_id.is_some() {
            file.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
            // A disabled provider did not observe a new failure. Keep the
            // accepted binding and its persisted remote rows, but do not carry
            // an unrelated historical failure into this refresh result.
            file.metadata_failure_reason = None;
            return;
        }

        file.remote_media_type = None;
        file.metadata_status = Some(METADATA_STATUS_SKIPPED.to_string());
        file.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_DISABLED.to_string());
        return;
    }

    if file.metadata_provider_item_id.is_some() {
        file.remote_media_type = remote_media_type.map(str::to_string);
        file.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
        file.metadata_failure_reason = None;
        return;
    }

    file.remote_media_type = None;
    file.metadata_status = Some(METADATA_STATUS_UNMATCHED.to_string());
    file.metadata_failure_reason = Some(METADATA_FAILURE_NO_REMOTE_MATCH.to_string());
}

fn resolve_refreshed_metadata_failure_reason(
    metadata_status: &str,
    observed_failure_reason: Option<String>,
    previous_failure_reason: Option<String>,
) -> Option<String> {
    if observed_failure_reason.is_some() {
        return observed_failure_reason;
    }

    if metadata_status.eq_ignore_ascii_case(METADATA_STATUS_MATCHED) {
        return None;
    }

    previous_failure_reason
}

fn should_replace_remote_data(remote_metadata_applied: bool, is_episode: bool) -> bool {
    remote_metadata_applied && !is_episode
}

fn seed_accepted_tmdb_binding(
    file: &mut mova_scan::DiscoveredMediaFile,
    media_item: &MediaItem,
) -> Option<String> {
    let provider_item_id = accepted_tmdb_lookup_hint_for_identity(
        media_item.metadata_status.as_str(),
        media_item.metadata_provider.as_deref(),
        media_item.metadata_provider_item_id.as_deref(),
    )?;
    file.metadata_provider = Some(TMDB_PROVIDER_NAME.to_string());
    file.metadata_provider_item_id = Some(provider_item_id.clone());
    file.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
    if file.remote_media_type.is_none() {
        file.remote_media_type
            .clone_from(&media_item.remote_media_type);
    }
    Some(provider_item_id)
}

fn accepted_tmdb_lookup_hint_for_file(file: &mova_scan::DiscoveredMediaFile) -> Option<String> {
    accepted_tmdb_lookup_hint_for_identity(
        file.metadata_status.as_deref().unwrap_or_default(),
        file.metadata_provider.as_deref(),
        file.metadata_provider_item_id.as_deref(),
    )
}

fn unique_accepted_tmdb_lookup_hint(files: &[mova_scan::DiscoveredMediaFile]) -> Option<String> {
    let hints = files
        .iter()
        .filter_map(accepted_tmdb_lookup_hint_for_file)
        .collect::<BTreeSet<_>>();
    (hints.len() == 1)
        .then(|| hints.into_iter().next())
        .flatten()
}

fn local_metadata_snapshots_for_refresh(
    media_type: &str,
    files: &[mova_scan::DiscoveredMediaFile],
) -> Vec<mova_db::CreateLocalMetadataSnapshotParams> {
    let is_series = media_type.eq_ignore_ascii_case("series");
    let mut snapshots = BTreeMap::new();

    for file in files {
        let (metadata, is_selected) = if is_series {
            (
                file.series_local_nfo.as_ref(),
                file.series_local_nfo_is_selected,
            )
        } else {
            (file.local_nfo.as_ref(), file.local_nfo_is_selected)
        };
        let Some(snapshot) = metadata.and_then(|metadata| {
            crate::local_metadata::build_local_metadata_snapshot_for_file(
                metadata,
                is_selected,
                file,
            )
        }) else {
            continue;
        };

        match snapshots.entry(snapshot.source_path.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(snapshot);
            }
            std::collections::btree_map::Entry::Occupied(mut entry)
                if snapshot.is_selected && !entry.get().is_selected =>
            {
                entry.insert(snapshot);
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }
    }

    snapshots.into_values().collect()
}

fn accepted_tmdb_lookup_hint_for_identity(
    metadata_status: &str,
    metadata_provider: Option<&str>,
    metadata_provider_item_id: Option<&str>,
) -> Option<String> {
    if !metadata_status.eq_ignore_ascii_case(METADATA_STATUS_MATCHED)
        || !metadata_provider
            .is_some_and(|provider| provider.eq_ignore_ascii_case(TMDB_PROVIDER_NAME))
    {
        return None;
    }

    normalized_provider_item_id(metadata_provider_item_id)
}

fn normalized_provider_item_id(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn remote_media_type_for_lookup_type(lookup_type: &str) -> Option<&'static str> {
    if lookup_type.eq_ignore_ascii_case("series") {
        return Some(REMOTE_MEDIA_TYPE_SERIES);
    }

    if lookup_type.eq_ignore_ascii_case("movie") {
        return Some(REMOTE_MEDIA_TYPE_MOVIE);
    }

    None
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

fn normalize_recently_added_limit(limit: Option<i64>) -> ApplicationResult<i64> {
    match limit.unwrap_or(DEFAULT_RECENTLY_ADDED_ITEM_LIMIT) {
        value if value <= 0 => Err(ApplicationError::Validation(
            "limit must be a positive integer".to_string(),
        )),
        value => Ok(value.min(MAX_RECENTLY_ADDED_ITEM_LIMIT)),
    }
}

fn normalize_recently_added_days(days: Option<i64>) -> ApplicationResult<Option<i64>> {
    match days {
        None => Ok(None),
        Some(value) if value <= 0 => Err(ApplicationError::Validation(
            "days must be a positive integer".to_string(),
        )),
        Some(value) if value > MAX_RECENTLY_ADDED_DAYS => Ok(Some(MAX_RECENTLY_ADDED_DAYS)),
        Some(value) => Ok(Some(value)),
    }
}

fn normalize_global_search_limit(limit: Option<i64>) -> ApplicationResult<i64> {
    match limit.unwrap_or(DEFAULT_GLOBAL_SEARCH_LIMIT) {
        value if value <= 0 => Err(ApplicationError::Validation(
            "limit must be a positive integer".to_string(),
        )),
        value if value > MAX_GLOBAL_SEARCH_LIMIT => Ok(MAX_GLOBAL_SEARCH_LIMIT),
        value => Ok(value),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        accepted_tmdb_lookup_hint_for_identity, local_metadata_snapshots_for_refresh,
        merge_remote_outline_with_local, normalize_global_search_limit, normalize_page,
        normalize_page_size, normalize_query, normalize_recently_added_days,
        normalize_recently_added_limit, normalize_year, resolve_refreshed_metadata_failure_reason,
        should_replace_remote_data, LocalSeriesEpisode, LocalSeriesSeason,
    };
    use crate::ApplicationError;
    use crate::{RemoteSeriesEpisode, RemoteSeriesEpisodeOutline, RemoteSeriesSeason};
    use mova_domain::MediaSourceKind;
    use mova_scan::{
        inspect_media_file_inventory_shallow, DiscoveredMediaFile, DiscoveredMediaFileInventory,
        LocalNfoArtwork, LocalNfoCollection, LocalNfoCredits, LocalNfoKind, LocalNfoMetadata,
    };
    use std::collections::{BTreeMap, HashMap};
    use std::path::PathBuf;

    fn discovered_file(path: &str) -> DiscoveredMediaFile {
        inspect_media_file_inventory_shallow(DiscoveredMediaFileInventory {
            file_path: PathBuf::from(path),
            source_kind: MediaSourceKind::LocalFile,
            stream_reference_hash: None,
            file_size: 1,
            file_modified_at_ms: Some(1),
            sidecar_fingerprint: String::new(),
        })
        .expect("shallow discovered media file")
    }

    fn movie_nfo(source_path: &str, title: &str) -> LocalNfoMetadata {
        LocalNfoMetadata {
            kind: LocalNfoKind::Movie,
            source_path: PathBuf::from(source_path),
            suppress_tmdb_identity_projection: false,
            title: Some(title.to_string()),
            original_title: None,
            sort_title: None,
            year: Some(2026),
            overview: None,
            outline: None,
            tagline: None,
            status: None,
            premiered: None,
            aired: None,
            date_added: None,
            runtime_minutes: None,
            content_rating: None,
            original_language: None,
            preferred_metadata_language: None,
            preferred_metadata_country_code: None,
            show_title: None,
            end_date: None,
            display_order: None,
            air_days: Vec::new(),
            air_time: None,
            custom_rating: None,
            trailers: Vec::new(),
            aspect_ratio: None,
            top_250: None,
            season_number: None,
            episode_number: None,
            season_count: None,
            episode_count: None,
            display_episode_number: None,
            display_season_number: None,
            display_after_season_number: None,
            show_link: None,
            genres: Vec::new(),
            countries: Vec::new(),
            studios: Vec::new(),
            tags: Vec::new(),
            styles: Vec::new(),
            named_seasons: Vec::new(),
            unique_ids: Vec::new(),
            episode_guide_ids: Vec::new(),
            ratings: Vec::new(),
            credits: LocalNfoCredits::default(),
            artwork: LocalNfoArtwork::default(),
            collection: None::<LocalNfoCollection>,
            lock_data: false,
            locked_fields: Vec::new(),
        }
    }

    #[test]
    fn normalize_query_discards_blank_strings() {
        assert_eq!(normalize_query(Some("   ".to_string())), None);
        assert_eq!(
            normalize_query(Some(" dragon ".to_string())),
            Some("dragon".to_string())
        );
    }

    #[test]
    fn automatic_refresh_uses_only_an_accepted_tmdb_binding_as_direct_hint() {
        assert_eq!(
            accepted_tmdb_lookup_hint_for_identity("matched", Some("TMDB"), Some(" 12345 ")),
            Some("12345".to_string())
        );
        assert_eq!(
            accepted_tmdb_lookup_hint_for_identity("unmatched", Some("tmdb"), Some("12345")),
            None
        );
        assert_eq!(
            accepted_tmdb_lookup_hint_for_identity("matched", Some("other"), Some("12345")),
            None
        );
        assert_eq!(
            accepted_tmdb_lookup_hint_for_identity("matched", Some("tmdb"), Some("   ")),
            None
        );
    }

    #[test]
    fn successful_refresh_does_not_restore_a_stale_failure_reason() {
        assert_eq!(
            resolve_refreshed_metadata_failure_reason(
                mova_domain::METADATA_STATUS_MATCHED,
                None,
                Some(mova_domain::METADATA_FAILURE_PROVIDER_ERROR.to_string()),
            ),
            None
        );
        assert_eq!(
            resolve_refreshed_metadata_failure_reason(
                mova_domain::METADATA_STATUS_MATCHED,
                Some(mova_domain::METADATA_FAILURE_PROVIDER_ERROR.to_string()),
                None,
            )
            .as_deref(),
            Some(mova_domain::METADATA_FAILURE_PROVIDER_ERROR)
        );
    }

    #[test]
    fn refresh_replaces_remote_rows_only_after_remote_metadata_was_applied() {
        assert!(should_replace_remote_data(true, false));
        assert!(!should_replace_remote_data(false, false));
        assert!(!should_replace_remote_data(false, true));
        assert!(!should_replace_remote_data(true, true));
    }

    #[test]
    fn manual_refresh_provider_error_restores_trusted_local_state_for_commit() {
        let mut trusted = discovered_file("/media/Movie/Movie.mkv");
        trusted.metadata_provider = Some("tmdb".to_string());
        trusted.metadata_provider_item_id = Some("123".to_string());
        trusted.metadata_status = Some(mova_domain::METADATA_STATUS_MATCHED.to_string());
        trusted.title = "Local NFO title".to_string();
        trusted.overview = Some("Local NFO overview".to_string());
        trusted.poster_path = Some("/media/Movie/poster.jpg".to_string());

        let mut partially_enriched = trusted.clone();
        partially_enriched.title = "Partial remote title".to_string();
        partially_enriched.overview = None;
        partially_enriched.poster_path = None;
        partially_enriched.metadata_provider_item_id = Some("999".to_string());
        let mut files = vec![partially_enriched];

        crate::scan_jobs::restore_group_after_provider_error(
            &mut files,
            vec![trusted],
            Some(mova_domain::REMOTE_MEDIA_TYPE_MOVIE),
        );

        assert_eq!(files[0].title, "Local NFO title");
        assert_eq!(files[0].overview.as_deref(), Some("Local NFO overview"));
        assert_eq!(
            files[0].poster_path.as_deref(),
            Some("/media/Movie/poster.jpg")
        );
        assert_eq!(files[0].metadata_provider_item_id.as_deref(), Some("123"));
        assert_eq!(
            files[0].metadata_status.as_deref(),
            Some(mova_domain::METADATA_STATUS_MATCHED)
        );
        assert_eq!(
            files[0].metadata_failure_reason.as_deref(),
            Some(mova_domain::METADATA_FAILURE_PROVIDER_ERROR)
        );
    }

    #[test]
    fn metadata_refresh_keeps_every_version_nfo_and_selects_exactly_one() {
        let mut files = vec![
            discovered_file("/media/Movie/Movie.1080p.mkv"),
            discovered_file("/media/Movie/Movie.2160p.mkv"),
        ];
        files[0].local_nfo = Some(movie_nfo(
            "/media/Movie/Movie.1080p.nfo",
            "Movie 1080p metadata",
        ));
        files[1].local_nfo = Some(movie_nfo(
            "/media/Movie/Movie.2160p.nfo",
            "Movie 2160p metadata",
        ));

        crate::local_metadata::apply_group_local_metadata(&mut files, "movie");
        let snapshots = local_metadata_snapshots_for_refresh("movie", &files);

        assert_eq!(snapshots.len(), 2);
        assert_eq!(
            snapshots
                .iter()
                .map(|snapshot| snapshot.source_path.as_str())
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([
                "/media/Movie/Movie.1080p.nfo",
                "/media/Movie/Movie.2160p.nfo",
            ])
        );
        assert_eq!(
            snapshots
                .iter()
                .filter(|snapshot| snapshot.is_selected)
                .count(),
            1
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
    fn normalize_recently_added_limit_rejects_non_positive_values() {
        assert!(matches!(
            normalize_recently_added_limit(Some(0)),
            Err(ApplicationError::Validation(message))
                if message.contains("positive integer")
        ));
    }

    #[test]
    fn normalize_recently_added_limit_caps_large_values() {
        assert_eq!(normalize_recently_added_limit(Some(80)).unwrap(), 50);
    }

    #[test]
    fn normalize_recently_added_limit_uses_safe_default() {
        assert_eq!(normalize_recently_added_limit(None).unwrap(), 8);
    }

    #[test]
    fn normalize_recently_added_days_omits_time_filter_by_default() {
        assert_eq!(normalize_recently_added_days(None).unwrap(), None);
    }

    #[test]
    fn normalize_recently_added_days_rejects_non_positive_values() {
        assert!(matches!(
            normalize_recently_added_days(Some(0)),
            Err(ApplicationError::Validation(message))
                if message.contains("positive integer")
        ));
    }

    #[test]
    fn normalize_recently_added_days_caps_large_values() {
        assert_eq!(normalize_recently_added_days(Some(900)).unwrap(), Some(365));
    }

    #[test]
    fn normalize_global_search_limit_rejects_non_positive_values() {
        assert!(matches!(
            normalize_global_search_limit(Some(0)),
            Err(ApplicationError::Validation(message))
                if message.contains("positive integer")
        ));
    }

    #[test]
    fn normalize_global_search_limit_caps_large_values() {
        assert_eq!(normalize_global_search_limit(Some(100)).unwrap(), 30);
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
                intro_start_seconds: None,
                intro_end_seconds: None,
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
                intro_start_seconds: None,
                intro_end_seconds: None,
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
                intro_start_seconds: None,
                intro_end_seconds: None,
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
    fn merge_remote_outline_prefers_remote_artwork_for_available_entries() {
        let mut season_1_episodes = BTreeMap::new();
        season_1_episodes.insert(
            1,
            LocalSeriesEpisode {
                media_item_id: 11,
                title: "Local Episode 1".to_string(),
                overview: None,
                poster_path: Some("/cache/local-episode-poster.jpg".to_string()),
                backdrop_path: Some("/cache/local-episode-backdrop.jpg".to_string()),
                intro_start_seconds: None,
                intro_end_seconds: None,
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
                intro_start_seconds: None,
                intro_end_seconds: None,
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
            Some("https://image.tmdb.org/remote-season-poster.jpg")
        );
        assert_eq!(
            merged.seasons[0].backdrop_path.as_deref(),
            Some("https://image.tmdb.org/remote-season-backdrop.jpg")
        );
        assert_eq!(
            merged.seasons[0].episodes[0].poster_path.as_deref(),
            Some("https://image.tmdb.org/remote-episode-poster.jpg")
        );
        assert_eq!(
            merged.seasons[0].episodes[0].backdrop_path.as_deref(),
            Some("https://image.tmdb.org/remote-episode-backdrop.jpg")
        );
    }

    #[test]
    fn merge_remote_outline_keeps_remote_missing_artwork_empty() {
        let mut season_1_episodes = BTreeMap::new();
        season_1_episodes.insert(
            1,
            LocalSeriesEpisode {
                media_item_id: 11,
                title: "Local Episode 1".to_string(),
                overview: None,
                poster_path: Some("/cache/local-episode-poster.jpg".to_string()),
                backdrop_path: Some("/cache/local-episode-backdrop.jpg".to_string()),
                intro_start_seconds: None,
                intro_end_seconds: None,
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
                intro_start_seconds: None,
                intro_end_seconds: None,
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
                    episodes: vec![RemoteSeriesEpisode {
                        episode_number: 1,
                        title: Some("Pilot".to_string()),
                        overview: None,
                        poster_path: None,
                        backdrop_path: None,
                    }],
                }],
            },
            &local_inventory,
            &HashMap::new(),
        );

        assert_eq!(merged.seasons[0].poster_path, None);
        assert_eq!(merged.seasons[0].backdrop_path, None);
        assert_eq!(merged.seasons[0].episodes[0].poster_path, None);
        assert_eq!(merged.seasons[0].episodes[0].backdrop_path, None);
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
                intro_start_seconds: None,
                intro_end_seconds: None,
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
                intro_start_seconds: None,
                intro_end_seconds: None,
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
