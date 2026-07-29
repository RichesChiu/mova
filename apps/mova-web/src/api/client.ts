import { type ApiErrorParams, errorCodeForHttpStatus, localizeApiError } from '../lib/api-error'
import type {
  AudioTrack,
  BootstrapAdminInput,
  BootstrapStatus,
  ChangeOwnPasswordInput,
  ContinueWatchingItem,
  CreateLibraryInput,
  CreateUserInput,
  EpisodeOutline,
  GlobalSearchResult,
  HomeResponse,
  Library,
  LibraryDetail,
  LoginInput,
  MediaCastMember,
  MediaFile,
  MediaItem,
  MediaItemDetail,
  MediaItemListResponse,
  MediaItemPlaybackHeader,
  MetadataSearchResult,
  NotificationFeed,
  PlaybackProgress,
  RealtimeState,
  RecentlyAddedLibraryMediaItems,
  ScanJob,
  ServerMediaDirectoryNode,
  SubtitleFile,
  UpdateLibraryInput,
  UpdateOwnProfileInput,
  UpdateUserInput,
  UserAccount,
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
  error_code?: string | null
  message: string
  params?: ApiErrorParams | null
}

type MockJsonRequester = <T>(path: string, init?: RequestInit) => Promise<{ data: T } | null>

type PlaybackProgressUpdateInput = {
  media_file_id: number
  position_seconds: number
  duration_seconds?: number
  is_finished?: boolean
}

const withApiPrefix = (path: string) => `${API_PREFIX}${path}`

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

class ApiError extends Error {
  diagnosticMessage: string
  errorCode: string
  params: ApiErrorParams
  status: number

  constructor(
    status: number,
    errorCode: string,
    params: ApiErrorParams,
    diagnosticMessage: string,
  ) {
    super(localizeApiError(errorCode, params, diagnosticMessage))
    this.name = 'ApiError'
    this.status = status
    this.errorCode = errorCode
    this.params = params
    this.diagnosticMessage = diagnosticMessage
  }
}

const isApiEnvelope = <T>(value: unknown): value is ApiEnvelope<T> => {
  if (
    !isRecord(value) ||
    !Object.hasOwn(value, 'data') ||
    !Number.isInteger(value.code) ||
    typeof value.message !== 'string'
  ) {
    return false
  }

  if (
    value.error_code !== undefined &&
    value.error_code !== null &&
    typeof value.error_code !== 'string'
  ) {
    return false
  }

  return value.params === undefined || value.params === null || isRecord(value.params)
}

const diagnosticMessageForFetchError = (error: unknown) => {
  if (error instanceof Error && error.message.trim()) {
    return error.message
  }

  if (typeof error === 'string' && error.trim()) {
    return error
  }

  return 'fetch failed'
}

const requestDevMockJson: MockJsonRequester = import.meta.env.DEV
  ? async <T>(path: string, init?: RequestInit) => {
      const { isMockApiEnabled } = await import('./mock-control')
      if (!isMockApiEnabled()) {
        return null
      }

      const { requestMockJson } = await import('./mock-api')
      return requestMockJson<T>(path, init)
    }
  : async () => null

