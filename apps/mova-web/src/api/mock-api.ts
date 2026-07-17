import { isMockApiEnabled } from './mock-control'
import type {
  AudioTrack,
  ContinueWatchingItem,
  EpisodeOutline,
  GlobalSearchResult,
  HomeResponse,
  Library,
  LibraryDetail,
  MediaCastMember,
  MediaFile,
  MediaItem,
  MediaItemDetail,
  MediaItemListResponse,
  MediaItemPlaybackHeader,
  PlaybackProgress,
  RecentlyAddedLibraryMediaItems,
  ServerMediaDirectoryNode,
  SubtitleFile,
  UserAccount,
} from './types'

const MOCK_NOW = '2026-06-05T14:00:00+08:00'

interface MockResult<T> {
  data: T
  handled: true
}

const browserWindow = () => (typeof window === 'undefined' ? null : window)

const mockResult = <T>(data: T): MockResult<T> => ({ data, handled: true })

const mockPosterPaths = [
  new URL('./mock-media/poster-07.png', import.meta.url).href,
  new URL('./mock-media/poster-08.png', import.meta.url).href,
  new URL('./mock-media/poster-09.png', import.meta.url).href,
  new URL('./mock-media/poster-10.png', import.meta.url).href,
  new URL('./mock-media/poster-11.png', import.meta.url).href,
  new URL('./mock-media/poster-01.png', import.meta.url).href,
  new URL('./mock-media/poster-02.png', import.meta.url).href,
  new URL('./mock-media/poster-03.png', import.meta.url).href,
  new URL('./mock-media/poster-04.png', import.meta.url).href,
  new URL('./mock-media/poster-05.png', import.meta.url).href,
  new URL('./mock-media/poster-06.png', import.meta.url).href,
] as const

const mockBackdropPaths = [
  new URL('./mock-media/backdrop-01.png', import.meta.url).href,
  new URL('./mock-media/backdrop-02.png', import.meta.url).href,
  new URL('./mock-media/backdrop-03.png', import.meta.url).href,
] as const

const mockPosterPath = (id: number) => mockPosterPaths[Math.abs(id) % mockPosterPaths.length]
const mockBackdropPath = (id: number) => mockBackdropPaths[Math.abs(id) % mockBackdropPaths.length]

const createLibrary = (id: number, name: string): Library => ({
  id,
  name,
  description: null,
  metadata_language: 'zh-CN',
  root_path: `/media/${name.toLowerCase().replaceAll(' ', '-')}`,
  created_at: MOCK_NOW,
  updated_at: MOCK_NOW,
})

const mockLibraries = [
  createLibrary(1, 'Overseas TV'),
  createLibrary(2, 'Animation'),
  createLibrary(3, 'Family'),
  createLibrary(4, 'Documentary'),
  createLibrary(5, 'Kids'),
  createLibrary(6, 'Classics'),
]

const mockServerMediaTree: ServerMediaDirectoryNode = {
  name: 'media',
  path: '/media',
  children: [
    {
      name: 'Movies',
      path: '/media/Movies',
      children: [
        { name: 'Chinese', path: '/media/Movies/Chinese', children: [] },
        { name: 'International', path: '/media/Movies/International', children: [] },
      ],
    },
    {
      name: 'Series',
      path: '/media/Series',
      children: [
        { name: 'Animation', path: '/media/Series/Animation', children: [] },
        { name: 'Documentary', path: '/media/Series/Documentary', children: [] },
      ],
    },
  ],
}

const libraryCounts = new Map<number, { media: number; movies: number; series: number }>([
  [1, { media: 1248, movies: 248, series: 732 }],
  [2, { media: 932, movies: 512, series: 156 }],
  [3, { media: 1156, movies: 388, series: 406 }],
  [4, { media: 614, movies: 220, series: 188 }],
  [5, { media: 482, movies: 266, series: 92 }],
  [6, { media: 729, movies: 640, series: 48 }],
])

