mod error;
mod file_sync;
mod libraries;
mod media_cast;
mod media_classification;
mod media_enrichment;
mod media_items;
mod metadata;
mod metadata_match;
mod playback_header;
mod playback_progress;
mod scan_jobs;
mod users;
mod watch_history;

pub use error::{ApplicationError, ApplicationResult};
pub use file_sync::{reconcile_library_inventory, sync_library_filesystem_changes};
pub use libraries::{
    create_library, delete_library, get_library, get_library_detail, list_libraries,
    update_library, CreateLibraryInput, UpdateLibraryInput,
};
pub use media_cast::{
    invalidate_media_item_cast_cache, list_media_item_cast, refresh_media_item_cast_if_stale,
};
pub use media_classification::{LIBRARY_TYPE_MIXED, LIBRARY_TYPE_MOVIE, LIBRARY_TYPE_SERIES};
pub use media_items::{
    get_audio_track, get_media_file, get_media_item, get_season, get_subtitle_file,
    list_audio_tracks_for_media_file, list_episodes_for_season, list_media_files_for_media_item,
    list_media_items_for_library, list_seasons_for_series, list_subtitle_files_for_media_file,
    refresh_media_item_metadata, series_episode_outline_for_media_item,
    ListMediaItemsForLibraryInput, ListMediaItemsForLibraryOutput, SeriesEpisodeOutline,
    SeriesEpisodeOutlineEpisode, SeriesEpisodeOutlineSeason,
};
pub use metadata::{
    apply_remote_metadata, build_metadata_provider, normalize_base_url,
    normalize_metadata_language, normalize_optional_value, normalize_required_value,
    MetadataLookup, MetadataLookupCache, MetadataProvider, MetadataProviderConfig,
    NullMetadataProvider, RemoteCastMember, RemoteMetadata, RemoteMetadataSearchResult,
    RemoteSeriesEpisode, RemoteSeriesEpisodeOutline, RemoteSeriesSeason, TmdbMetadataProvider,
    TmdbMetadataProviderConfig, DEFAULT_OMDB_API_BASE_URL, DEFAULT_TMDB_API_BASE_URL,
    DEFAULT_TMDB_IMAGE_BASE_URL, DEFAULT_TMDB_LANGUAGE, SUPPORTED_TMDB_LANGUAGES,
};
pub use metadata_match::{
    apply_media_item_metadata_match, search_media_item_metadata_matches, ApplyMetadataMatchInput,
    MetadataMatchCandidate, SearchMetadataMatchesInput,
};
pub use playback_header::{get_media_item_playback_header, MediaItemPlaybackHeader};
pub use playback_progress::{
    get_playback_progress_for_media_item, list_continue_watching,
    update_playback_progress_for_media_item, UpdatePlaybackProgressInput,
};
pub use scan_jobs::{
    enqueue_library_scan, execute_scan_job, execute_scan_job_with_cancellation,
    get_scan_job_for_library, list_scan_jobs_for_library, EnqueueLibraryScanResult,
    ExecuteScanJobOutcome, ScanJobEvent, ScanJobItemProgressUpdate, ScanJobProgressUpdate,
};
pub use users::{
    bootstrap_admin, bootstrap_required, change_own_password, create_user, delete_user, get_user,
    get_user_by_session_token, list_users, login, logout, replace_user_library_access,
    reset_user_password, update_own_profile, update_user, AuthSession, BootstrapAdminInput,
    ChangeOwnPasswordInput, CreateUserInput, LoginInput, ResetUserPasswordInput,
    UpdateOwnProfileInput, UpdateUserInput, UpdateUserLibraryAccessInput,
};
pub use watch_history::list_watch_history;
