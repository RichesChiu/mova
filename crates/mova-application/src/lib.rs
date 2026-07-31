mod access;
mod cache;
mod error;
mod file_sync;
mod home;
mod intro_detection;
mod libraries;
mod media_cast;
mod media_classification;
mod media_enrichment;
mod media_items;
mod metadata;
mod metadata_match;
mod notifications;
mod playback_header;
mod playback_progress;
mod scan_jobs;
mod tmdb_revalidation;
mod users;

pub use access::{
    authorize_library, authorize_media_file_with_library, authorize_media_item_with_library,
    authorize_season_with_library,
};
pub use cache::{
    cache_temp_path, commit_cache_file, is_nonempty_cache_file, library_artwork_cache_dir,
    library_audio_track_cache_dir, library_cache_dir, library_subtitle_cache_path, lock_cache_path,
    remove_library_cache, write_cache_file_atomically, CacheTempFileGuard,
};
pub use error::{
    ApplicationError, ApplicationResult, AuthTokenErrorCode, BusinessError, BusinessErrorKind,
    BusinessErrorParams,
};
pub use file_sync::{reconcile_library_inventory, sync_library_filesystem_changes};
pub use home::{get_home_snapshot, HomeLibrarySnapshot, HomeSnapshot};
pub use libraries::{
    create_library, delete_library, get_library, get_library_detail,
    library_metadata_language_will_change, list_libraries, update_library, CreateLibraryInput,
    UpdateLibraryInput, UpdateLibraryOutput,
};
pub use media_cast::{
    ensure_media_item_cast, invalidate_media_item_cast_cache, list_media_item_cast,
};
pub use media_classification::{LIBRARY_TYPE_MOVIE, LIBRARY_TYPE_SERIES};
pub use media_items::{
    get_audio_track, get_media_file, get_media_item, get_season, get_subtitle_file, global_search,
    list_audio_tracks_for_media_file, list_media_files_for_media_item,
    list_media_items_for_library, list_recently_added_media_items_by_library,
    list_subtitle_files_for_media_file, refresh_media_item_metadata,
    series_episode_outline_for_media_item, GlobalSearchInput, GlobalSearchResult,
    ListMediaItemsForLibraryInput, ListMediaItemsForLibraryOutput, ListRecentlyAddedByLibraryInput,
    RecentlyAddedLibraryMediaItems, SeriesEpisodeOutline, SeriesEpisodeOutlineEpisode,
    SeriesEpisodeOutlineSeason,
};
pub use metadata::{
    apply_remote_metadata, build_metadata_provider, normalize_base_url,
    normalize_metadata_language, normalize_optional_value, normalize_required_value,
    MetadataLookup, MetadataLookupCache, MetadataProvider, MetadataProviderConfig,
    NullMetadataProvider, RemoteCastMember, RemoteMetadata, RemoteMetadataSearchResult,
    RemoteSeriesEpisode, RemoteSeriesEpisodeOutline, RemoteSeriesSeason, TmdbMetadataProvider,
    TmdbMetadataProviderConfig, DEFAULT_TMDB_API_BASE_URL, DEFAULT_TMDB_IMAGE_BASE_URL,
    DEFAULT_TMDB_LANGUAGE, SUPPORTED_TMDB_LANGUAGES,
};
pub use metadata_match::{
    apply_media_item_metadata_match, search_media_item_metadata_matches, ApplyMetadataMatchInput,
    MetadataMatchCandidate, SearchMetadataMatchesInput,
};
pub use notifications::{list_notifications, mark_all_notifications_read, mark_notification_read};
pub use playback_header::{get_media_item_playback_header, MediaItemPlaybackHeader};
pub use playback_progress::{
    get_playback_progress_for_media_item, list_continue_watching,
    update_playback_progress_for_media_item, UpdatePlaybackProgressInput,
};
pub use scan_jobs::{
    enqueue_library_scan, execute_scan_job_with_cancellation, get_scan_job_for_library,
    list_scan_jobs_for_library, EnqueueLibraryScanResult, ExecuteScanJobOutcome, ScanJobEvent,
    ScanJobItemProgressUpdate, ScanJobProgressUpdate,
};
pub use tmdb_revalidation::{
    execute_tmdb_artwork_cleanup, execute_tmdb_artwork_orphan_sweep,
    execute_tmdb_metadata_revalidation, TmdbMetadataRevalidationInput,
    TmdbMetadataRevalidationOutcome,
};
pub use users::{
    bootstrap_admin, bootstrap_required, change_own_password, create_user, delete_user,
    get_native_access_session, get_user, get_user_by_native_access_token,
    get_user_by_session_token, get_user_session, list_users, login, login_native_client, logout,
    logout_native_client_access_token, logout_native_client_refresh_token,
    refresh_native_client_session, reset_user_password, update_own_profile, update_user,
    AuthSession, AuthenticatedSession, BootstrapAdminInput, ChangeOwnPasswordInput,
    CreateUserInput, LoginInput, NativeAuthSession, NativeClientLoginInput,
    RefreshNativeClientSessionInput, ResetUserPasswordInput, UpdateOwnProfileInput,
    UpdateUserInput,
};
