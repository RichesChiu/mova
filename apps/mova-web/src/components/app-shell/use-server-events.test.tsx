import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useServerEvents } from './use-server-events'

const clientMocks = vi.hoisted(() => ({
  getRealtimeState: vi.fn(),
}))

vi.mock('../../api/client', () => ({
  getRealtimeState: clientMocks.getRealtimeState,
}))

type EventListenerMap = Map<string, Set<EventListener>>

class FakeEventSource {
  static instances: FakeEventSource[] = []
  listeners: EventListenerMap = new Map()

  constructor(_url: string) {
    FakeEventSource.instances.push(this)
  }

  addEventListener(type: string, listener: EventListener) {
    const listeners = this.listeners.get(type) ?? new Set<EventListener>()
    listeners.add(listener)
    this.listeners.set(type, listeners)
  }

  removeEventListener(type: string, listener: EventListener) {
    this.listeners.get(type)?.delete(listener)
  }

  close() {}

  emit(type: string, data?: unknown) {
    const event =
      data === undefined
        ? new Event(type)
        : ({ data: JSON.stringify(data) } as MessageEvent<string>)
    this.listeners.get(type)?.forEach((listener) => {
      listener(event as unknown as Event)
    })
  }
}

const scanJob = {
  id: 88,
  library_id: 7,
  status: 'running',
  phase: 'enriching',
  total_files: 40,
  scanned_files: 12,
  created_at: '2026-07-14T00:00:00Z',
  started_at: '2026-07-14T00:00:01Z',
  finished_at: null,
  error_message: null,
}

const completedScanItem = {
  scan_job_id: 88,
  library_id: 7,
  item_key: 'series:arcane',
  media_type: 'series',
  title: 'Arcane',
  year: 2024,
  overview: 'Two sisters fight on opposite sides of a conflict.',
  poster_path: null,
  backdrop_path: null,
  metadata_status: 'matched',
  remote_media_type: 'series',
  season_number: null,
  episode_number: null,
  item_index: 1,
  total_items: 1,
  stage: 'completed',
  progress_percent: 100,
}

const baselineState = {
  protocol_version: 2,
  server_epoch: 'server-a',
  resources: {
    'admin:libraries': 2,
    'library:7:catalog': 10,
    'user:3:continue-watching': 4,
  },
  active_scans: [scanJob],
}

const createTestQueryClient = () =>
  new QueryClient({ defaultOptions: { queries: { retry: false } } })

const HookHarness = () => {
  const runtime = useServerEvents({ enabled: true })
  return <div data-testid="scan-runtime">{JSON.stringify(runtime)}</div>
}

const renderHook = (queryClient: QueryClient, path = '/') =>
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route element={<HookHarness />} path="*" />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  )