const createMediaItem = ({
  id,
  libraryId,
  mediaType = 'series',
  title,
  year = 2025,
}: {
  id: number
  libraryId: number
  mediaType?: string
  title: string
  year?: number
}): MediaItem => ({
  id,
  library_id: libraryId,
  media_type: mediaType,
  title,
  source_title: title,
  original_title: null,
  sort_title: null,
  metadata_provider: 'tmdb',
  metadata_provider_item_id: id * 100,
  metadata_status: 'matched',
  metadata_failure_reason: null,
  remote_media_type: mediaType,
  year,
  imdb_rating: id % 3 === 0 ? '8.6' : null,
  country: mediaType === 'movie' ? 'US' : null,
  genres: mediaType === 'movie' ? 'Drama, Adventure' : 'Drama',
  studio: null,
  overview:
    'A polished mock media item used for local UI review when the real library is too small.',
  poster_path: mockPosterPath(id),
  backdrop_path: mockBackdropPath(libraryId + id),
  created_at: new Date(Date.UTC(2026, 5, 5, 8, 0, 0) - id * 60000).toISOString(),
  updated_at: MOCK_NOW,
})

const mockMediaItems = [
  createMediaItem({ id: 11, libraryId: 1, title: 'The Long Voyage' }),
  createMediaItem({ id: 12, libraryId: 1, title: 'City of Lights' }),
  createMediaItem({ id: 13, libraryId: 1, title: 'Shadow Realm' }),
  createMediaItem({ id: 14, libraryId: 1, title: 'Northern Winds' }),
  createMediaItem({ id: 15, libraryId: 1, title: 'Silent Code' }),
  createMediaItem({ id: 16, libraryId: 1, title: 'Nightfall' }),
  createMediaItem({ id: 21, libraryId: 2, title: 'Moon Gate' }),
  createMediaItem({ id: 22, libraryId: 2, title: 'Dragon Arc' }),
  createMediaItem({ id: 23, libraryId: 2, title: 'Blue Archive' }),
  createMediaItem({ id: 24, libraryId: 2, title: 'Signal Bloom' }),
  createMediaItem({ id: 25, libraryId: 2, title: 'Aster World' }),
  createMediaItem({ id: 31, libraryId: 3, mediaType: 'movie', title: 'River House', year: 2024 }),
  createMediaItem({ id: 32, libraryId: 3, mediaType: 'movie', title: 'Golden Summer', year: 2021 }),
  createMediaItem({
    id: 33,
    libraryId: 3,
    mediaType: 'movie',
    title: 'The Small Road',
    year: 2020,
  }),
  createMediaItem({ id: 34, libraryId: 3, mediaType: 'movie', title: 'Home Again', year: 2022 }),
  createMediaItem({ id: 41, libraryId: 4, mediaType: 'movie', title: 'Frozen Roads', year: 2019 }),
  createMediaItem({ id: 51, libraryId: 5, mediaType: 'series', title: 'Tiny Orbit', year: 2026 }),
  createMediaItem({
    id: 61,
    libraryId: 6,
    mediaType: 'movie',
    title: 'The Old Harbor',
    year: 1988,
  }),
]

const mockCurrentUser: UserAccount = {
  id: 1,
  username: 'admin',
  nickname: 'Alex Chen',
  role: 'admin',
  is_primary_admin: true,
  is_enabled: true,
  library_ids: [],
  created_at: MOCK_NOW,
  updated_at: MOCK_NOW,
}

const findLibrary = (libraryId: number) => mockLibraries.find((library) => library.id === libraryId)

const findMediaItem = (mediaItemId: number) =>
  mockMediaItems.find((item) => item.id === mediaItemId)

const libraryForMediaItem = (mediaItem: MediaItem) => {
  const library = findLibrary(mediaItem.library_id)
  if (!library) {
    throw new Error(`Mock media item ${mediaItem.id} references a missing library`)
  }
  return library
}

