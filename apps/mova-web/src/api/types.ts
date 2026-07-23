export type UserRole = 'admin' | 'viewer'

export interface Library {
  id: number
  name: string
  description: string | null
  metadata_language: string
  root_path: string
  created_at: string
  updated_at: string
}

export interface ScanJob {
  id: number
  library_id: number
  status: string
  phase?: string | null
  total_files: number
  scanned_files: number
  local_analyzed_files: number
  local_committed_files: number
  remote_completed_files: number
  progress_percent: number
  created_at: string
  started_at: string | null
  finished_at: string | null
  error_message: string | null
}

export interface ScanNotificationIssue {
  item_key: string
  media_type: string
  title: string
  year: number | null
  file_count: number
  metadata_status: MetadataStatus
  metadata_failure_reason: string | null
  failure_detail: string | null
  probe_warning_count: number
  probe_warning_file_path: string | null
  probe_warning_detail: string | null
}

export type NotificationCategory = 'scan' | 'system' | 'library' | 'account' | string
export type NotificationSeverity = 'info' | 'success' | 'warning' | 'error' | string

export interface ScanNotificationPayload {
  scan_job_id: number
  library_id: number
  library_name: string
  status: string
  total_files: number
  reused_files: number
  matched_files: number
  unmatched_files: number
  failed_files: number
  skipped_files: number
  probe_warning_count: number
  issue_count: number
  error_message: string | null
  issues: ScanNotificationIssue[]
}

export interface NotificationItem {
  id: number
  category: NotificationCategory
  notification_type: string
  severity: NotificationSeverity
  library_id: number | null
  payload: unknown
  is_read: boolean
  read_at: string | null
  created_at: string
}

export interface NotificationFeed {
  items: NotificationItem[]
  total_unread: number
  unread_by_category: Record<string, number>
}

export interface PlaybackProgress {
  id: number
  media_item_id: number
  media_file_id: number
  position_seconds: number
  duration_seconds: number | null
  last_watched_at: string
  is_finished: boolean
}

export interface LibraryDetail extends Library {
  media_count: number
  movie_count: number
  series_count: number
  last_scan: ScanJob | null
}

export type MediaType = 'movie' | 'series' | 'episode' | string
export type MetadataStatus = 'matched' | 'unmatched' | 'failed' | 'skipped' | string

export interface MediaRating {
  source: string
  kind: string
  score: number
  scale: number
  rating_count: number | null
  attributes: Record<string, unknown>
  fetched_at: string
}

export interface MediaItem {
  id: number
  library_id: number
  media_type: MediaType
  title: string
  source_title: string
  original_title: string | null
  sort_title: string | null
  metadata_provider: string | null
  metadata_provider_item_id: number | null
  metadata_status: MetadataStatus
  metadata_failure_reason: string | null
  remote_media_type: string | null
  year: number | null
  ratings: MediaRating[]
  country?: string | null
  genres?: string | null
  studio?: string | null
  overview: string | null
  poster_path: string | null
  backdrop_path: string | null
  logo_path: string | null
  created_at: string
  updated_at: string
}

export interface MediaCastMember {
  person_id: number | null
  sort_order: number
  name: string
  character_name: string | null
  profile_path: string | null
}

export interface MediaItemDetail extends MediaItem {}

export interface MetadataSearchResult {
  provider_item_id: number
  title: string
  original_title: string | null
  year: number | null
  overview: string | null
  poster_path: string | null
  backdrop_path: string | null
}

export interface MediaItemPlaybackHeader {
  media_item_id: number
  library_id: number
  media_type: MediaType
  series_media_item_id: number | null
  title: string
  original_title: string | null
  year: number | null
  logo_path: string | null
  season_number: number | null
  episode_number: number | null
  episode_title: string | null
}

export interface MediaItemListResponse {
  items: MediaItem[]
  total: number
  page: number
  page_size: number
}

export interface RecentlyAddedLibraryMediaItems {
  library: Library
  items: MediaItem[]
  total: number
}

export interface HomeLibrary {
  library: LibraryDetail
  preview_items: MediaItem[]
}

export interface HomeRealtimeState {
  protocol_version: number
  server_epoch: string
  resources: Record<string, number>
}

export interface HomeResponse {
  current_user: UserAccount
  libraries: HomeLibrary[]
  recently_added: RecentlyAddedLibraryMediaItems[]
  continue_watching: ContinueWatchingItem[]
  realtime: HomeRealtimeState
}

