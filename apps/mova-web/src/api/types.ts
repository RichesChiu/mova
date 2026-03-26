export type LibraryType = 'mixed' | 'movie' | 'series'
export type UserRole = 'admin' | 'viewer'

export interface Library {
  id: number
  name: string
  description: string | null
  library_type: LibraryType
  metadata_language: string
  root_path: string
  is_enabled: boolean
  created_at: string
  updated_at: string
}

export interface ScanJob {
  id: number
  library_id: number
  status: string
  total_files: number
  scanned_files: number
  created_at: string
  started_at: string | null
  finished_at: string | null
  error_message: string | null
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

export interface MediaItem {
  id: number
  library_id: number
  media_type: MediaType
  title: string
  source_title: string
  original_title: string | null
  sort_title: string | null
  year: number | null
  overview: string | null
  poster_path: string | null
  backdrop_path: string | null
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

export interface MediaItemDetail extends MediaItem {
  cast: MediaCastMember[]
}

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

export interface Season {
  id: number
  series_id: number
  season_number: number
  title: string | null
  overview: string | null
  poster_path: string | null
  backdrop_path: string | null
  episode_count: number
  created_at: string
  updated_at: string
}

export interface Episode {
  id: number
  media_item_id: number
  series_id: number
  season_id: number
  episode_number: number
  title: string
  overview: string | null
  poster_path: string | null
  backdrop_path: string | null
  created_at: string
  updated_at: string
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
  backdrop_path: string | null
  episodes: EpisodeOutlineEpisode[]
}

export interface EpisodeOutlineEpisode {
  episode_number: number
  title: string
  overview: string | null
  poster_path: string | null
  backdrop_path: string | null
  media_item_id: number | null
  is_available: boolean
  playback_progress: EpisodeOutlinePlaybackProgress | null
}

export interface EpisodeOutlinePlaybackProgress {
  position_seconds: number
  duration_seconds: number | null
  is_finished: boolean
}

export interface MediaFile {
  id: number
  media_item_id: number
  file_path: string
  container: string | null
  file_size: number
  duration_seconds: number | null
  video_codec: string | null
  audio_codec: string | null
  width: number | null
  height: number | null
  bitrate: number | null
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

export interface WatchHistory {
  id: number
  media_item_id: number
  media_file_id: number
  position_seconds: number
  duration_seconds: number | null
  started_at: string
  last_watched_at: string
  ended_at: string | null
  completed_at: string | null
  is_finished: boolean
}

export interface WatchHistoryItem {
  media_item: MediaItem
  watch_history: WatchHistory
}

export interface CreateLibraryInput {
  name: string
  description?: string
  library_type: LibraryType
  metadata_language: string
  root_path: string
  is_enabled: boolean
}

export interface ServerRootPathOption {
  path: string
  source: string
}

export interface UserAccount {
  id: number
  username: string
  role: UserRole
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
  password: string
  role: UserRole
  is_enabled: boolean
  library_ids: number[]
}