const libraryDetail = (libraryId: number): LibraryDetail | null => {
  const library = findLibrary(libraryId)
  const counts = libraryCounts.get(libraryId)
  if (!library || !counts) {
    return null
  }

  return {
    ...library,
    media_count: counts.media,
    movie_count: counts.movies,
    series_count: counts.series,
    last_scan: {
      id: libraryId * 100,
      library_id: libraryId,
      status: 'success',
      phase: null,
      total_files: counts.media,
      scanned_files: counts.media,
      local_analyzed_files: counts.media,
      local_committed_files: counts.media,
      remote_completed_files: counts.media,
      progress_percent: 100,
      created_at: MOCK_NOW,
      started_at: MOCK_NOW,
      finished_at: MOCK_NOW,
      error_message: null,
    },
  }
}

const sortedMediaItems = (items: MediaItem[]) =>
  [...items].sort((left, right) => {
    const createdDiff = Date.parse(right.created_at) - Date.parse(left.created_at)
    return createdDiff === 0 ? right.id - left.id : createdDiff
  })

const listLibraryMediaItems = (libraryId: number, url: URL): MediaItemListResponse => {
  const page = Math.max(1, Number(url.searchParams.get('page') ?? 1))
  const pageSize = Math.max(1, Number(url.searchParams.get('page_size') ?? 50))
  const query = url.searchParams.get('query')?.trim().toLowerCase() ?? ''
  const year = Number(url.searchParams.get('year') ?? Number.NaN)
  const filtered = sortedMediaItems(
    mockMediaItems.filter((item) => {
      if (item.library_id !== libraryId) {
        return false
      }
      if (
        query &&
        !`${item.title} ${item.source_title} ${item.original_title ?? ''}`
          .toLowerCase()
          .includes(query)
      ) {
        return false
      }
      return Number.isFinite(year) ? item.year === year : true
    }),
  )
  const offset = (page - 1) * pageSize

  return {
    items: filtered.slice(offset, offset + pageSize),
    total: filtered.length,
    page,
    page_size: pageSize,
  }
}

const recentlyAddedByLibrary = (url: URL): RecentlyAddedLibraryMediaItems[] => {
  const itemLimit = Math.min(50, Math.max(1, Number(url.searchParams.get('limit') ?? 8)))
  const daysValue = url.searchParams.get('days')
  const days = daysValue === null ? null : Math.min(365, Math.max(1, Number(daysValue)))
  const createdSince = days === null ? null : Date.parse(MOCK_NOW) - days * 24 * 60 * 60 * 1000

  return mockLibraries
    .map((library) => {
      const items = sortedMediaItems(
        mockMediaItems.filter(
          (item) =>
            item.library_id === library.id &&
            (createdSince === null || Date.parse(item.created_at) >= createdSince),
        ),
      )
      return {
        library,
        items: items.slice(0, itemLimit),
        total: items.length,
      }
    })
    .filter((group) => group.items.length > 0)
    .sort((left, right) => {
      const leftLatest = Date.parse(left.items[0]?.created_at ?? '0')
      const rightLatest = Date.parse(right.items[0]?.created_at ?? '0')
      return rightLatest - leftLatest
    })
}

const globalSearch = (url: URL): GlobalSearchResult[] => {
  const query = url.searchParams.get('q')?.trim().toLowerCase() ?? ''
  const limit = Math.max(1, Number(url.searchParams.get('limit') ?? 12))

  if (!query) {
    return []
  }

  const mediaResults: GlobalSearchResult[] = mockMediaItems
    .filter((item) =>
      `${item.title} ${item.source_title} ${item.original_title ?? ''} ${item.overview ?? ''}`
        .toLowerCase()
        .includes(query),
    )
    .map((item) => ({
      kind: 'media_item',
      library_id: item.library_id,
      library_name: libraryForMediaItem(item).name,
      media_item_id: item.id,
      series_media_item_id: null,
      media_type: item.media_type,
      title: item.title,
      subtitle: null,
      year: item.year,
      overview: item.overview,
      poster_path: item.poster_path,
      backdrop_path: item.backdrop_path,
      season_number: null,
      episode_number: null,
    }))

  const episodeResults: GlobalSearchResult[] = mockMediaItems
    .filter((item) => item.media_type === 'series')
    .flatMap((series) =>
      episodeOutline(series.id).seasons.flatMap((season) =>
        season.episodes
          .filter((episode) =>
            `${episode.title} ${episode.overview ?? ''} ${series.title}`
              .toLowerCase()
              .includes(query),
          )
          .map((episode) => ({
            kind: 'episode',
            library_id: series.library_id,
            library_name: libraryForMediaItem(series).name,
            media_item_id: episode.media_item_id ?? series.id,
            series_media_item_id: series.id,
            media_type: 'episode',
            title: episode.title,
            subtitle: series.title,
            year: series.year,
            overview: episode.overview,
            poster_path: episode.poster_path,
            backdrop_path: episode.backdrop_path,
            season_number: season.season_number,
            episode_number: episode.episode_number,
          })),
      ),
    )

  return [...mediaResults, ...episodeResults].slice(0, limit)
}

