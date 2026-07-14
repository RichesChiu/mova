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

const baselineState = {
  protocol_version: 1,
  server_epoch: 'server-a',
  resources: {
    libraries: 2,
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
    clientMocks.getRealtimeState.mockResolvedValue(baselineState)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('restores active scan state without invalidating the first revision snapshot', async () => {
    const queryClient = createTestQueryClient()
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue(undefined)
    renderHook(queryClient)

    FakeEventSource.instances[0].emit('open')

    await waitFor(() => {
      expect(screen.getByTestId('scan-runtime')).toHaveTextContent('"library_id":7')
    })
    expect(invalidateSpy).not.toHaveBeenCalled()
  })

  it('refreshes only the changed catalog resource and ignores duplicate revisions', async () => {
    const queryClient = createTestQueryClient()
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue(undefined)
    renderHook(queryClient, '/media-items/31')
    const source = FakeEventSource.instances[0]
    source.emit('open')
    await waitFor(() => expect(clientMocks.getRealtimeState).toHaveBeenCalled())

    const payload = {
      version: 1,
      changes: [{ resource: 'library:7:catalog', revision: 11 }],
    }
    source.emit('resources.changed', payload)

    await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(5))
    const keys = invalidateSpy.mock.calls.map(([filters]) => filters?.queryKey)
    expect(keys).toEqual(
      expect.arrayContaining([
        ['library', 7],
        ['library-media', 7],
        ['home'],
        ['media-item', 31],
        ['media-episode-outline', 31],
      ]),
    )

    source.emit('resources.changed', payload)
    await new Promise((resolve) => window.setTimeout(resolve, 10))
    expect(invalidateSpy).toHaveBeenCalledTimes(5)
  })

  it('uses the cached home revisions as the reconnect baseline', async () => {
    const queryClient = createTestQueryClient()
    queryClient.setQueryData(['home'], {
      realtime: {
        server_epoch: 'server-a',
        resources: { 'library:7:catalog': 9 },
      },
    })
    clientMocks.getRealtimeState.mockResolvedValue({
      ...baselineState,
      resources: { 'library:7:catalog': 10 },
      active_scans: [],
    })
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue(undefined)
    renderHook(queryClient)

    FakeEventSource.instances[0].emit('open')

    await waitFor(() => {
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['home'] })
    })
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['library', 7] })
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['library-media', 7] })
  })

  it('stores a server-batched scan progress payload in one runtime update', async () => {
    const queryClient = createTestQueryClient()
    renderHook(queryClient)
    const source = FakeEventSource.instances[0]

    source.emit('scan.progress', {
      version: 1,
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
})