const requestJson = async <T>(path: string, init?: RequestInit): Promise<T> => {
  const mockResponse = await requestDevMockJson<T>(path, init)
  if (mockResponse) {
    return mockResponse.data
  }

  const headers = new Headers(init?.headers)
  if (init?.body !== undefined && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json')
  }

  let response: Response
  try {
    response = await fetch(path, {
      credentials: 'same-origin',
      ...init,
      headers,
    })
  } catch (error) {
    throw new ApiError(0, 'network_error', {}, diagnosticMessageForFetchError(error))
  }

  let payload: unknown = null

  if (response.status !== 204) {
    try {
      payload = await response.json()
    } catch {
      payload = null
    }
  }

  if (!response.ok) {
    let diagnosticMessage = `${response.status} ${response.statusText}`
    let errorCode = errorCodeForHttpStatus(response.status)
    let params: ApiErrorParams = {}

    if (isApiEnvelope(payload) && payload.code === response.status) {
      diagnosticMessage = payload.message
      errorCode = payload.error_code || errorCode
      params = payload.params ?? {}
    }

    throw new ApiError(response.status, errorCode, params, diagnosticMessage)
  }

  if (response.status === 204) {
    return undefined as T
  }

  if (isApiEnvelope<T>(payload) && payload.code === response.status) {
    return payload.data as T
  }

  throw new ApiError(response.status, 'invalid_response', {}, `invalid API response for ${path}`)
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

export const getHome = () => requestJson<HomeResponse>(withApiPrefix('/home'))

export const getRealtimeState = () => requestJson<RealtimeState>(withApiPrefix('/realtime/state'))

export const updateOwnProfile = (input: UpdateOwnProfileInput) =>
  requestJson<UserAccount>(withApiPrefix('/auth/me'), {
    method: 'PATCH',
    body: JSON.stringify(input),
  })

export const changeOwnPassword = (input: ChangeOwnPasswordInput) =>
  requestJson<UserAccount>(withApiPrefix('/auth/password'), {
    method: 'PUT',
    body: JSON.stringify(input),
  })

export const listLibraries = () => requestJson<Library[]>(withApiPrefix('/libraries'))

export const listNotifications = ({
  category,
  limit = 20,
}: {
  category?: string
  limit?: number
} = {}) => {
  const searchParams = new URLSearchParams({ limit: String(limit) })
  if (category) {
    searchParams.set('category', category)
  }
  return requestJson<NotificationFeed>(withApiPrefix(`/notifications?${searchParams.toString()}`))
}

export const markNotificationRead = (notificationId: number) =>
  requestJson<void>(withApiPrefix(`/notifications/${notificationId}/read`), { method: 'PUT' })

export const markAllNotificationsRead = (category?: string) =>
  requestJson<number>(withApiPrefix('/notifications'), {
    method: 'PUT',
    body: JSON.stringify({ category: category ?? null }),
  })

export const createLibrary = (input: CreateLibraryInput) =>
  requestJson<Library>(withApiPrefix('/libraries'), {
    method: 'POST',
    body: JSON.stringify(input),
  })

export const updateLibrary = (libraryId: number, input: UpdateLibraryInput) =>
  requestJson<Library>(withApiPrefix(`/libraries/${libraryId}`), {
    method: 'PATCH',
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

interface RecentlyAddedParams {
  days?: number
  limit?: number
}

export const listRecentlyAddedByLibrary = ({ days, limit }: RecentlyAddedParams = {}) => {
  const searchParams = new URLSearchParams()

  if (days !== undefined) {
    searchParams.set('days', String(days))
  }
  if (limit !== undefined) {
    searchParams.set('limit', String(limit))
  }

  return requestJson<RecentlyAddedLibraryMediaItems[]>(
    withApiPrefix(
      `/libraries/recently-added${searchParams.size > 0 ? `?${searchParams.toString()}` : ''}`,
    ),
  )
}

export const globalSearch = (query: string, limit = 12) => {
  const searchParams = new URLSearchParams({
    q: query,
    limit: String(limit),
  })

  return requestJson<GlobalSearchResult[]>(withApiPrefix(`/search?${searchParams.toString()}`))
}

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

export const getMediaItemCast = (mediaItemId: number) =>
  requestJson<MediaCastMember[]>(withApiPrefix(`/media-items/${mediaItemId}/cast`))

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

export const applyMediaItemMetadataMatch = (mediaItemId: number, providerItemId: string) =>
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

export const listMediaFileAudioTracks = (mediaFileId: number) =>
  requestJson<AudioTrack[]>(withApiPrefix(`/media-files/${mediaFileId}/audio-tracks`))

export const getMediaItemEpisodeOutline = (mediaItemId: number) =>
  requestJson<EpisodeOutline>(withApiPrefix(`/media-items/${mediaItemId}/episode-outline`))

export const refreshMediaItemMetadata = (mediaItemId: number) =>
  requestJson<MediaItem>(withApiPrefix(`/media-items/${mediaItemId}/refresh-metadata`), {
    method: 'POST',
  })

export const mediaFileStreamUrl = (
  mediaFileId: number,
  options?: {
    audioTrackId?: number | null
  },
) => {
  const searchParams = new URLSearchParams()

  if (typeof options?.audioTrackId === 'number') {
    searchParams.set('audio_track_id', String(options.audioTrackId))
  }

  const query = searchParams.toString()
  return withApiPrefix(`/media-files/${mediaFileId}/stream${query ? `?${query}` : ''}`)
}

export const subtitleFileStreamUrl = (subtitleFileId: number) =>
  withApiPrefix(`/subtitle-files/${subtitleFileId}/stream`)

export { ApiError }
