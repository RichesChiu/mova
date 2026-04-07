import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { buildPlaybackSourceErrorMessage, MediaPlayerPanel } from './index'

const clientMocks = vi.hoisted(() => ({
  flushMediaItemPlaybackProgress: vi.fn(),
  getMediaItemPlaybackProgress: vi.fn(),
  listMediaFileSubtitles: vi.fn(),
  listMediaItemFiles: vi.fn(),
  mediaFileStreamUrl: vi.fn((mediaFileId: number) => `/api/media-files/${mediaFileId}/stream`),
  subtitleFileStreamUrl: vi.fn(
    (subtitleFileId: number) => `/api/subtitle-files/${subtitleFileId}/stream`,
  ),
  updateMediaItemPlaybackProgress: vi.fn(),
}))

vi.mock('../../api/client', () => ({
  flushMediaItemPlaybackProgress: clientMocks.flushMediaItemPlaybackProgress,
  getMediaItemPlaybackProgress: clientMocks.getMediaItemPlaybackProgress,
  listMediaFileSubtitles: clientMocks.listMediaFileSubtitles,
  listMediaItemFiles: clientMocks.listMediaItemFiles,
  mediaFileStreamUrl: clientMocks.mediaFileStreamUrl,
  subtitleFileStreamUrl: clientMocks.subtitleFileStreamUrl,
  updateMediaItemPlaybackProgress: clientMocks.updateMediaItemPlaybackProgress,
}))

const createTestQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
      mutations: {
        retry: false,
      },
    },
  })

const installVideoTestState = (video: HTMLVideoElement) => {
  let currentTime = 0
  let duration = 7200
  let paused = true
  let muted = false
  let volume = 1

  Object.defineProperty(video, 'currentTime', {
    configurable: true,
    get: () => currentTime,
    set: (value: number) => {
      currentTime = Number(value)
    },
  })
  Object.defineProperty(video, 'duration', {
    configurable: true,
    get: () => duration,
  })
  Object.defineProperty(video, 'paused', {
    configurable: true,
    get: () => paused,
  })
  Object.defineProperty(video, 'muted', {
    configurable: true,
    get: () => muted,
    set: (value: boolean) => {
      muted = Boolean(value)
    },
  })
  Object.defineProperty(video, 'volume', {
    configurable: true,
    get: () => volume,
    set: (value: number) => {
      volume = Number(value)
    },
  })
  Object.defineProperty(video, 'buffered', {
    configurable: true,
    get: () => ({
      length: 0,
      start: () => 0,
      end: () => 0,
    }),
  })
  Object.defineProperty(video, 'textTracks', {
    configurable: true,
    get: () => [],
  })
  Object.defineProperty(video, 'error', {
    configurable: true,
    writable: true,
    value: null,
  })

  video.load = vi.fn()
  video.play = vi.fn().mockImplementation(async () => {
    paused = false
  })
  video.pause = vi.fn().mockImplementation(() => {
    paused = true
  })

  return {
    getCurrentTime: () => currentTime,
    setCurrentTime: (value: number) => {
      currentTime = value
    },
    setPaused: (value: boolean) => {
      paused = value
    },
    setDuration: (value: number) => {
      duration = value
    },
  }
}

