use axum::{http::StatusCode, Json};
use mova_application::{
    MediaItemPlaybackHeader, MetadataMatchCandidate, ScanJobItemProgressUpdate,
    SeriesEpisodeOutline, SeriesEpisodeOutlineEpisode, SeriesEpisodeOutlineSeason,
};
use mova_domain::{
    AudioTrack, ContinueWatchingItem, Episode, Library, LibraryDetail, MediaCastMember, MediaFile,
    MediaItem, PlaybackProgress, ScanJob, Season, SubtitleFile, UserProfile, WatchHistory,
    WatchHistoryItem,
};
use serde::Serialize;
use time::{format_description::well_known::Rfc3339, OffsetDateTime, UtcOffset};

/// 所有 JSON 业务接口统一包裹成 code/message/data，便于前端和第三方客户端稳定消费。
#[derive(Debug, Serialize)]
pub struct ApiEnvelope<T> {
    pub code: u16,
    pub message: String,
    pub data: T,
}

pub type ApiJson<T> = Json<ApiEnvelope<T>>;

pub fn ok<T>(data: T) -> ApiJson<T> {
    Json(ApiEnvelope {
        code: StatusCode::OK.as_u16(),
        message: "ok".to_string(),
        data,
    })
}

pub fn ok_message<T>(message: impl Into<String>, data: T) -> ApiJson<T> {
    Json(ApiEnvelope {
        code: StatusCode::OK.as_u16(),
        message: message.into(),
        data,
    })
}

pub fn with_status<T>(
    status: StatusCode,
    message: impl Into<String>,
    data: T,
) -> (StatusCode, ApiJson<T>) {
    (
        status,
        Json(ApiEnvelope {
            code: status.as_u16(),
            message: message.into(),
            data,
        }),
    )
}

pub fn created<T>(data: T) -> (StatusCode, ApiJson<T>) {
    with_status(StatusCode::CREATED, "created", data)
}

pub fn accepted<T>(data: T) -> (StatusCode, ApiJson<T>) {
    with_status(StatusCode::ACCEPTED, "accepted", data)
}

