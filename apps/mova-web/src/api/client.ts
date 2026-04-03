import type {
  BootstrapAdminInput,
  BootstrapStatus,
  ChangeOwnPasswordInput,
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
  ServerMediaDirectoryNode,
  SubtitleFile,
  UpdateUserInput,
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

interface ApiEnvelope<T> {
  code: number
  data: T
  message: string
}

type PlaybackProgressUpdateInput = {
  media_file_id: number
  position_seconds: number
  duration_seconds?: number
  is_finished?: boolean
}

const withApiPrefix = (path: string) => `${API_PREFIX}${path}`

class ApiError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

const isApiEnvelope = <T>(value: unknown): value is ApiEnvelope<T> =>
  typeof value === 'object' &&
  value !== null &&
  'code' in value &&
  'message' in value &&
  'data' in value

const requestJson = async <T>(path: string, init?: RequestInit): Promise<T> => {
  const response = await fetch(path, {
    credentials: 'same-origin',
    headers: {
      'Content-Type': 'application/json',
      ...init?.headers,
    },
    ...init,
  })

  let payload: unknown = null

  if (response.status !== 204) {
    try {
      payload = await response.json()
    } catch {
      payload = null
    }
  }

  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`

    if (isApiEnvelope(payload)) {
      message = payload.message
    } else if (
      typeof payload === 'object' &&
      payload !== null &&
      'error' in payload &&
      typeof payload.error === 'string'
    ) {
      // Keep the old parser during the transition so stale backend processes still surface errors.
      message = payload.error
    }

    throw new ApiError(response.status, message)
  }

  if (response.status === 204) {
    return undefined as T
  }

  if (isApiEnvelope<T>(payload)) {
    return payload.data as T
  }

  return payload as T
}

export const getBootstrapStatus = () =>
  requestJson<BootstrapStatus>(withApiPrefix('/auth/bootstrap-status'))

export const bootstrapAdmin = (input: BootstrapAdminInput) =>
  requestJson<UserAccount>(withApiPrefix('/auth/bootstrap-admin'), {
    method: 'POST',
    body: JSON.stringify(input),
  })

export const login = (input: LoginInput) =>
  requestJson<UserAccount>(withApiPrefix('/auth/login'), {
    method: 'POST',
    body: JSON.stringify(input),
  })

export const logout = () =>
  requestJson<void>(withApiPrefix('/auth/logout'), {
    method: 'POST',
  })

export const getCurrentUser = () => requestJson<UserAccount>(withApiPrefix('/auth/me'))

export const changeOwnPassword = (input: ChangeOwnPasswordInput) =>
  requestJson<UserAccount>(withApiPrefix('/auth/password'), {
    method: 'PUT',
    body: JSON.stringify(input),
  })

export const listLibraries = () => requestJson<Library[]>(withApiPrefix('/libraries'))

export const createLibrary = (input: CreateLibraryInput) =>
  requestJson<Library>(withApiPrefix('/libraries'), {
    method: 'POST',
    body: JSON.stringify(input),
  })

export const listUsers = () => requestJson<UserAccount[]>(withApiPrefix('/users'))

export const createUser = (input: CreateUserInput) =>
  requestJson<UserAccount>(withApiPrefix('/users'), {
    method: 'POST',
    body: JSON.stringify(input),
  })

export const updateUser = (userId: number, input: UpdateUserInput) =>
  requestJson<UserAccount>(withApiPrefix(`/users/${userId}`), {
    method: 'PATCH',
    body: JSON.stringify(input),
  })

export const deleteUser = (userId: number) =>
  requestJson<void>(withApiPrefix(`/users/${userId}`), {
    method: 'DELETE',
  })

export const deleteLibrary = (libraryId: number) =>
  requestJson<void>(withApiPrefix(`/libraries/${libraryId}`), {
    method: 'DELETE',
  })

export const getServerMediaTree = () =>
  requestJson<ServerMediaDirectoryNode | null>(withApiPrefix('/server/media-tree'))

export const getLibrary = (libraryId: number) =>
  requestJson<LibraryDetail>(withApiPrefix(`/libraries/${libraryId}`))

export const listLibraryMediaItems = (libraryId: number, params: ListMediaItemsParams) => {
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

export const scanLibrary = (libraryId: number) =>
  requestJson<ScanJob>(withApiPrefix(`/libraries/${libraryId}/scan`), {
    method: 'POST',
  })

export const listContinueWatching = (limit = 12) => {
  const searchParams = new URLSearchParams({
    limit: String(limit),
  })

  return requestJson<ContinueWatchingItem[]>(
    withApiPrefix(`/playback-progress/continue-watching?${searchParams.toString()}`),
  )
}

export const listWatchHistory = (limit = 50) => {
  const searchParams = new URLSearchParams({
    limit: String(limit),
  })

  return requestJson<WatchHistoryItem[]>(withApiPrefix(`/watch-history?${searchParams.toString()}`))
}

export const getMediaItemPlaybackProgress = (mediaItemId: number) =>
  requestJson<PlaybackProgress | null>(
    withApiPrefix(`/media-items/${mediaItemId}/playback-progress`),
  )

export const updateMediaItemPlaybackProgress = (
  mediaItemId: number,
  input: PlaybackProgressUpdateInput,
) => {
  return requestJson<PlaybackProgress>(
    withApiPrefix(`/media-items/${mediaItemId}/playback-progress`),
    {
      method: 'PUT',
      body: JSON.stringify(input),
    },
  )
}

export const flushMediaItemPlaybackProgress = (
  mediaItemId: number,
  input: PlaybackProgressUpdateInput,
) => {
  const path = withApiPrefix(`/media-items/${mediaItemId}/playback-progress`)
  const body = JSON.stringify(input)

  // 页面关闭或切后台时，优先用 beacon/keepalive 把最后一笔进度发出去，
  // 避免只依赖暂停事件导致离场进度丢失。
  if (typeof navigator !== 'undefined' && typeof navigator.sendBeacon === 'function') {
    const sent = navigator.sendBeacon(path, new Blob([body], { type: 'application/json' }))
    if (sent) {
      return
    }
  }

  void fetch(path, {
    method: 'PUT',
    credentials: 'same-origin',
    headers: {
      'Content-Type': 'application/json',
    },
    body,
    keepalive: true,
  })
}

export const getMediaItem = (mediaItemId: number) =>
  requestJson<MediaItemDetail>(withApiPrefix(`/media-items/${mediaItemId}`))

export const searchMediaItemMetadata = (
  mediaItemId: number,
  params: { query: string; year?: number },
) => {
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

export const applyMediaItemMetadataMatch = (mediaItemId: number, providerItemId: number) =>
  requestJson<MediaItem>(withApiPrefix(`/media-items/${mediaItemId}/metadata-match`), {
    method: 'POST',
    body: JSON.stringify({
      provider_item_id: providerItemId,
    }),
  })

export const getMediaItemPlaybackHeader = (mediaItemId: number) =>
  requestJson<MediaItemPlaybackHeader>(withApiPrefix(`/media-items/${mediaItemId}/playback-header`))

export const listMediaItemFiles = (mediaItemId: number) =>
  requestJson<MediaFile[]>(withApiPrefix(`/media-items/${mediaItemId}/files`))

export const listMediaFileSubtitles = (mediaFileId: number) =>
  requestJson<SubtitleFile[]>(withApiPrefix(`/media-files/${mediaFileId}/subtitles`))

export const listMediaItemSeasons = (mediaItemId: number) =>
  requestJson<Season[]>(withApiPrefix(`/media-items/${mediaItemId}/seasons`))

export const getMediaItemEpisodeOutline = (mediaItemId: number) =>
  requestJson<EpisodeOutline>(withApiPrefix(`/media-items/${mediaItemId}/episode-outline`))

export const listSeasonEpisodes = (seasonId: number) =>
  requestJson<Episode[]>(withApiPrefix(`/seasons/${seasonId}/episodes`))

export const refreshMediaItemMetadata = (mediaItemId: number) =>
  requestJson<MediaItem>(withApiPrefix(`/media-items/${mediaItemId}/refresh-metadata`), {
    method: 'POST',
  })

export const mediaFileStreamUrl = (mediaFileId: number) =>
  withApiPrefix(`/media-files/${mediaFileId}/stream`)

export const subtitleFileStreamUrl = (subtitleFileId: number) =>
  withApiPrefix(`/subtitle-files/${subtitleFileId}/stream`)

export { ApiError }