describe('useServerEvents revision protocol', () => {
  beforeEach(() => {
    FakeEventSource.instances = []
    vi.stubGlobal('EventSource', FakeEventSource as unknown as typeof EventSource)
    clientMocks.getRealtimeState.mockReset().mockResolvedValue(baselineState)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('restores active scans and refreshes loaded read models before accepting the first baseline', async () => {
    const queryClient = createTestQueryClient()
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue(undefined)
    renderHook(queryClient)

    FakeEventSource.instances[0].emit('open')

    await waitFor(() => {
      expect(screen.getByTestId('scan-runtime')).toHaveTextContent('"library_id":7')
    })
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['home'] }, { throwOnError: true })
  })

  it('refreshes only the changed catalog resource and ignores duplicate revisions', async () => {
    const queryClient = createTestQueryClient()
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue(undefined)
    renderHook(queryClient, '/media-items/31')
    const source = FakeEventSource.instances[0]
    source.emit('open')
    await waitFor(() => expect(clientMocks.getRealtimeState).toHaveBeenCalled())

    const payload = {
      protocol_version: 2,
      changes: [{ resource: 'library:7:catalog', revision: 11 }],
    }
    source.emit('resources.changed', payload)

    await waitFor(() =>
      expect(invalidateSpy).toHaveBeenCalledWith(
        { queryKey: ['media-item-files'] },
        { throwOnError: true },
      ),
    )
    const keys = invalidateSpy.mock.calls.map(([filters]) => filters?.queryKey)
    expect(keys).toEqual(
      expect.arrayContaining([
        ['library', 7],
        ['library-media', 7],
        ['home'],
        ['libraries-page-recently-added'],
        ['media-item'],
        ['media-item-cast'],
        ['media-item-files'],
        ['media-item-playback-header'],
        ['media-episode-outline'],
      ]),
    )
    const firstRefreshCallCount = invalidateSpy.mock.calls.length

    source.emit('resources.changed', payload)
    await new Promise((resolve) => window.setTimeout(resolve, 10))
    expect(invalidateSpy).toHaveBeenCalledTimes(firstRefreshCallCount)
  })

  it('refreshes loaded read models before accepting the initial state revisions', async () => {
    const queryClient = createTestQueryClient()
    clientMocks.getRealtimeState.mockResolvedValue({
      ...baselineState,
      resources: { 'library:7:catalog': 10 },
      active_scans: [],
    })
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue(undefined)
    renderHook(queryClient)

    FakeEventSource.instances[0].emit('open')

    await waitFor(() => {
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['home'] }, { throwOnError: true })
    })
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['library', 7] }, { throwOnError: true })
    expect(invalidateSpy).toHaveBeenCalledWith(
      { queryKey: ['library-media', 7] },
      { throwOnError: true },
    )
  })

  it('deduplicates shared query keys across one resource event batch', async () => {
    const queryClient = createTestQueryClient()
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue(undefined)
    renderHook(queryClient)
    const source = FakeEventSource.instances[0]
    source.emit('open')
    await waitFor(() =>
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['home'] }, { throwOnError: true }),
    )
    invalidateSpy.mockClear()

    source.emit('resources.changed', {
      protocol_version: 2,
      changes: [
        { resource: 'library:7:catalog', revision: 11 },
        { resource: 'user:3:continue-watching', revision: 5 },
      ],
    })

    await waitFor(() =>
      expect(invalidateSpy).toHaveBeenCalledWith(
        { queryKey: ['continue-watching'] },
        { throwOnError: true },
      ),
    )
    expect(
      invalidateSpy.mock.calls.filter(([filters]) => filters?.queryKey?.[0] === 'home'),
    ).toHaveLength(1)
  })

  it('retries a failed resource refresh before applying its revision', async () => {
    const queryClient = createTestQueryClient()
    let homeRefreshAttempts = 0
    let failHomeRefresh = false
    const invalidateSpy = vi
      .spyOn(queryClient, 'invalidateQueries')
      .mockImplementation((filters) => {
        if (filters?.queryKey?.[0] === 'home') {
          if (failHomeRefresh) {
            homeRefreshAttempts += 1
          }
          if (failHomeRefresh && homeRefreshAttempts === 1) {
            return Promise.reject(new Error('temporary refresh failure'))
          }
        }
        return Promise.resolve()
      })
    renderHook(queryClient)
    const source = FakeEventSource.instances[0]
    source.emit('open')
    await waitFor(() =>
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['home'] }, { throwOnError: true }),
    )
    failHomeRefresh = true
    homeRefreshAttempts = 0

    const change = {
      protocol_version: 2,
      changes: [{ resource: 'user:3:continue-watching', revision: 5 }],
    }
    source.emit('resources.changed', change)

    await waitFor(() => expect(homeRefreshAttempts).toBe(2), { timeout: 2_000 })
    source.emit('resources.changed', change)
    await new Promise((resolve) => window.setTimeout(resolve, 20))
    expect(homeRefreshAttempts).toBe(2)
  })

  it('stores a server-batched scan progress payload in one runtime update', async () => {
    const queryClient = createTestQueryClient()
    renderHook(queryClient)
    const source = FakeEventSource.instances[0]

    source.emit('scan.progress', {
      protocol_version: 2,
      scan_job: scanJob,
      items: [
        {
          scan_job_id: 88,
          library_id: 7,
          item_key: 'series:arcane',
          media_type: 'series',
          title: 'Arcane',
          year: 2024,
          overview: null,
          poster_path: null,
          backdrop_path: null,
          metadata_status: 'pending',
          remote_media_type: null,
          season_number: null,
          episode_number: null,
          item_index: 1,
          total_items: 3,
          stage: 'artwork',
          progress_percent: 76,
        },
      ],
    })

    await waitFor(() => {
      expect(screen.getByTestId('scan-runtime')).toHaveTextContent('Arcane')
      expect(screen.getByTestId('scan-runtime')).toHaveTextContent('76')
    })
  })

  it('refreshes final catalog data before removing completed scan cards', async () => {
    const queryClient = createTestQueryClient()
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue(undefined)
    renderHook(queryClient)
    const source = FakeEventSource.instances[0]

    source.emit('scan.progress', {
      protocol_version: 2,
      scan_job: scanJob,
      items: [completedScanItem],
    })
    await waitFor(() => {
      expect(screen.getByTestId('scan-runtime')).toHaveTextContent('completed')
    })

    source.emit('scan.finished', {
      protocol_version: 2,
      scan_job: {
        ...scanJob,
        status: 'success',
        phase: 'finished',
        scanned_files: 40,
        finished_at: '2026-07-14T00:00:30Z',
      },
      items: [completedScanItem],
      changes: [
        { resource: 'library:7:catalog', revision: 11 },
        { resource: 'library:7:scan', revision: 1 },
      ],
    })

    await waitFor(() => {
      const keys = invalidateSpy.mock.calls.map(([filters]) => filters?.queryKey)
      expect(keys).toEqual(expect.arrayContaining([['library', 7], ['library-media', 7], ['home']]))
      expect(screen.getByTestId('scan-runtime')).toHaveTextContent('{}')
    })
  })

  it('reconciles active scans when the durable scan revision changes', async () => {
    const queryClient = createTestQueryClient()
    renderHook(queryClient)
    const source = FakeEventSource.instances[0]

    source.emit('open')
    await waitFor(() => {
      expect(screen.getByTestId('scan-runtime')).toHaveTextContent('"library_id":7')
    })

    clientMocks.getRealtimeState.mockResolvedValue({
      ...baselineState,
      resources: {
        ...baselineState.resources,
        'library:7:scan': 1,
      },
      active_scans: [],
    })
    source.emit('resources.changed', {
      protocol_version: 2,
      changes: [{ resource: 'library:7:scan', revision: 1 }],
    })

    await waitFor(() => {
      expect(screen.getByTestId('scan-runtime')).toHaveTextContent('{}')
    })
  })

  it('does not reconcile durable state for every transient scan progress batch', async () => {
    const queryClient = createTestQueryClient()
    renderHook(queryClient)

    FakeEventSource.instances[0].emit('scan.progress', {
      protocol_version: 2,
      scan_job: scanJob,
      items: [completedScanItem],
    })

    await waitFor(() => {
      expect(screen.getByTestId('scan-runtime')).toHaveTextContent('completed')
    })
    await new Promise((resolve) => window.setTimeout(resolve, 20))
    expect(clientMocks.getRealtimeState).not.toHaveBeenCalled()
  })
})