const playbackProgress = (mediaItemId: number, index = 0): PlaybackProgress => ({
  id: mediaItemId * 10,
  media_item_id: mediaItemId,
  media_file_id: mediaItemId * 100,
  position_seconds: 900 + index * 240,
  duration_seconds: 3600,
  last_watched_at: MOCK_NOW,
  is_finished: false,
})

const continueWatching = (url: URL): ContinueWatchingItem[] => {
  const limit = Math.max(1, Number(url.searchParams.get('limit') ?? 12))
  return sortedMediaItems(mockMediaItems)
    .slice(0, limit)
    .map((mediaItem, index) => ({
      media_item: mediaItem,
      playback_progress: playbackProgress(mediaItem.id, index),
      season_number: mediaItem.media_type === 'series' ? (index % 3) + 1 : null,
      episode_number: mediaItem.media_type === 'series' ? index + 1 : null,
      episode_title: mediaItem.media_type === 'series' ? mediaItem.title : null,
      episode_overview: mediaItem.media_type === 'series' ? mediaItem.overview : null,
      episode_poster_path: mediaItem.media_type === 'series' ? mediaItem.poster_path : null,
      episode_backdrop_path: mediaItem.media_type === 'series' ? mediaItem.backdrop_path : null,
    }))
}

const mediaFile = (mediaItem: MediaItem): MediaFile => ({
  id: mediaItem.id * 100,
  media_item_id: mediaItem.id,
  file_path: `${libraryForMediaItem(mediaItem).root_path}/${mediaItem.title}.mkv`,
  container: 'mkv',
  file_size: 12_400_000_000,
  duration_seconds: 3600,
  video_title: mediaItem.title,
  video_codec: 'hevc',
  video_profile: 'Main 10',
  video_level: '5.1',
  audio_codec: 'eac3',
  width: 3840,
  height: 2160,
  bitrate: 18_000_000,
  video_bitrate: 16_000_000,
  video_frame_rate: 23.976,
  video_aspect_ratio: '16:9',
  video_scan_type: 'progressive',
  video_color_primaries: 'bt2020',
  video_color_space: 'bt2020nc',
  video_color_transfer: 'smpte2084',
  video_bit_depth: 10,
  video_pixel_format: 'yuv420p10le',
  video_reference_frames: 1,
  technical_tags: ['4K', 'HDR10', 'EAC3'],
  scan_hash: 'mock-scan-hash',
  created_at: mediaItem.created_at,
  updated_at: mediaItem.updated_at,
})

const episodeOutline = (mediaItemId: number): EpisodeOutline => ({
  seasons: [1, 2, 3].map((seasonNumber) => ({
    season_id: mediaItemId * 10 + seasonNumber,
    season_number: seasonNumber,
    title: `Season ${seasonNumber}`,
    year: 2025,
    overview: null,
    poster_path: mockPosterPath(mediaItemId * 10 + seasonNumber),
    intro_start_seconds: 72,
    intro_end_seconds: 112,
    episodes: [1, 2, 3, 4, 5].map((episodeNumber) => ({
      episode_number: episodeNumber,
      title: `Episode ${episodeNumber}`,
      overview: 'Mock episode summary for UI review.',
      poster_path: mockPosterPath(mediaItemId * 100 + episodeNumber),
      backdrop_path: mockBackdropPath(mediaItemId * 100 + episodeNumber + 50),
      intro_start_seconds: 72,
      intro_end_seconds: 112,
      media_item_id: mediaItemId,
      is_available: true,
      playback_progress:
        seasonNumber === 1 && episodeNumber === 4 ? playbackProgress(mediaItemId, 2) : null,
    })),
  })),
})

