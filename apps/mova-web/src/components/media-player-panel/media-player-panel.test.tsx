import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { PlaybackProgress } from '../../api/types'
import { buildPlaybackSourceErrorMessage, MediaPlayerPanel } from './index'

const clientMocks = vi.hoisted(() => ({
  flushMediaItemPlaybackProgress: vi.fn(),
  getMediaItemPlaybackProgress: vi.fn(),
  listMediaFileAudioTracks: vi.fn(),
  listMediaFileSubtitles: vi.fn(),
  listMediaItemFiles: vi.fn(),
  mediaFileStreamUrl: vi.fn(
    (mediaFileId: number, options?: { audioTrackId?: number | null }) =>
      `/api/media-files/${mediaFileId}/stream${
        typeof options?.audioTrackId === 'number' ? `?audio_track_id=${options.audioTrackId}` : ''
      }`,
  ),
  subtitleFileStreamUrl: vi.fn(
    (subtitleFileId: number) => `/api/subtitle-files/${subtitleFileId}/stream`,
  ),
  updateMediaItemPlaybackProgress: vi.fn(),
}))

vi.mock('../../api/client', () => ({
  flushMediaItemPlaybackProgress: clientMocks.flushMediaItemPlaybackProgress,
  getMediaItemPlaybackProgress: clientMocks.getMediaItemPlaybackProgress,
  listMediaFileAudioTracks: clientMocks.listMediaFileAudioTracks,
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

const createDeferred = <T,>() => {
  let resolve!: (value: T | PromiseLike<T>) => void
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve
  })

  return { promise, resolve }
}

const installVideoTestState = (video: HTMLVideoElement) => {
  let currentTime = 0
  let duration = 7200
  let paused = true
  let muted = false
  let volume = 1
  let playbackRate = 1
  let readyState: number = HTMLMediaElement.HAVE_NOTHING

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
  Object.defineProperty(video, 'playbackRate', {
    configurable: true,
    get: () => playbackRate,
    set: (value: number) => {
      playbackRate = Number(value)
    },
  })
  Object.defineProperty(video, 'readyState', {
    configurable: true,
    get: () => readyState,
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
    setReadyState: (value: number) => {
      readyState = value
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
        technical_tags: [],
        scan_hash: null,
        created_at: '2026-04-07T00:00:00Z',
        updated_at: '2026-04-07T00:00:00Z',
      },
    ])
    clientMocks.getMediaItemPlaybackProgress.mockResolvedValue({
      id: 71,
      media_item_id: 31,
      last_media_file_id: 401,
      position_seconds: 320,
      duration_seconds: 7200,
      last_watched_at: '2026-04-07T00:00:00Z',
      is_finished: false,
    })
    clientMocks.listMediaFileAudioTracks.mockResolvedValue([
      {
        id: 801,
        media_file_id: 401,
        stream_index: 1,
        language: 'zh-CN',
        audio_codec: 'aac',
        label: 'Mandarin Stereo',
        is_default: true,
        created_at: '2026-04-07T00:00:00Z',
        updated_at: '2026-04-07T00:00:00Z',
      },
      {
        id: 802,
        media_file_id: 401,
        stream_index: 2,
        language: 'en',
        audio_codec: 'ac3',
        label: 'English 5.1',
        is_default: false,
        created_at: '2026-04-07T00:00:00Z',
        updated_at: '2026-04-07T00:00:00Z',
      },
    ])
    clientMocks.listMediaFileSubtitles.mockResolvedValue([])
    clientMocks.updateMediaItemPlaybackProgress.mockImplementation(
      async (_mediaItemId: number, input: Record<string, unknown>) => ({
        id: 71,
        media_item_id: 31,
        last_media_file_id: input.media_file_id as number,
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

  it('opens the first media file when playback progress does not exist yet', async () => {
    clientMocks.getMediaItemPlaybackProgress.mockResolvedValue(null)

    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} title="Interstellar" />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')?.getAttribute('src')).toContain(
        '/api/media-files/401/stream',
      )
    })
    expect(screen.queryByText('Loading player…')).not.toBeInTheDocument()
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
    await waitFor(() => {
      expect(clientMocks.updateMediaItemPlaybackProgress).toHaveBeenCalledWith(31, {
        media_file_id: 401,
        position_seconds: 0,
        duration_seconds: 7200,
        is_finished: false,
      })
    })
    expect(clientMocks.updateMediaItemPlaybackProgress).not.toHaveBeenCalledWith(
      31,
      expect.objectContaining({ position_seconds: 320 }),
    )
  })

  it('waits for saved progress before opening an explicitly requested file version', async () => {
    const playbackProgress = createDeferred<PlaybackProgress>()
    clientMocks.getMediaItemPlaybackProgress.mockReturnValue(playbackProgress.promise)
    clientMocks.listMediaItemFiles.mockResolvedValue([
      {
        id: 401,
        media_item_id: 31,
        file_path: '/media/movies/interstellar-1080p.mkv',
        container: 'mkv',
        file_size: 1,
        duration_seconds: 7200,
        video_codec: 'h264',
        audio_codec: 'aac',
        width: 1920,
        height: 1080,
        bitrate: 1_000_000,
        technical_tags: [],
        scan_hash: null,
        created_at: '2026-04-07T00:00:00Z',
        updated_at: '2026-04-07T00:00:00Z',
      },
      {
        id: 402,
        media_item_id: 31,
        file_path: '/media/movies/interstellar-2160p.mkv',
        container: 'mkv',
        file_size: 2,
        duration_seconds: 7200,
        video_codec: 'hevc',
        audio_codec: 'aac',
        width: 3840,
        height: 2160,
        bitrate: 2_000_000,
        technical_tags: [],
        scan_hash: null,
        created_at: '2026-04-07T00:00:00Z',
        updated_at: '2026-04-07T00:00:00Z',
      },
    ])

    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} preferredMediaFileId={402} title="Interstellar" />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(clientMocks.listMediaItemFiles).toHaveBeenCalledWith(31)
    })

    expect(container.querySelector('video')).toBeNull()
    expect(screen.getByText('Loading player…')).toBeInTheDocument()
    expect(clientMocks.updateMediaItemPlaybackProgress).not.toHaveBeenCalled()

    playbackProgress.resolve({
      id: 71,
      media_item_id: 31,
      last_media_file_id: 402,
      position_seconds: 320,
      duration_seconds: 7200,
      last_watched_at: '2026-04-07T00:00:00Z',
      is_finished: false,
    })

    await waitFor(() => {
      expect(container.querySelector('video')?.getAttribute('src')).toContain(
        '/api/media-files/402/stream',
      )
    })
    await waitFor(() => {
      expect(clientMocks.updateMediaItemPlaybackProgress).toHaveBeenCalledWith(31, {
        media_file_id: 402,
        position_seconds: 320,
        duration_seconds: 7200,
        is_finished: false,
      })
    })
    expect(clientMocks.updateMediaItemPlaybackProgress).not.toHaveBeenCalledWith(
      31,
      expect.objectContaining({
        media_file_id: 401,
        position_seconds: 0,
      }),
    )
  })

  it('starts playback automatically after the player metadata loads', async () => {
    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} title="Interstellar" />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const video = container.querySelector('video') as HTMLVideoElement
    installVideoTestState(video)

    fireEvent.loadedMetadata(video)

    await waitFor(() => {
      expect(video.play).toHaveBeenCalledTimes(1)
    })
  })

  it('lets space cancel an automatic play request that has not settled yet', async () => {
    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} title="Interstellar" variant="immersive" />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const video = container.querySelector('video') as HTMLVideoElement
    const videoState = installVideoTestState(video)
    const playRequest = createDeferred<void>()
    videoState.setReadyState(HTMLMediaElement.HAVE_METADATA)
    video.play = vi.fn().mockReturnValue(playRequest.promise)

    fireEvent.loadedMetadata(video)
    expect(video.play).toHaveBeenCalledTimes(1)

    fireEvent.keyDown(window, { code: 'Space', key: ' ' })
    expect(video.pause).toHaveBeenCalledTimes(1)

    playRequest.resolve()

    await waitFor(() => {
      expect(video.pause).toHaveBeenCalledTimes(2)
    })
    expect(
      screen.queryByText(
        'Playback was interrupted before it could start. Click play again to continue.',
      ),
    ).not.toBeInTheDocument()
  })

  it('keeps the first space-key pause intent while a continue-watching source loads', async () => {
    const playbackProgress = createDeferred<PlaybackProgress>()
    clientMocks.getMediaItemPlaybackProgress.mockReturnValue(playbackProgress.promise)

    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel
          mediaItemId={31}
          preferredMediaFileId={401}
          title="Interstellar"
          variant="immersive"
        />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(clientMocks.listMediaItemFiles).toHaveBeenCalledWith(31)
    })
    expect(container.querySelector('video')).toBeNull()

    fireEvent.keyDown(window, { code: 'Space', key: ' ' })

    playbackProgress.resolve({
      id: 71,
      media_item_id: 31,
      last_media_file_id: 401,
      position_seconds: 320,
      duration_seconds: 7200,
      last_watched_at: '2026-04-07T00:00:00Z',
      is_finished: false,
    })

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const video = container.querySelector('video') as HTMLVideoElement
    installVideoTestState(video)
    fireEvent.loadedMetadata(video)

    expect(video.play).not.toHaveBeenCalled()

    fireEvent.keyDown(window, { code: 'Space', key: ' ' })

    await waitFor(() => {
      expect(video.play).toHaveBeenCalledTimes(1)
    })
  })

  it('toggles playback when pressing the space key', async () => {
    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} title="Interstellar" variant="immersive" />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const video = container.querySelector('video') as HTMLVideoElement
    const videoState = installVideoTestState(video)

    fireEvent.loadedMetadata(video)
    await waitFor(() => {
      expect(video.play).toHaveBeenCalledTimes(1)
    })

    vi.mocked(video.play).mockClear()

    fireEvent.keyDown(window, { code: 'Space', key: ' ' })

    expect(video.pause).toHaveBeenCalledTimes(1)

    videoState.setPaused(true)
    fireEvent.keyDown(window, { code: 'Space', key: ' ' })

    await waitFor(() => {
      expect(video.play).toHaveBeenCalledTimes(1)
    })
  })

  it('shows the current playback action with the player chrome on mouse movement', async () => {
    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} title="Interstellar" variant="immersive" />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const video = container.querySelector('video') as HTMLVideoElement
    installVideoTestState(video)
    fireEvent.canPlay(video)
    fireEvent.play(video)
    fireEvent.mouseMove(window)

    await waitFor(() => {
      expect(container.querySelector('.player-panel__center-playback-control')).toHaveAttribute(
        'aria-label',
        'Pause playback',
      )
    })
    expect(container.querySelector('.player-stage__controls')).toHaveClass(
      'player-stage__controls--visible',
    )

    fireEvent.click(container.querySelector('.player-panel__center-playback-control') as Element)
    fireEvent.pause(video)

    await waitFor(() => {
      expect(container.querySelector('.player-panel__center-playback-control')).toBeNull()
    })
    expect(container.querySelector('.player-stage__controls')).not.toHaveClass(
      'player-stage__controls--visible',
    )

    fireEvent.mouseMove(window)

    await waitFor(() => {
      expect(container.querySelector('.player-panel__center-playback-control')).toHaveAttribute(
        'aria-label',
        'Start playback',
      )
    })
  })

  it('releases pointer-focused buttons and sliders after player actions', async () => {
    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} title="Interstellar" variant="immersive" />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const video = container.querySelector('video') as HTMLVideoElement
    const videoState = installVideoTestState(video)
    videoState.setReadyState(HTMLMediaElement.HAVE_METADATA)
    const fullscreenButton = screen.getByRole('button', { name: 'Enter fullscreen' })

    fullscreenButton.focus()
    expect(document.activeElement).toBe(fullscreenButton)

    fireEvent.pointerUp(fullscreenButton, { pointerType: 'mouse' })
    expect(document.activeElement).not.toBe(fullscreenButton)

    const timeline = screen.getByRole('slider', { name: 'Seek playback position' })
    timeline.focus()
    expect(document.activeElement).toBe(timeline)

    fireEvent.change(timeline, { target: { value: '480' } })
    fireEvent.pointerUp(timeline, { pointerType: 'mouse' })
    expect(document.activeElement).not.toBe(timeline)

    fireEvent.keyDown(window, { code: 'Space', key: ' ' })
    expect(video.play).toHaveBeenCalledTimes(1)
  })

  it('changes playback speed from the immersive player toolbar', async () => {
    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} title="Interstellar" variant="immersive" />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const video = container.querySelector('video') as HTMLVideoElement
    installVideoTestState(video)
    fireEvent.loadedMetadata(video)

    fireEvent.click(screen.getByRole('button', { name: 'Playback speed: 1×' }))
    fireEvent.click(screen.getByRole('menuitemradio', { name: '1.5×' }))

    expect(video.playbackRate).toBe(1.5)
    expect(screen.getByRole('button', { name: 'Playback speed: 1.5×' })).toBeInTheDocument()
  })

  it('requests fullscreen on the complete player screen', async () => {
    const { container } = render(
      <div className="player-screen" data-testid="player-screen">
        <QueryClientProvider client={createTestQueryClient()}>
          <MediaPlayerPanel mediaItemId={31} title="Interstellar" variant="immersive" />
        </QueryClientProvider>
      </div>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const playerScreen = screen.getByTestId('player-screen')
    const requestFullscreen = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(playerScreen, 'requestFullscreen', {
      configurable: true,
      value: requestFullscreen,
    })

    fireEvent.click(screen.getByRole('button', { name: 'Enter fullscreen' }))

    expect(requestFullscreen).toHaveBeenCalledTimes(1)
  })

  it('shows skip intro only inside the configured intro window', async () => {
    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel
          intro={{ startSeconds: 15, endSeconds: 75 }}
          mediaItemId={31}
          title="Interstellar"
          variant="immersive"
        />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const video = container.querySelector('video') as HTMLVideoElement
    const videoState = installVideoTestState(video)
    fireEvent.loadedMetadata(video)

    expect(screen.queryByRole('button', { name: 'Skip Intro' })).toBeNull()

    videoState.setCurrentTime(20)
    fireEvent.timeUpdate(video)

    expect(screen.getByRole('button', { name: 'Skip Intro' })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Skip Intro' }))

    expect(videoState.getCurrentTime()).toBe(75)
    expect(screen.queryByRole('button', { name: 'Skip Intro' })).toBeNull()
  })

  it('shows next episode actions only when a next episode exists', async () => {
    const onSelectEpisode = vi.fn()
    const { container, rerender } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel
          mediaItemId={31}
          nextEpisode={{
            label: 'S01 · E02 · Lazarus',
            mediaItemId: 32,
            seasonNumber: 1,
            episodeNumber: 2,
          }}
          onSelectEpisode={onSelectEpisode}
          title="Interstellar"
          variant="immersive"
        />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const video = container.querySelector('video') as HTMLVideoElement
    const videoState = installVideoTestState(video)
    fireEvent.loadedMetadata(video)

    expect(screen.getByRole('button', { name: /Play next episode/i })).toBeInTheDocument()

    videoState.setCurrentTime(7172)
    fireEvent.timeUpdate(video)

    expect(screen.getAllByRole('button', { name: /Play next episode/i }).length).toBeGreaterThan(0)

    fireEvent.click(screen.getByRole('button', { name: /Play next episode/i }))
    expect(onSelectEpisode).toHaveBeenCalledWith(32)

    rerender(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} title="Interstellar" variant="immersive" />
      </QueryClientProvider>,
    )

    expect(screen.queryByRole('button', { name: /Play next episode/i })).toBeNull()
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
        technical_tags: [],
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
        technical_tags: [],
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

  it('keeps the playback position when switching to another audio track', async () => {
    const { container } = render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MediaPlayerPanel mediaItemId={31} title="Interstellar" variant="immersive" />
      </QueryClientProvider>,
    )

    await waitFor(() => {
      expect(container.querySelector('video')).not.toBeNull()
    })

    const video = container.querySelector('video') as HTMLVideoElement
    const videoState = installVideoTestState(video)
    fireEvent.loadedMetadata(video)
    videoState.setCurrentTime(845)
    videoState.setPaused(false)

    fireEvent.click(await screen.findByRole('button', { name: 'Select audio track' }))
    fireEvent.click(screen.getByRole('menuitem', { name: /English 5\.1/i }))

    await waitFor(() => {
      expect(video.getAttribute('src')).toContain('/api/media-files/401/stream?audio_track_id=802')
    })

    fireEvent.loadedMetadata(video)

    expect(videoState.getCurrentTime()).toBe(845)
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