/// 面向 HTTP 接口返回的媒体库对象。
/// 时间字段会在这里按配置时区格式化成可读字符串。
#[derive(Debug, Serialize)]
pub struct LibraryResponse {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub metadata_language: String,
    pub root_path: String,
    pub is_enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 面向 HTTP 接口返回的扫描摘要对象。
/// 详情页只需要最近一次扫描的核心信息，不必带整段历史。
#[derive(Debug, Serialize)]
pub struct LibraryLastScanResponse {
    pub id: i64,
    pub status: String,
    pub phase: Option<String>,
    pub total_files: i32,
    pub scanned_files: i32,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error_message: Option<String>,
}

/// 面向 HTTP 接口返回的媒体库详情对象。
/// 这里聚合了库信息、媒体数量和最近一次扫描摘要。
#[derive(Debug, Serialize)]
pub struct LibraryDetailResponse {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub metadata_language: String,
    pub root_path: String,
    pub is_enabled: bool,
    pub media_count: i64,
    pub movie_count: i64,
    pub series_count: i64,
    pub last_scan: Option<LibraryLastScanResponse>,
    pub created_at: String,
    pub updated_at: String,
}

/// 面向 HTTP 接口返回的媒体条目对象。
#[derive(Debug, Serialize)]
pub struct MediaItemResponse {
    pub id: i64,
    pub library_id: i64,
    pub media_type: String,
    pub title: String,
    pub source_title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub year: Option<i32>,
    pub imdb_rating: Option<String>,
    pub country: Option<String>,
    pub genres: Option<String>,
    pub studio: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct MediaCastMemberResponse {
    pub person_id: Option<i64>,
    pub sort_order: i32,
    pub name: String,
    pub character_name: Option<String>,
    pub profile_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MediaItemDetailResponse {
    pub id: i64,
    pub library_id: i64,
    pub media_type: String,
    pub title: String,
    pub source_title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub year: Option<i32>,
    pub imdb_rating: Option<String>,
    pub country: Option<String>,
    pub genres: Option<String>,
    pub studio: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct MediaItemPlaybackHeaderResponse {
    pub media_item_id: i64,
    pub library_id: i64,
    pub media_type: String,
    pub series_media_item_id: Option<i64>,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub episode_title: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MetadataMatchCandidateResponse {
    pub provider_item_id: i64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MediaItemListResponse {
    pub items: Vec<MediaItemResponse>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Serialize)]
pub struct SeasonResponse {
    pub id: i64,
    pub series_id: i64,
    pub season_number: i32,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub intro_start_seconds: Option<i32>,
    pub intro_end_seconds: Option<i32>,
    pub episode_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct EpisodeResponse {
    pub id: i64,
    pub media_item_id: i64,
    pub series_id: i64,
    pub season_id: i64,
    pub episode_number: i32,
    pub title: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub intro_start_seconds: Option<i32>,
    pub intro_end_seconds: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct SeriesEpisodeOutlineResponse {
    pub seasons: Vec<SeriesEpisodeOutlineSeasonResponse>,
}

#[derive(Debug, Serialize)]
pub struct SeriesEpisodeOutlineSeasonResponse {
    pub season_id: Option<i64>,
    pub season_number: i32,
    pub title: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub intro_start_seconds: Option<i32>,
    pub intro_end_seconds: Option<i32>,
    pub episodes: Vec<SeriesEpisodeOutlineEpisodeResponse>,
}

#[derive(Debug, Serialize)]
pub struct SeriesEpisodeOutlineEpisodeResponse {
    pub episode_number: i32,
    pub title: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub intro_start_seconds: Option<i32>,
    pub intro_end_seconds: Option<i32>,
    pub media_item_id: Option<i64>,
    pub is_available: bool,
    pub playback_progress: Option<EpisodePlaybackProgressResponse>,
}

#[derive(Debug, Serialize)]
pub struct EpisodePlaybackProgressResponse {
    pub position_seconds: i32,
    pub duration_seconds: Option<i32>,
    pub is_finished: bool,
}

/// 面向 HTTP 接口返回的媒体文件对象。
#[derive(Debug, Serialize)]
pub struct MediaFileResponse {
    pub id: i64,
    pub media_item_id: i64,
    pub file_path: String,
    pub container: Option<String>,
    pub file_size: i64,
    pub duration_seconds: Option<i32>,
    pub video_title: Option<String>,
    pub video_codec: Option<String>,
    pub video_profile: Option<String>,
    pub video_level: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bitrate: Option<i64>,
    pub video_bitrate: Option<i64>,
    pub video_frame_rate: Option<f64>,
    pub video_aspect_ratio: Option<String>,
    pub video_scan_type: Option<String>,
    pub video_color_primaries: Option<String>,
    pub video_color_space: Option<String>,
    pub video_color_transfer: Option<String>,
    pub video_bit_depth: Option<i32>,
    pub video_pixel_format: Option<String>,
    pub video_reference_frames: Option<i32>,
    pub scan_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct SubtitleFileResponse {
    pub id: i64,
    pub media_file_id: i64,
    pub source_kind: String,
    pub file_path: Option<String>,
    pub stream_index: Option<i32>,
    pub language: Option<String>,
    pub subtitle_format: String,
    pub label: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct AudioTrackResponse {
    pub id: i64,
    pub media_file_id: i64,
    pub stream_index: i32,
    pub language: Option<String>,
    pub audio_codec: Option<String>,
    pub label: Option<String>,
    pub channel_layout: Option<String>,
    pub channels: Option<i32>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i32>,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 面向 HTTP 接口返回的扫描任务对象。
#[derive(Debug, Serialize)]
pub struct ScanJobResponse {
    pub id: i64,
    pub library_id: i64,
    pub status: String,
    pub phase: Option<String>,
    pub total_files: i32,
    pub scanned_files: i32,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ScanItemProgressResponse {
    pub scan_job_id: i64,
    pub library_id: i64,
    pub item_key: String,
    pub media_type: String,
    pub title: String,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub item_index: i32,
    pub total_items: i32,
    pub stage: String,
    pub progress_percent: i32,
}

/// 面向 HTTP 接口返回的播放进度对象。
#[derive(Debug, Serialize)]
pub struct PlaybackProgressResponse {
    pub id: i64,
    pub media_item_id: i64,
    pub media_file_id: i64,
    pub position_seconds: i32,
    pub duration_seconds: Option<i32>,
    pub last_watched_at: String,
    pub is_finished: bool,
}

/// 面向 HTTP 接口返回的“继续观看”列表对象。
#[derive(Debug, Serialize)]
pub struct ContinueWatchingItemResponse {
    pub media_item: MediaItemResponse,
    pub playback_progress: PlaybackProgressResponse,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub episode_title: Option<String>,
    pub episode_overview: Option<String>,
    pub episode_poster_path: Option<String>,
    pub episode_backdrop_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WatchHistoryResponse {
    pub id: i64,
    pub media_item_id: i64,
    pub media_file_id: i64,
    pub position_seconds: i32,
    pub duration_seconds: Option<i32>,
    pub started_at: String,
    pub last_watched_at: String,
    pub ended_at: Option<String>,
    pub completed_at: Option<String>,
    pub is_finished: bool,
}

#[derive(Debug, Serialize)]
pub struct WatchHistoryItemResponse {
    pub media_item: MediaItemResponse,
    pub watch_history: WatchHistoryResponse,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub nickname: String,
    pub role: String,
    pub is_enabled: bool,
    pub library_ids: Vec<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct BootstrapStatusResponse {
    pub bootstrap_required: bool,
}

impl LibraryResponse {
    pub fn from_domain(library: Library, offset: UtcOffset) -> Self {
        Self {
            id: library.id,
            name: library.name,
            description: library.description,
            metadata_language: library.metadata_language,
            root_path: library.root_path,
            is_enabled: library.is_enabled,
            created_at: format_datetime(library.created_at, offset),
            updated_at: format_datetime(library.updated_at, offset),
        }
    }
}

impl LibraryDetailResponse {
    pub fn from_domain(detail: LibraryDetail, offset: UtcOffset) -> Self {
        Self {
            id: detail.library.id,
            name: detail.library.name,
            description: detail.library.description,
            metadata_language: detail.library.metadata_language,
            root_path: detail.library.root_path,
            is_enabled: detail.library.is_enabled,
            media_count: detail.media_count,
            movie_count: detail.movie_count,
            series_count: detail.series_count,
            last_scan: detail
                .last_scan
                .map(|scan_job| LibraryLastScanResponse::from_domain(scan_job, offset)),
            created_at: format_datetime(detail.library.created_at, offset),
            updated_at: format_datetime(detail.library.updated_at, offset),
        }
    }
}

impl MediaItemResponse {
    pub fn from_domain(media_item: MediaItem, offset: UtcOffset) -> Self {
        let title = display_media_item_title(&media_item);

        Self {
            id: media_item.id,
            library_id: media_item.library_id,
            media_type: media_item.media_type,
            title,
            source_title: media_item.source_title,
            original_title: media_item.original_title,
            sort_title: media_item.sort_title,
            year: media_item.year,
            imdb_rating: media_item.imdb_rating,
            country: media_item.country,
            genres: media_item.genres,
            studio: media_item.studio,
            overview: media_item.overview,
            poster_path: public_media_item_asset_path(
                media_item.id,
                media_item.poster_path.as_deref(),
                "poster",
                media_item.updated_at,
            ),
            backdrop_path: public_media_item_asset_path(
                media_item.id,
                media_item.backdrop_path.as_deref(),
                "backdrop",
                media_item.updated_at,
            ),
            created_at: format_datetime(media_item.created_at, offset),
            updated_at: format_datetime(media_item.updated_at, offset),
        }
    }
}

impl MediaCastMemberResponse {
    pub fn from_domain(member: MediaCastMember) -> Self {
        Self {
            person_id: member.person_id,
            sort_order: member.sort_order,
            name: member.name,
            character_name: member.character_name,
            profile_path: member.profile_path,
        }
    }
}

impl MediaItemDetailResponse {
    pub fn from_domain(media_item: MediaItem, offset: UtcOffset) -> Self {
        let title = display_media_item_title(&media_item);

        Self {
            id: media_item.id,
            library_id: media_item.library_id,
            media_type: media_item.media_type,
            title,
            source_title: media_item.source_title,
            original_title: media_item.original_title,
            sort_title: media_item.sort_title,
            year: media_item.year,
            imdb_rating: media_item.imdb_rating,
            country: media_item.country,
            genres: media_item.genres,
            studio: media_item.studio,
            overview: media_item.overview,
            poster_path: public_media_item_asset_path(
                media_item.id,
                media_item.poster_path.as_deref(),
                "poster",
                media_item.updated_at,
            ),
            backdrop_path: public_media_item_asset_path(
                media_item.id,
                media_item.backdrop_path.as_deref(),
                "backdrop",
                media_item.updated_at,
            ),
            created_at: format_datetime(media_item.created_at, offset),
            updated_at: format_datetime(media_item.updated_at, offset),
        }
    }
}

impl MediaItemPlaybackHeaderResponse {
    pub fn from_domain(header: MediaItemPlaybackHeader) -> Self {
        Self {
            media_item_id: header.media_item_id,
            library_id: header.library_id,
            media_type: header.media_type,
            series_media_item_id: header.series_media_item_id,
            title: header.title,
            original_title: header.original_title,
            year: header.year,
            season_number: header.season_number,
            episode_number: header.episode_number,
            episode_title: header.episode_title,
        }
    }
}

impl MetadataMatchCandidateResponse {
    pub fn from_domain(candidate: MetadataMatchCandidate) -> Self {
        Self {
            provider_item_id: candidate.provider_item_id,
            title: candidate.title,
            original_title: candidate.original_title,
            year: candidate.year,
            overview: candidate.overview,
            poster_path: candidate.poster_path,
            backdrop_path: candidate.backdrop_path,
        }
    }
}

impl MediaItemListResponse {
    pub fn from_domain(
        result: mova_application::ListMediaItemsForLibraryOutput,
        offset: UtcOffset,
    ) -> Self {
        Self {
            items: result
                .items
                .into_iter()
                .map(|media_item| MediaItemResponse::from_domain(media_item, offset))
                .collect(),
            total: result.total,
            page: result.page,
            page_size: result.page_size,
        }
    }
}

impl UserResponse {
    pub fn from_domain(user: UserProfile, offset: UtcOffset) -> Self {
        Self {
            id: user.user.id,
            username: user.user.username,
            nickname: user.user.nickname,
            role: user.user.role.as_str().to_string(),
            is_enabled: user.user.is_enabled,
            library_ids: user.library_ids,
            created_at: format_datetime(user.user.created_at, offset),
            updated_at: format_datetime(user.user.updated_at, offset),
        }
    }
}

impl WatchHistoryResponse {
    pub fn from_domain(history: WatchHistory, offset: UtcOffset) -> Self {
        Self {
            id: history.id,
            media_item_id: history.media_item_id,
            media_file_id: history.media_file_id,
            position_seconds: history.position_seconds,
            duration_seconds: history.duration_seconds,
            started_at: format_datetime(history.started_at, offset),
            last_watched_at: format_datetime(history.last_watched_at, offset),
            ended_at: history.ended_at.map(|value| format_datetime(value, offset)),
            completed_at: history
                .completed_at
                .map(|value| format_datetime(value, offset)),
            is_finished: history.completed_at.is_some(),
        }
    }
}

impl WatchHistoryItemResponse {
    pub fn from_domain(item: WatchHistoryItem, offset: UtcOffset) -> Self {
        Self {
            media_item: MediaItemResponse::from_domain(item.media_item, offset),
            watch_history: WatchHistoryResponse::from_domain(item.watch_history, offset),
        }
    }
}

impl SeasonResponse {
    pub fn from_domain(season: Season, offset: UtcOffset) -> Self {
        Self {
            id: season.id,
            series_id: season.series_id,
            season_number: season.season_number,
            title: season.title,
            overview: season.overview,
            poster_path: public_season_asset_path(
                season.id,
                season.poster_path.as_deref(),
                "poster",
                season.updated_at,
            ),
            backdrop_path: public_season_asset_path(
                season.id,
                season.backdrop_path.as_deref(),
                "backdrop",
                season.updated_at,
            ),
            intro_start_seconds: season.intro_start_seconds,
            intro_end_seconds: season.intro_end_seconds,
            episode_count: season.episode_count,
            created_at: format_datetime(season.created_at, offset),
            updated_at: format_datetime(season.updated_at, offset),
        }
    }
}

impl EpisodeResponse {
    pub fn from_domain(episode: Episode, offset: UtcOffset) -> Self {
        Self {
            id: episode.id,
            media_item_id: episode.media_item_id,
            series_id: episode.series_id,
            season_id: episode.season_id,
            episode_number: episode.episode_number,
            title: episode.title,
            overview: episode.overview,
            poster_path: public_media_item_asset_path(
                episode.media_item_id,
                episode.poster_path.as_deref(),
                "poster",
                episode.updated_at,
            ),
            backdrop_path: public_media_item_asset_path(
                episode.media_item_id,
                episode.backdrop_path.as_deref(),
                "backdrop",
                episode.updated_at,
            ),
            intro_start_seconds: episode.intro_start_seconds,
            intro_end_seconds: episode.intro_end_seconds,
            created_at: format_datetime(episode.created_at, offset),
            updated_at: format_datetime(episode.updated_at, offset),
        }
    }
}

impl SeriesEpisodeOutlineResponse {
    pub fn from_domain(outline: SeriesEpisodeOutline) -> Self {
        Self {
            seasons: outline
                .seasons
                .into_iter()
                .map(SeriesEpisodeOutlineSeasonResponse::from_domain)
                .collect(),
        }
    }
}

impl SeriesEpisodeOutlineSeasonResponse {
    fn from_domain(season: SeriesEpisodeOutlineSeason) -> Self {
        let poster_path = match season.season_id {
            Some(season_id) => public_season_asset_path(
                season_id,
                season.poster_path.as_deref(),
                "poster",
                OffsetDateTime::UNIX_EPOCH,
            ),
            None => season.poster_path.as_deref().map(|value| value.to_string()),
        };
        let backdrop_path = match season.season_id {
            Some(season_id) => public_season_asset_path(
                season_id,
                season.backdrop_path.as_deref(),
                "backdrop",
                OffsetDateTime::UNIX_EPOCH,
            ),
            None => season
                .backdrop_path
                .as_deref()
                .map(|value| value.to_string()),
        };

        Self {
            season_id: season.season_id,
            season_number: season.season_number,
            title: season.title,
            year: season.year,
            overview: season.overview,
            poster_path,
            backdrop_path,
            intro_start_seconds: season.intro_start_seconds,
            intro_end_seconds: season.intro_end_seconds,
            episodes: season
                .episodes
                .into_iter()
                .map(SeriesEpisodeOutlineEpisodeResponse::from_domain)
                .collect(),
        }
    }
}

impl SeriesEpisodeOutlineEpisodeResponse {
    fn from_domain(episode: SeriesEpisodeOutlineEpisode) -> Self {
        let poster_path = episode.media_item_id.and_then(|media_item_id| {
            public_media_item_asset_path(
                media_item_id,
                episode.poster_path.as_deref(),
                "poster",
                OffsetDateTime::UNIX_EPOCH,
            )
        });
        let backdrop_path = episode.media_item_id.and_then(|media_item_id| {
            public_media_item_asset_path(
                media_item_id,
                episode.backdrop_path.as_deref(),
                "backdrop",
                OffsetDateTime::UNIX_EPOCH,
            )
        });

        Self {
            episode_number: episode.episode_number,
            title: episode.title,
            overview: episode.overview,
            poster_path: poster_path.or(episode.poster_path),
            backdrop_path: backdrop_path.or(episode.backdrop_path),
            intro_start_seconds: episode.intro_start_seconds,
            intro_end_seconds: episode.intro_end_seconds,
            media_item_id: episode.media_item_id,
            is_available: episode.is_available,
            playback_progress: episode
                .playback_progress
                .map(EpisodePlaybackProgressResponse::from_domain),
        }
    }
}

impl EpisodePlaybackProgressResponse {
    fn from_domain(progress: PlaybackProgress) -> Self {
        Self {
            position_seconds: progress.position_seconds,
            duration_seconds: progress.duration_seconds,
            is_finished: progress.is_finished,
        }
    }
}

impl MediaFileResponse {
    pub fn from_domain(media_file: MediaFile, offset: UtcOffset) -> Self {
        Self {
            id: media_file.id,
            media_item_id: media_file.media_item_id,
            file_path: media_file.file_path,
            container: media_file.container,
            file_size: media_file.file_size,
            duration_seconds: media_file.duration_seconds,
            video_title: media_file.video_title,
            video_codec: media_file.video_codec,
            video_profile: media_file.video_profile,
            video_level: media_file.video_level,
            audio_codec: media_file.audio_codec,
            width: media_file.width,
            height: media_file.height,
            bitrate: media_file.bitrate,
            video_bitrate: media_file.video_bitrate,
            video_frame_rate: media_file.video_frame_rate,
            video_aspect_ratio: media_file.video_aspect_ratio,
            video_scan_type: media_file.video_scan_type,
            video_color_primaries: media_file.video_color_primaries,
            video_color_space: media_file.video_color_space,
            video_color_transfer: media_file.video_color_transfer,
            video_bit_depth: media_file.video_bit_depth,
            video_pixel_format: media_file.video_pixel_format,
            video_reference_frames: media_file.video_reference_frames,
            scan_hash: media_file.scan_hash,
            created_at: format_datetime(media_file.created_at, offset),
            updated_at: format_datetime(media_file.updated_at, offset),
        }
    }
}

impl SubtitleFileResponse {
    pub fn from_domain(subtitle_file: SubtitleFile, offset: UtcOffset) -> Self {
        Self {
            id: subtitle_file.id,
            media_file_id: subtitle_file.media_file_id,
            source_kind: subtitle_file.source_kind,
            file_path: subtitle_file.file_path,
            stream_index: subtitle_file.stream_index,
            language: subtitle_file.language,
            subtitle_format: subtitle_file.subtitle_format,
            label: subtitle_file.label,
            is_default: subtitle_file.is_default,
            is_forced: subtitle_file.is_forced,
            is_hearing_impaired: subtitle_file.is_hearing_impaired,
            created_at: format_datetime(subtitle_file.created_at, offset),
            updated_at: format_datetime(subtitle_file.updated_at, offset),
        }
    }
}

impl AudioTrackResponse {
    pub fn from_domain(audio_track: AudioTrack, offset: UtcOffset) -> Self {
        Self {
            id: audio_track.id,
            media_file_id: audio_track.media_file_id,
            stream_index: audio_track.stream_index,
            language: audio_track.language,
            audio_codec: audio_track.audio_codec,
            label: audio_track.label,
            channel_layout: audio_track.channel_layout,
            channels: audio_track.channels,
            bitrate: audio_track.bitrate,
            sample_rate: audio_track.sample_rate,
            is_default: audio_track.is_default,
            created_at: format_datetime(audio_track.created_at, offset),
            updated_at: format_datetime(audio_track.updated_at, offset),
        }
    }
}

impl ScanJobResponse {
    pub fn from_domain(scan_job: ScanJob, offset: UtcOffset) -> Self {
        Self::from_realtime(scan_job, None, offset)
    }

    pub fn from_realtime(scan_job: ScanJob, phase: Option<String>, offset: UtcOffset) -> Self {
        Self {
            id: scan_job.id,
            library_id: scan_job.library_id,
            status: scan_job.status,
            phase,
            total_files: scan_job.total_files,
            scanned_files: scan_job.scanned_files,
            created_at: format_datetime(scan_job.created_at, offset),
            started_at: format_optional_datetime(scan_job.started_at, offset),
            finished_at: format_optional_datetime(scan_job.finished_at, offset),
            error_message: scan_job.error_message,
        }
    }
}

impl LibraryLastScanResponse {
    pub fn from_domain(scan_job: ScanJob, offset: UtcOffset) -> Self {
        Self {
            id: scan_job.id,
            status: scan_job.status,
            phase: None,
            total_files: scan_job.total_files,
            scanned_files: scan_job.scanned_files,
            created_at: format_datetime(scan_job.created_at, offset),
            started_at: format_optional_datetime(scan_job.started_at, offset),
            finished_at: format_optional_datetime(scan_job.finished_at, offset),
            error_message: scan_job.error_message,
        }
    }
}

impl ScanItemProgressResponse {
    pub fn from_domain(item: ScanJobItemProgressUpdate) -> Self {
        Self {
            scan_job_id: item.scan_job_id,
            library_id: item.library_id,
            item_key: item.item_key,
            media_type: item.media_type,
            title: item.title,
            season_number: item.season_number,
            episode_number: item.episode_number,
            item_index: item.item_index,
            total_items: item.total_items,
            stage: item.stage,
            progress_percent: item.progress_percent,
        }
    }
}

impl PlaybackProgressResponse {
    pub fn from_domain(progress: PlaybackProgress, offset: UtcOffset) -> Self {
        Self {
            id: progress.id,
            media_item_id: progress.media_item_id,
            media_file_id: progress.media_file_id,
            position_seconds: progress.position_seconds,
            duration_seconds: progress.duration_seconds,
            last_watched_at: format_datetime(progress.last_watched_at, offset),
            is_finished: progress.is_finished,
        }
    }
}

impl ContinueWatchingItemResponse {
    pub fn from_domain(item: ContinueWatchingItem, offset: UtcOffset) -> Self {
        let episode_poster_path = public_media_item_asset_path(
            item.playback_progress.media_item_id,
            item.episode_poster_path.as_deref(),
            "poster",
            item.media_item.updated_at,
        )
        .or(item.episode_poster_path.clone());
        let episode_backdrop_path = public_media_item_asset_path(
            item.playback_progress.media_item_id,
            item.episode_backdrop_path.as_deref(),
            "backdrop",
            item.media_item.updated_at,
        )
        .or(item.episode_backdrop_path.clone());

        Self {
            media_item: MediaItemResponse::from_domain(item.media_item, offset),
            playback_progress: PlaybackProgressResponse::from_domain(
                item.playback_progress,
                offset,
            ),
            season_number: item.season_number,
            episode_number: item.episode_number,
            episode_title: item.episode_title,
            episode_overview: item.episode_overview,
            episode_poster_path,
            episode_backdrop_path,
        }
    }
}

fn format_datetime(datetime: OffsetDateTime, offset: UtcOffset) -> String {
    let localized = datetime.to_offset(offset);

    localized
        .format(&Rfc3339)
        .unwrap_or_else(|_| localized.unix_timestamp().to_string())
}

fn display_media_item_title(media_item: &MediaItem) -> String {
    // 列表和详情首要目标是把条目展示出来，所以远端标题缺失或异常时要回退到本地解析标题。
    if !media_item.title.trim().is_empty() {
        return media_item.title.clone();
    }

    if !media_item.source_title.trim().is_empty() {
        return media_item.source_title.clone();
    }

    "Untitled".to_string()
}

fn format_optional_datetime(datetime: Option<OffsetDateTime>, offset: UtcOffset) -> Option<String> {
    datetime.map(|value| format_datetime(value, offset))
}

fn public_media_item_asset_path(
    media_item_id: i64,
    stored_path: Option<&str>,
    asset_kind: &str,
    version: OffsetDateTime,
) -> Option<String> {
    let stored_path = stored_path?.trim();

    if stored_path.is_empty() {
        return None;
    }

    if is_external_url(stored_path) {
        Some(stored_path.to_string())
    } else {
        Some(format!(
            "/api/media-items/{}/{}?v={}",
            media_item_id,
            asset_kind,
            version.unix_timestamp()
        ))
    }
}

fn public_season_asset_path(
    season_id: i64,
    stored_path: Option<&str>,
    asset_kind: &str,
    version: OffsetDateTime,
) -> Option<String> {
    let stored_path = stored_path?.trim();

    if stored_path.is_empty() {
        return None;
    }

    if is_external_url(stored_path) {
        Some(stored_path.to_string())
    } else {
        Some(format!(
            "/api/seasons/{}/{}?v={}",
            season_id,
            asset_kind,
            version.unix_timestamp()
        ))
    }
}

fn is_external_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::{public_media_item_asset_path, public_season_asset_path, MediaItemResponse};
    use mova_domain::MediaItem;
    use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

    fn sample_media_item() -> MediaItem {
        let timestamp = PrimitiveDateTime::new(
            Date::from_calendar_date(2024, Month::January, 2).unwrap(),
            Time::from_hms(3, 4, 5).unwrap(),
        )
        .assume_utc();

        MediaItem {
            id: 42,
            library_id: 7,
            media_type: "movie".to_string(),
            title: "Spirited Away".to_string(),
            source_title: "Spirited Away".to_string(),
            original_title: None,
            sort_title: None,
            metadata_provider: None,
            metadata_provider_item_id: None,
            year: Some(2001),
            imdb_rating: Some("8.6".to_string()),
            country: Some("Japan".to_string()),
            genres: Some("Animation · Fantasy".to_string()),
            studio: Some("Studio Ghibli".to_string()),
            overview: Some("A young girl enters the spirit world.".to_string()),
            poster_path: Some("/library/poster.jpg".to_string()),
            backdrop_path: Some("https://images.example.com/backdrop.jpg".to_string()),
            created_at: timestamp,
            updated_at: timestamp,
        }
    }

    #[test]
    fn public_media_item_asset_path_maps_local_files_to_internal_routes() {
        assert_eq!(
            public_media_item_asset_path(
                42,
                Some("/library/poster.jpg"),
                "poster",
                OffsetDateTime::UNIX_EPOCH,
            ),
            Some("/api/media-items/42/poster?v=0".to_string())
        );
    }

    #[test]
    fn public_media_item_asset_path_preserves_remote_urls() {
        assert_eq!(
            public_media_item_asset_path(
                42,
                Some("https://images.example.com/poster.jpg"),
                "poster",
                OffsetDateTime::UNIX_EPOCH,
            ),
            Some("https://images.example.com/poster.jpg".to_string())
        );
    }

    #[test]
    fn public_season_asset_path_maps_local_files_to_internal_routes() {
        assert_eq!(
            public_season_asset_path(
                7,
                Some("/library/season01.jpg"),
                "poster",
                OffsetDateTime::UNIX_EPOCH,
            ),
            Some("/api/seasons/7/poster?v=0".to_string())
        );
    }

    #[test]
    fn public_season_asset_path_preserves_remote_urls() {
        assert_eq!(
            public_season_asset_path(
                7,
                Some("https://images.example.com/season01.jpg"),
                "poster",
                OffsetDateTime::UNIX_EPOCH,
            ),
            Some("https://images.example.com/season01.jpg".to_string())
        );
    }

    #[test]
    fn media_item_response_exposes_public_asset_urls() {
        let response = MediaItemResponse::from_domain(sample_media_item(), UtcOffset::UTC);

        assert_eq!(
            response.poster_path.as_deref(),
            Some("/api/media-items/42/poster?v=1704164645")
        );
        assert_eq!(
            response.backdrop_path.as_deref(),
            Some("https://images.example.com/backdrop.jpg")
        );
    }
}
