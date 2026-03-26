import type {
  BootstrapAdminInput,
  BootstrapStatus,
  ContinueWatchingItem,
  CreateLibraryInput,
  CreateUserInput,
  Episode,
  EpisodeOutline,
  Library,
  LibraryDetail,
  LoginInput,
  MediaFile,
  MediaItem,
  MediaItemDetail,
  MediaItemListResponse,
  MediaItemPlaybackHeader,
  MetadataSearchResult,
  PlaybackProgress,
  ScanJob,
  Season,
  ServerRootPathOption,
  UserAccount,
  WatchHistoryItem,
} from './types'

interface ListMediaItemsParams {
  page: number
  pageSize: number
  query?: string
  year?: number
}

const API_PREFIX = '/api'

function withApiPrefix(path: string) {
  return `${API_PREFIX}${path}`
}

class ApiError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    credentials: 'same-origin',
    headers: {
      'Content-Type': 'application/json',
      ...init?.headers,
    },
    ...init,
  })

  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`

    try {
      const payload = (await response.json()) as { error?: string }
      if (payload.error) {
        message = payload.error
      }
    } catch {
      // Ignore non-JSON error bodies.
    }

    throw new ApiError(response.status, message)
  }

  if (response.status === 204) {
    return undefined as T
  }

  return (await response.json()) as T
}

export function getBootstrapStatus() {
  return requestJson<BootstrapStatus>(withApiPrefix('/auth/bootstrap-status'))
}

export function bootstrapAdmin(input: BootstrapAdminInput) {
  return requestJson<UserAccount>(withApiPrefix('/auth/bootstrap-admin'), {
    method: 'POST',
    body: JSON.stringify(input),
  })
}

export function login(input: LoginInput) {
  return requestJson<UserAccount>(withApiPrefix('/auth/login'), {
    method: 'POST',
    body: JSON.stringify(input),
  })
}

export function logout() {
  return requestJson<void>(withApiPrefix('/auth/logout'), {
    method: 'POST',
  })
}

export function getCurrentUser() {
  return requestJson<UserAccount>(withApiPrefix('/auth/me'))
}

export function listLibraries() {
  return requestJson<Library[]>(withApiPrefix('/libraries'))
}

export function createLibrary(input: CreateLibraryInput) {
  return requestJson<Library>(withApiPrefix('/libraries'), {
    method: 'POST',
    body: JSON.stringify(input),
  })
}

export function listUsers() {
  return requestJson<UserAccount[]>(withApiPrefix('/users'))
}

export function createUser(input: CreateUserInput) {
  return requestJson<UserAccount>(withApiPrefix('/users'), {
    method: 'POST',
    body: JSON.stringify(input),
  })
}

export function deleteLibrary(libraryId: number) {
  return requestJson<void>(withApiPrefix(`/libraries/${libraryId}`), {
    method: 'DELETE',
  })
}

export function listServerRootPaths() {
  return requestJson<ServerRootPathOption[]>(withApiPrefix('/server/root-paths'))
}

export function getLibrary(libraryId: number) {
  return requestJson<LibraryDetail>(withApiPrefix(`/libraries/${libraryId}`))
}

export function listLibraryMediaItems(libraryId: number, params: ListMediaItemsParams) {
  const searchParams = new URLSearchParams({
    page: String(params.page),
    page_size: String(params.pageSize),
  })

  if (params.query) {
    searchParams.set('query', params.query)
  }

  if (typeof params.year === 'number' && Number.isFinite(params.year)) {
    searchParams.set('year', String(params.year))
  }

  return requestJson<MediaItemListResponse>(
    withApiPrefix(`/libraries/${libraryId}/media-items?${searchParams.toString()}`),
  )
}

export function scanLibrary(libraryId: number) {
  return requestJson<ScanJob>(withApiPrefix(`/libraries/${libraryId}/scan`), {
    method: 'POST',
  })
}

export function listContinueWatching(limit = 12) {
  const searchParams = new URLSearchParams({
    limit: String(limit),
  })

  return requestJson<ContinueWatchingItem[]>(
    withApiPrefix(`/playback-progress/continue-watching?${searchParams.toString()}`),
  )
}

export function listWatchHistory(limit = 50) {
  const searchParams = new URLSearchParams({
    limit: String(limit),
  })

  return requestJson<WatchHistoryItem[]>(withApiPrefix(`/watch-history?${searchParams.toString()}`))
}

export function getMediaItemPlaybackProgress(mediaItemId: number) {
  return requestJson<PlaybackProgress | null>(
    withApiPrefix(`/media-items/${mediaItemId}/playback-progress`),
  )
}

export function updateMediaItemPlaybackProgress(
  mediaItemId: number,
  input: {
    media_file_id: number
    position_seconds: number
    duration_seconds?: number
    is_finished?: boolean
  },
) {
  return requestJson<PlaybackProgress>(
    withApiPrefix(`/media-items/${mediaItemId}/playback-progress`),
    {
      method: 'PUT',
      body: JSON.stringify(input),
    },
  )
}

export function getMediaItem(mediaItemId: number) {
  return requestJson<MediaItemDetail>(withApiPrefix(`/media-items/${mediaItemId}`))
}

export function searchMediaItemMetadata(
  mediaItemId: number,
  params: { query: string; year?: number },
) {
  const searchParams = new URLSearchParams({
    query: params.query,
  })

  if (typeof params.year === 'number' && Number.isFinite(params.year)) {
    searchParams.set('year', String(params.year))
  }

  return requestJson<MetadataSearchResult[]>(
    withApiPrefix(`/media-items/${mediaItemId}/metadata-search?${searchParams.toString()}`),
  )
}

export function applyMediaItemMetadataMatch(mediaItemId: number, providerItemId: number) {
  return requestJson<MediaItem>(withApiPrefix(`/media-items/${mediaItemId}/metadata-match`), {
    method: 'POST',
    body: JSON.stringify({
      provider_item_id: providerItemId,
    }),
  })
}

export function getMediaItemPlaybackHeader(mediaItemId: number) {
  return requestJson<MediaItemPlaybackHeader>(
    withApiPrefix(`/media-items/${mediaItemId}/playback-header`),
  )
}

export function listMediaItemFiles(mediaItemId: number) {
  return requestJson<MediaFile[]>(withApiPrefix(`/media-items/${mediaItemId}/files`))
}

export function listMediaItemSeasons(mediaItemId: number) {
  return requestJson<Season[]>(withApiPrefix(`/media-items/${mediaItemId}/seasons`))
}

export function getMediaItemEpisodeOutline(mediaItemId: number) {
  return requestJson<EpisodeOutline>(withApiPrefix(`/media-items/${mediaItemId}/episode-outline`))
}

export function listSeasonEpisodes(seasonId: number) {
  return requestJson<Episode[]>(withApiPrefix(`/seasons/${seasonId}/episodes`))
}

export function refreshMediaItemMetadata(mediaItemId: number) {
  return requestJson<MediaItem>(withApiPrefix(`/media-items/${mediaItemId}/refresh-metadata`), {
    method: 'POST',
  })
}

export function mediaFileStreamUrl(mediaFileId: number) {
  return withApiPrefix(`/media-files/${mediaFileId}/stream`)
}

export { ApiError }
