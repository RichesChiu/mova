import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { MemoryRouter, Outlet, Route, Routes } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { MediaItemDetail, UserAccount } from '../../api/types'
import { I18nProvider } from '../../i18n'
import { MediaItemPage } from '.'

const currentUser: UserAccount = {
  id: 1,
  username: 'viewer',
  nickname: 'Viewer',
  role: 'viewer',
  is_enabled: true,
  library_ids: [7],
  created_at: '2026-08-15T00:00:00Z',
  updated_at: '2026-08-15T00:00:00Z',
}

const movie: MediaItemDetail = {
  id: 5,
  library_id: 7,
  media_type: 'movie',
  title: 'Dune',
  source_title: 'Dune',
  original_title: 'Dune',
  sort_title: null,
  metadata_provider: null,
  metadata_provider_item_id: null,
  metadata_status: 'unmatched',
  metadata_failure_reason: null,
  remote_media_type: null,
  year: 2021,
  tagline: null,
  premiere_date: null,
  content_rating: null,
  ratings: [],
  country: null,
  genres: null,
  studio: null,
  overview: null,
  poster_path: null,
  backdrop_path: null,
  logo_path: null,
  created_at: '2026-08-15T00:00:00Z',
  updated_at: '2026-08-15T00:00:00Z',
  source_status: 'missing',
  missing_media_file_ids: [11],
  rescan_scan_job_id: 42,
}

const successfulResponse = (data: unknown) =>
  new Response(JSON.stringify({ code: 200, data, message: 'ok' }), {
    headers: { 'Content-Type': 'application/json' },
    status: 200,
  })

const renderMediaItemPage = () => {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })

  return render(
    <I18nProvider>
      <QueryClientProvider client={queryClient}>
        <MemoryRouter initialEntries={['/media-items/5']}>
          <Routes>
            <Route
              element={
                <Outlet
                  context={{
                    currentUser,
                    libraries: [],
                    librariesLoading: false,
                    scanRuntimeByLibrary: {},
                  }}
                />
              }
            >
              <Route element={<MediaItemPage />} path="/media-items/:mediaItemId" />
            </Route>
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>
    </I18nProvider>,
  )
}

describe('MediaItemPage source availability', () => {
  beforeEach(() => {
    window.localStorage.setItem('mova.interfaceLanguage', 'en-US')
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    window.localStorage.clear()
  })

  it('shows the missing notice instead of the details when every version is gone', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const path = String(input)
        if (path === '/api/media-items/5') {
          return successfulResponse(movie)
        }
        return successfulResponse([])
      }),
    )

    renderMediaItemPage()

    expect(await screen.findByText('Resource no longer exists')).toBeInTheDocument()
    expect(screen.getByText('The files of Dune were not found on disk.')).toBeInTheDocument()
    expect(
      screen.getByText(
        'The library is being rescanned. This entry is updated or removed when the scan finishes.',
      ),
    ).toBeInTheDocument()
    expect(screen.queryByRole('link', { name: 'Play' })).not.toBeInTheDocument()
  })

  it('explains how the entry comes back when no rescan was queued', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const path = String(input)
        if (path === '/api/media-items/5') {
          return successfulResponse({ ...movie, rescan_scan_job_id: null })
        }
        return successfulResponse([])
      }),
    )

    renderMediaItemPage()

    expect(
      await screen.findByText(
        'Rescan the library after restoring the files to bring this entry back.',
      ),
    ).toBeInTheDocument()
  })
})
