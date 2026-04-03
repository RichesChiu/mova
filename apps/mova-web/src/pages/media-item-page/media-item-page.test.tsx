import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { MemoryRouter, Outlet, Route, Routes } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AppShellOutletContext } from '../../components/app-shell'
import { MediaItemPage } from './index'

const clientMocks = vi.hoisted(() => ({
  getMediaItem: vi.fn(),
  getMediaItemEpisodeOutline: vi.fn(),
  getMediaItemPlaybackHeader: vi.fn(),
  getMediaItemPlaybackProgress: vi.fn(),
}))

vi.mock('../../api/client', () => ({
  getMediaItem: clientMocks.getMediaItem,
  getMediaItemEpisodeOutline: clientMocks.getMediaItemEpisodeOutline,
  getMediaItemPlaybackHeader: clientMocks.getMediaItemPlaybackHeader,
  getMediaItemPlaybackProgress: clientMocks.getMediaItemPlaybackProgress,
}))

const createTestQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  })

const outletContext: AppShellOutletContext = {
  currentUser: {
    id: 1,
    username: 'viewer01',
    role: 'viewer',
    is_enabled: true,
    library_ids: [],
    created_at: '2026-04-03T08:00:00Z',
    updated_at: '2026-04-03T08:00:00Z',
  },
  libraries: [],
  librariesLoading: false,
  scanRuntimeByLibrary: {},
}

const OutletHarness = () => <Outlet context={outletContext} />

describe('MediaItemPage playback entry actions', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    clientMocks.getMediaItem.mockResolvedValue({
      id: 31,
      library_id: 7,
      media_type: 'movie',
      title: 'Interstellar',
      source_title: 'Interstellar',
      original_title: 'Interstellar',
      sort_title: 'Interstellar',
      year: 2014,
      overview: 'A science fiction film.',
      poster_path: null,
      backdrop_path: null,
      created_at: '2026-04-03T08:00:00Z',
      updated_at: '2026-04-03T08:00:00Z',
      cast: [],
    })
    clientMocks.getMediaItemEpisodeOutline.mockResolvedValue({ seasons: [] })
    clientMocks.getMediaItemPlaybackHeader.mockResolvedValue(null)
  })

  it('shows both resume and from-start actions when resumable progress exists', async () => {
    clientMocks.getMediaItemPlaybackProgress.mockResolvedValue({
      id: 99,
      media_item_id: 31,
      media_file_id: 401,
      position_seconds: 320,
      duration_seconds: 7200,
      last_watched_at: '2026-04-03T08:00:00Z',
      is_finished: false,
    })

    render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MemoryRouter initialEntries={['/media-items/31']}>
          <Routes>
            <Route element={<OutletHarness />}>
              <Route element={<MediaItemPage />} path="/media-items/:mediaItemId" />
            </Route>
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    )

    expect(await screen.findByRole('link', { name: 'Resume Playback' })).toHaveAttribute(
      'href',
      '/media-items/31/play',
    )
    expect(await screen.findByRole('link', { name: 'Play from Beginning' })).toHaveAttribute(
      'href',
      '/media-items/31/play?fromStart=1',
    )
  })

  it('only shows the default play action when no resume point exists', async () => {
    clientMocks.getMediaItemPlaybackProgress.mockResolvedValue(null)

    render(
      <QueryClientProvider client={createTestQueryClient()}>
        <MemoryRouter initialEntries={['/media-items/31']}>
          <Routes>
            <Route element={<OutletHarness />}>
              <Route element={<MediaItemPage />} path="/media-items/:mediaItemId" />
            </Route>
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    )

    expect(await screen.findByRole('link', { name: 'Play' })).toHaveAttribute(
      'href',
      '/media-items/31/play',
    )
    expect(screen.queryByRole('link', { name: 'Play from Beginning' })).not.toBeInTheDocument()
  })
})
