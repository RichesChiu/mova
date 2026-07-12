mod libraries;
mod media_cast;
mod media_items;
mod playback_progress;
mod pool;
mod scan_jobs;
mod users;

pub use libraries::{
    create_library, delete_library, get_library, list_libraries, list_library_artwork_paths,
    list_referenced_artwork_paths, update_library, CreateLibraryParams, UpdateLibraryParams,
};
pub use media_cast::{
    delete_media_item_cast_cache, get_media_item_cast_cache, list_media_item_cast_members,
    replace_media_item_cast, MediaItemCastCacheEntry, ReplaceMediaItemCastMember,
    ReplaceMediaItemCastParams,
};
pub use media_items::{
    count_media_items_for_library, delete_library_media_by_file_path,
    delete_library_media_by_path_prefix, delete_series_episode_outline_cache, get_audio_track,
    get_library_media_type_counts, get_media_file, get_media_item, get_media_item_playback_header,
    get_season, get_series_episode_outline_cache, get_subtitle_file, global_search,
    list_audio_tracks_for_media_file, list_episodes_for_season,
    list_existing_media_metadata_for_file_paths, list_library_media_file_paths,
    list_media_files_for_media_item, list_media_items_for_library,
    list_recently_added_media_items_by_library, list_seasons_for_series,
    list_series_media_item_ids_for_library, list_subtitle_files_for_media_file,
    replace_audio_tracks_for_media_file, replace_subtitle_files_for_media_file, sync_library_media,
    sync_library_media_best_effort, sync_library_media_changes, update_media_file_metadata,
    update_media_item_metadata, update_season_intro_markers, update_series_episode_metadata,
    update_series_season_metadata, upsert_library_media_entry_by_file_path,
    upsert_series_episode_outline_cache, CreateAudioTrackParams, CreateMediaEntryParams,
    CreateSubtitleTrackParams, ExistingMediaMetadataSummary, GlobalSearchParams,
    GlobalSearchResult, LibraryMediaTypeCounts, ListMediaItemsForLibraryParams,
    ListMediaItemsForLibraryResult, MediaItemPlaybackHeader, RecentlyAddedLibraryMediaItems,
    SeriesEpisodeOutlineCacheEntry, SyncLibraryMediaBestEffortOutcome,
    UpdateMediaFileMetadataParams, UpdateMediaItemMetadataParams,
    UpdateSeriesEpisodeMetadataParams, UpdateSeriesSeasonMetadataParams,
    UpsertSeriesEpisodeOutlineCacheParams,
};
pub use playback_progress::{
    get_playback_progress_for_media_item, list_continue_watching,
    list_playback_progress_for_media_items, upsert_playback_progress, UpsertPlaybackProgressParams,
};
pub use pool::{connect, migrate, ping, DatabaseSettings};
pub use scan_jobs::{
    create_scan_job, enqueue_scan_job, fail_incomplete_scan_jobs, finalize_scan_job,
    get_latest_scan_job_for_library, get_scan_job, list_scan_jobs_for_library,
    mark_scan_job_running, update_scan_job_progress, CreateScanJobParams, EnqueueScanJobResult,
};
pub use users::{
    count_admin_users, count_enabled_admin_users, create_native_client_session, create_session,
    create_user, delete_session, delete_sessions_for_user, delete_user,
    get_native_client_session_by_refresh_token_hash, get_primary_admin_user_id,
    get_used_native_refresh_token, get_user, get_user_authentication_record,
    get_user_by_native_access_token_hash, get_user_by_session_token, get_user_by_username,
    list_library_ids_for_user, list_users, replace_user_library_access,
    revoke_native_client_session, revoke_native_client_session_by_refresh_token_hash,
    revoke_native_client_sessions_for_user, rotate_native_client_session_tokens,
    touch_native_client_session, update_user, update_user_nickname, update_user_password,
    CreateNativeClientSessionParams, CreateSessionParams, CreateUserParams,
    NativeClientSessionUser, UpdateUserParams, UsedNativeRefreshToken, UserAuthenticationRecord,
};
