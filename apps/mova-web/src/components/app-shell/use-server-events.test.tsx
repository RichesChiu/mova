import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useServerEvents } from './use-server-events'

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

  emit(type: string, event: Event = new Event(type)) {
    this.listeners.get(type)?.forEach((listener) => {
      listener(event)
    })
  }

  emitMessage(type: string, data: unknown) {
    const event = { data: JSON.stringify(data) } as MessageEvent<string>
    this.listeners.get(type)?.forEach((listener) => {
      listener(event as unknown as Event)
    })
  }
}

const createTestQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  })

const HookHarness = ({ enabled }: { enabled: boolean }) => {
  const location = useLocation()
  const scanRuntimeByLibrary = useServerEvents({ enabled })

  return (
    <>
      <div data-testid="pathname">{location.pathname}</div>
      <div data-testid="scan-runtime">{JSON.stringify(scanRuntimeByLibrary)}</div>
    </>
  )
}

describe('useServerEvents', () => {
  beforeEach(() => {
    FakeEventSource.instances = []
    vi.stubGlobal('EventSource', FakeEventSource as unknown as typeof EventSource)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('re-invalidates active queries after the SSE connection recovers', async () => {
    const queryClient = createTestQueryClient()
    const invalidateQueriesSpy = vi
      .spyOn(queryClient, 'invalidateQueries')
      .mockResolvedValue(undefined)

    render(
      <QueryClientProvider client={queryClient}>
        <MemoryRouter initialEntries={['/libraries/7']}>
          <Routes>
            <Route element={<HookHarness enabled />} path="*" />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    )

    const eventSource = FakeEventSource.instances[0]
    eventSource.emit('open')
    eventSource.emit('error')
    eventSource.emit('open')

    await waitFor(() => {
      expect(invalidateQueriesSpy).toHaveBeenCalledTimes(7)
    })

    const invalidatedQueryKeys = invalidateQueriesSpy.mock.calls.map(([filters]) =>
      JSON.stringify(filters?.queryKey ?? null),
    )

    expect(invalidatedQueryKeys).toEqual(
      expect.arrayContaining([
        JSON.stringify(['libraries']),
        JSON.stringify(['continue-watching']),
        JSON.stringify(['watch-history']),
        JSON.stringify(['home-library-detail']),
        JSON.stringify(['home-library-shelf']),
        JSON.stringify(['library', 7]),
        JSON.stringify(['library-media', 7]),
      ]),
    )
  })

  it('alerts and returns to the home page when the active library is deleted', async () => {
    const queryClient = createTestQueryClient()
    const alertSpy = vi.spyOn(window, 'alert').mockImplementation(() => {})

    render(
      <QueryClientProvider client={queryClient}>
        <MemoryRouter initialEntries={['/libraries/7']}>
          <Routes>
            <Route element={<HookHarness enabled />} path="*" />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    )

    const eventSource = FakeEventSource.instances[0]
    eventSource.emitMessage('library.deleted', {
      type: 'library.deleted',
      library_id: 7,
    })

    await waitFor(() => {
      expect(alertSpy).toHaveBeenCalledWith('当前媒体库已被删除。点击确认后将返回主页。')
      expect(screen.getByTestId('pathname')).toHaveTextContent('/')
    })
  })

  it('keeps discovered scan cards visible until post-finish refetches settle', async () => {
    const queryClient = createTestQueryClient()
    const invalidateResolvers: Array<() => void> = []

    vi.spyOn(queryClient, 'invalidateQueries').mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          invalidateResolvers.push(resolve)
        }),
    )

    render(
      <QueryClientProvider client={queryClient}>
        <MemoryRouter initialEntries={['/libraries/7']}>
          <Routes>
            <Route element={<HookHarness enabled />} path="*" />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    )

    const eventSource = FakeEventSource.instances[0]
    eventSource.emitMessage('scan.item.updated', {
      type: 'scan.item.updated',
      item: {
        scan_job_id: 41,
        library_id: 7,
        item_key: '/media/movies/Interstellar (2014)/Interstellar.mkv',
        media_type: 'movie',
        title: 'Interstellar',
        season_number: null,
        episode_number: null,
        item_index: 1,
        total_items: 1,
        stage: 'discovered',
        progress_percent: 6,
      },
    })
    eventSource.emitMessage('scan.item.updated', {
      type: 'scan.item.updated',
      item: {
        scan_job_id: 41,
        library_id: 7,
        item_key: '/media/series/Arcane/S01E01.mkv',
        media_type: 'episode',
        title: 'Arcane',
        season_number: 1,
        episode_number: 1,
        item_index: 2,
        total_items: 2,
        stage: 'metadata',
        progress_percent: 36,
      },
    })

    await waitFor(() => {
      expect(screen.getByTestId('scan-runtime')).toHaveTextContent('Interstellar')
      expect(screen.getByTestId('scan-runtime')).toHaveTextContent('Arcane')
    })

    eventSource.emitMessage('scan.job.finished', {
      type: 'scan.job.finished',
      scan_job: {
        id: 41,
        library_id: 7,
        status: 'success',
        phase: 'finished',
        total_files: 2,
        scanned_files: 2,
        created_at: '2026-04-03T00:00:00Z',
        started_at: '2026-04-03T00:00:05Z',
        finished_at: '2026-04-03T00:00:10Z',
        error_message: null,
      },
    })

    expect(screen.getByTestId('scan-runtime')).toHaveTextContent('Interstellar')
    expect(screen.getByTestId('scan-runtime')).toHaveTextContent('Arcane')

    invalidateResolvers.forEach((resolve) => {
      resolve()
    })

    await waitFor(() => {
      expect(screen.getByTestId('scan-runtime')).toHaveTextContent('{}')
    })
  })
})