export interface RealtimeState {
  protocol_version: number
  server_epoch: string
  resources: Record<string, number>
  active_scans: ScanJob[]
}

export type GlobalSearchResultKind = 'media_item' | 'episode' | string

export interface GlobalSearchResult {
  kind: GlobalSearchResultKind
  library_id: number
  library_name: string
  media_item_id: number
  series_media_item_id: number | null
  media_type: MediaType
  title: string
  subtitle: string | null
  year: number | null
  overview: string | null
  poster_path: string | null
  backdrop_path: string | null
  season_number: number | null
  episode_number: number | null
}

export interface EpisodeOutline {
  seasons: EpisodeOutlineSeason[]
}

export interface EpisodeOutlineSeason {
  season_id: number | null
  season_number: number
  title: string | null
  year: number | null
  overview: string | null
  poster_path: string | null
  intro_start_seconds: number | null
  intro_end_seconds: number | null
  episodes: EpisodeOutlineEpisode[]
}

export interface EpisodeOutlineEpisode {
  episode_number: number
  title: string
  overview: string | null
  poster_path: string | null
  backdrop_path: string | null
  intro_start_seconds: number | null
  intro_end_seconds: number | null
  media_item_id: number | null
  is_available: boolean
  playback_progress: EpisodeOutlinePlaybackProgress | null
}

export interface EpisodeOutlinePlaybackProgress {
  position_seconds: number
  duration_seconds: number | null
  last_watched_at: string
  is_finished: boolean
}

export interface MediaFile {
  id: number
  media_item_id: number
  file_path: string
  container: string | null
  file_size: number
  duration_seconds: number | null
  video_title?: string | null
  video_codec: string | null
  video_profile?: string | null
  video_level?: string | null
  audio_codec: string | null
  width: number | null
  height: number | null
  bitrate: number | null
  video_bitrate?: number | null
  video_frame_rate?: number | null
  video_aspect_ratio?: string | null
  video_scan_type?: string | null
  video_color_primaries?: string | null
  video_color_space?: string | null
  video_color_transfer?: string | null
  video_bit_depth?: number | null
  video_pixel_format?: string | null
  video_reference_frames?: number | null
  technical_tags: string[]
  scan_hash: string | null
  created_at: string
  updated_at: string
}

export interface SubtitleFile {
  id: number
  media_file_id: number
  source_kind: string
  file_path: string | null
  stream_index: number | null
  language: string | null
  subtitle_format: string
  label: string | null
  is_default: boolean
  is_forced: boolean
  is_hearing_impaired: boolean
  created_at: string
  updated_at: string
}

export interface AudioTrack {
  id: number
  media_file_id: number
  stream_index: number
  language: string | null
  audio_codec: string | null
  label: string | null
  channel_layout?: string | null
  channels?: number | null
  bitrate?: number | null
  sample_rate?: number | null
  is_default: boolean
  created_at: string
  updated_at: string
}

export interface ContinueWatchingItem {
  media_item: MediaItem
  playback_progress: PlaybackProgress
  season_number: number | null
  episode_number: number | null
  episode_title: string | null
  episode_overview: string | null
  episode_poster_path: string | null
  episode_backdrop_path: string | null
}

export interface CreateLibraryInput {
  name: string
  description?: string
  metadata_language: string
  root_path: string
}

export interface UpdateLibraryInput {
  name?: string
  description?: string | null
  metadata_language?: string
}

export interface ServerMediaDirectoryNode {
  name: string
  path: string
  children: ServerMediaDirectoryNode[]
}

export interface UserAccount {
  id: number
  username: string
  nickname: string
  role: UserRole
  is_primary_admin: boolean
  is_enabled: boolean
  library_ids: number[]
  created_at: string
  updated_at: string
}

export interface BootstrapStatus {
  bootstrap_required: boolean
}

export interface LoginInput {
  username: string
  password: string
}

export interface BootstrapAdminInput {
  username: string
  password: string
}

export interface CreateUserInput {
  username: string
  nickname?: string
  password: string
  role: UserRole
  is_enabled: boolean
  library_ids: number[]
}

export interface UpdateUserInput {
  nickname?: string
  role?: UserRole
  is_enabled?: boolean
  library_ids?: number[]
}

export interface ChangeOwnPasswordInput {
  current_password: string
  new_password: string
}

export interface UpdateOwnProfileInput {
  nickname: string
}