describe('MediaPlayerPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    clientMocks.listMediaItemFiles.mockResolvedValue([
      {
        id: 401,
        media_item_id: 31,
        file_path: '/media/movies/interstellar.mkv',
        container: 'mkv',
        file_size: 1,
        duration_seconds: 7200,
        video_codec: 'h264',
        audio_codec: 'aac',
        width: 1920,
        height: 1080,
        bitrate: 1_000_000,
        scan_hash: null,
        created_at: '2026-04-07T00:00:00Z',
        updated_at: '2026-04-07T00:00:00Z',
      },
    ])
    clientMocks.getMediaItemPlaybackProgress.mockResolvedValue({
      id: 71,
      media_item_id: 31,
      media_file_id: 401,
      position_seconds: 320,
      duration_seconds: 7200,
      last_watched_at: '2026-04-07T00:00:00Z',
      is_finished: false,
    })
    clientMocks.listMediaFileSubtitles.mockResolvedValue([])
    clientMocks.updateMediaItemPlaybackProgress.mockImplementation(
      async (_mediaItemId: number, input: Record<string, unknown>) => ({
        id: 71,
        media_item_id: 31,
        media_file_id: input.media_file_id as number,
        position_seconds: input.position_seconds as number,
        duration_seconds: (input.duration_seconds as number | null | undefined) ?? null,
        last_watched_at: '2026-04-07T00:00:05Z',
        is_finished: Boolean(input.is_finished),
      }),
    )
  })

  it('restores saved playback progress after metadata loads', async () => {
    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} title="Interstellar" />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const video = container.querySelector('video') as HTMLVideoElement
    const videoState = installVideoTestState(video)

    fireEvent.loadedMetadata(video)

    expect(videoState.getCurrentTime()).toBe(320)
  })

  it('honors from-start over a stored resume point', async () => {
    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} startMode="from-start" title="Interstellar" />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const video = container.querySelector('video') as HTMLVideoElement
    const videoState = installVideoTestState(video)
    videoState.setCurrentTime(100)

    fireEvent.loadedMetadata(video)

    expect(videoState.getCurrentTime()).toBe(0)
  })

  it('migrates the playback position when switching to another source', async () => {
    clientMocks.listMediaItemFiles.mockResolvedValue([
      {
        id: 401,
        media_item_id: 31,
        file_path: '/media/movies/interstellar.mkv',
        container: 'mkv',
        file_size: 1,
        duration_seconds: 7200,
        video_codec: 'h264',
        audio_codec: 'aac',
        width: 1920,
        height: 1080,
        bitrate: 1_000_000,
        scan_hash: null,
        created_at: '2026-04-07T00:00:00Z',
        updated_at: '2026-04-07T00:00:00Z',
      },
      {
        id: 402,
        media_item_id: 31,
        file_path: '/media/movies/interstellar.mp4',
        container: 'mp4',
        file_size: 1,
        duration_seconds: 7200,
        video_codec: 'h264',
        audio_codec: 'aac',
        width: 1920,
        height: 1080,
        bitrate: 1_000_000,
        scan_hash: null,
        created_at: '2026-04-07T00:00:00Z',
        updated_at: '2026-04-07T00:00:00Z',
      },
    ])

    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} title="Interstellar" />
      </QueryClientProvider>,
    )

    await screen.findByRole('button', { name: /MP4/i })

    const video = container.querySelector('video') as HTMLVideoElement
    const videoState = installVideoTestState(video)
    fireEvent.loadedMetadata(video)
    videoState.setCurrentTime(512)
    videoState.setPaused(false)

    fireEvent.click(screen.getByRole('button', { name: /MP4/i }))

    await waitFor(() => {
      expect(video.getAttribute('src')).toContain('/api/media-files/402/stream')
    })

    fireEvent.loadedMetadata(video)

    await waitFor(() => {
      expect(clientMocks.updateMediaItemPlaybackProgress).toHaveBeenCalledWith(31, {
        media_file_id: 402,
        position_seconds: 512,
        duration_seconds: 7200,
        is_finished: false,
      })
    })
  })

  it('maps source playback errors to a clearer user-facing message', () => {
    const failingVideo = {
      error: { code: 2 },
    } as HTMLVideoElement

    expect(buildPlaybackSourceErrorMessage(failingVideo)).toBe(
      'The selected file could not be streamed. Check the storage mount or network path.',
    )
  })

  it('degrades gracefully when subtitle loading fails', async () => {
    clientMocks.listMediaFileSubtitles.mockRejectedValueOnce(new Error('subtitle query failed'))

    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} title="Interstellar" />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    expect(
      await screen.findByText(
        'The selected subtitle could not be loaded. Playback will continue without subtitles.',
      ),
    ).toBeInTheDocument()
    expect(container.querySelector('video')).not.toBeNull()
  })
})