const playbackHeader = (mediaItem: MediaItem): MediaItemPlaybackHeader => ({
  media_item_id: mediaItem.id,
  library_id: mediaItem.library_id,
  media_type: mediaItem.media_type,
  series_media_item_id: mediaItem.media_type === 'episode' ? mediaItem.id : null,
  title: mediaItem.title,
  original_title: mediaItem.original_title,
  year: mediaItem.year,
  season_number: mediaItem.media_type === 'series' ? 1 : null,
  episode_number: mediaItem.media_type === 'series' ? 4 : null,
  episode_title: mediaItem.media_type === 'series' ? 'Episode 4' : null,
})

const mockCast: MediaCastMember[] = [
  {
    person_id: 1,
    sort_order: 1,
    name: 'Mira Stone',
    character_name: 'Captain',
    profile_path: null,
  },
  {
    person_id: 2,
    sort_order: 2,
    name: 'Leon Vale',
    character_name: 'Navigator',
    profile_path: null,
  },
  {
    person_id: 3,
    sort_order: 3,
    name: 'Ada Cross',
    character_name: 'Archivist',
    profile_path: null,
  },
]

const audioTracks: AudioTrack[] = [
  {
    id: 1,
    media_file_id: 0,
    stream_index: 1,
    language: 'eng',
    audio_codec: 'eac3',
    label: 'English 5.1',
    channel_layout: '5.1',
    channels: 6,
    bitrate: 640000,
    sample_rate: 48000,
    is_default: true,
    created_at: MOCK_NOW,
    updated_at: MOCK_NOW,
  },
]

const subtitles: SubtitleFile[] = [
  {
    id: 1,
    media_file_id: 0,
    source_kind: 'embedded',
    file_path: null,
    stream_index: 2,
    language: 'eng',
    subtitle_format: 'webvtt',
    label: 'English',
    is_default: false,
    is_forced: false,
    is_hearing_impaired: false,
    created_at: MOCK_NOW,
    updated_at: MOCK_NOW,
  },
]

const playbackProgressFromUpdate = (mediaItemId: number, init?: RequestInit): PlaybackProgress => {
  const baseProgress = playbackProgress(mediaItemId)
  if (typeof init?.body !== 'string') {
    return baseProgress
  }

  const input = JSON.parse(init.body) as Partial<PlaybackProgress>

  return {
    ...baseProgress,
    media_file_id:
      typeof input.media_file_id === 'number' ? input.media_file_id : baseProgress.media_file_id,
    position_seconds:
      typeof input.position_seconds === 'number'
        ? input.position_seconds
        : baseProgress.position_seconds,
    duration_seconds:
      typeof input.duration_seconds === 'number'
        ? input.duration_seconds
        : baseProgress.duration_seconds,
    is_finished:
      typeof input.is_finished === 'boolean' ? input.is_finished : baseProgress.is_finished,
  }
}

export const requestMockJson = async <T>(
  path: string,
  init?: RequestInit,
): Promise<MockResult<T> | null> => {
  if (!isMockApiEnabled()) {
    return null
  }

  const method = (init?.method ?? 'GET').toUpperCase()
  const origin = browserWindow()?.location.origin ?? 'http://mova.local'
  const url = new URL(path, origin)
  const pathname = url.pathname.replace(/^\/api/, '')

  if (method === 'POST' && pathname === '/auth/logout') {
    return mockResult(undefined as T)
  }

  if (method === 'PUT') {
    if (pathname === '/notifications' || /^\/notifications\/\d+\/read$/.test(pathname)) {
      return mockResult((pathname === '/notifications' ? 0 : undefined) as T)
    }
    const progressMatch = pathname.match(/^\/media-items\/(\d+)\/playback-progress$/)
    if (progressMatch) {
      return mockResult(playbackProgressFromUpdate(Number(progressMatch[1]), init) as T)
    }
  }

  if (method !== 'GET') {
    return null
  }

  if (pathname === '/auth/me') {
    return mockResult(mockCurrentUser as T)
  }
  if (pathname === '/home') {
    const homeURL = new URL(url)
    homeURL.searchParams.set('limit', '8')
    const home: HomeResponse = {
      current_user: mockCurrentUser,
      libraries: mockLibraries.flatMap((library) => {
        const detail = libraryDetail(library.id)
        return detail
          ? [
              {
                library: detail,
                preview_items: mockMediaItems
                  .filter((item) => item.library_id === library.id)
                  .slice(0, 16),
              },
            ]
          : []
      }),
      recently_added: recentlyAddedByLibrary(homeURL),
      continue_watching: continueWatching(homeURL),
      realtime: {
        protocol_version: 1,
        server_epoch: 'mock-server',
        resources: {},
      },
    }
    return mockResult(home as T)
  }
  if (pathname === '/libraries') {
    return mockResult(mockLibraries as T)
  }
  if (pathname === '/notifications') {
    return mockResult({ items: [], total_unread: 0, unread_by_category: {} } as T)
  }
  if (pathname === '/server/media-tree') {
    return mockResult(mockServerMediaTree as T)
  }
  if (pathname === '/libraries/recently-added') {
    return mockResult(recentlyAddedByLibrary(url) as T)
  }
  if (pathname === '/search') {
    return mockResult(globalSearch(url) as T)
  }
  if (pathname === '/playback-progress/continue-watching') {
    return mockResult(continueWatching(url) as T)
  }
  const libraryMediaMatch = pathname.match(/^\/libraries\/(\d+)\/media-items$/)
  if (libraryMediaMatch) {
    return mockResult(listLibraryMediaItems(Number(libraryMediaMatch[1]), url) as T)
  }

  const libraryMatch = pathname.match(/^\/libraries\/(\d+)$/)
  if (libraryMatch) {
    const detail = libraryDetail(Number(libraryMatch[1]))
    return detail ? mockResult(detail as T) : null
  }

  const mediaItemId = Number(pathname.match(/^\/media-items\/(\d+)/)?.[1] ?? Number.NaN)
  const mediaItem = Number.isFinite(mediaItemId) ? findMediaItem(mediaItemId) : null
  if (mediaItem && pathname === `/media-items/${mediaItemId}`) {
    return mockResult(mediaItem as MediaItemDetail as T)
  }
  if (mediaItem && pathname === `/media-items/${mediaItemId}/cast`) {
    return mockResult(mockCast as T)
  }
  if (mediaItem && pathname === `/media-items/${mediaItemId}/files`) {
    return mockResult([mediaFile(mediaItem)] as T)
  }
  if (mediaItem && pathname === `/media-items/${mediaItemId}/playback-progress`) {
    return mockResult(playbackProgress(mediaItem.id) as T)
  }
  if (mediaItem && pathname === `/media-items/${mediaItemId}/playback-header`) {
    return mockResult(playbackHeader(mediaItem) as T)
  }
  if (mediaItem && pathname === `/media-items/${mediaItemId}/episode-outline`) {
    return mockResult(episodeOutline(mediaItem.id) as T)
  }
  if (mediaItem && pathname === `/media-items/${mediaItemId}/metadata-search`) {
    return mockResult([] as T)
  }

  if (/^\/media-files\/\d+\/audio-tracks$/.test(pathname)) {
    return mockResult(audioTracks as T)
  }
  if (/^\/media-files\/\d+\/subtitles$/.test(pathname)) {
    return mockResult(subtitles as T)
  }

  return null
}
