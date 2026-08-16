import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { Link, MemoryRouter, Outlet, Route, Routes } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { LibraryDetail, UserAccount } from '../../api/types'
import { I18nProvider } from '../../i18n'
import { LibraryPage } from '.'

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

const library: LibraryDetail = {
  id: 7,
  name: 'Movies',
  description: null,
  metadata_language: 'zh-CN',
  root_path: '/media/movies',
  media_count: 120,
  movie_count: 120,
  series_count: 0,
  last_scan: null,
  created_at: '2026-08-15T00:00:00Z',
  updated_at: '2026-08-15T00:00:00Z',
}

const successfulResponse = (data: unknown) =>
  new Response(JSON.stringify({ code: 200, data, message: 'ok' }), {
    headers: { 'Content-Type': 'application/json' },
    status: 200,
  })

const renderLibraryPage = () => {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })

  return render(
    <I18nProvider>
      <QueryClientProvider client={queryClient}>
        <MemoryRouter initialEntries={['/libraries/7']}>
          <Routes>
            <Route
              element={
                <>
                  <Link to="/libraries/8">Switch library</Link>
                  <Outlet
                    context={{
                      currentUser,
                      libraries: [library],
                      librariesLoading: false,
                      scanRuntimeByLibrary: {},
                    }}
                  />
                </>
              }
            >
              <Route element={<LibraryPage />} path="/libraries/:libraryId" />
            </Route>
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>
    </I18nProvider>,
  )
}

describe('LibraryPage filters', () => {
  beforeEach(() => {
    window.localStorage.setItem('mova.interfaceLanguage', 'en-US')
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    window.localStorage.clear()
  })

  it('queries the server for title, year, and rating order', async () => {
    const requestedPaths: string[] = []
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const path = String(input)
        requestedPaths.push(path)
        if (path === '/api/libraries/7') {
          return successfulResponse(library)
        }
        if (path.startsWith('/api/libraries/7/media-items?')) {
          return successfulResponse({ items: [], page: 1, page_size: 60, total: 0 })
        }
        throw new Error(`Unexpected request: ${path}`)
      }),
    )

    renderLibraryPage()
    expect(await screen.findByRole('heading', { name: 'Movies' })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Filter media' }))
    fireEvent.change(screen.getByPlaceholderText('Search title or original title'), {
      target: { value: 'Dune' },
    })
    fireEvent.change(screen.getByPlaceholderText('All years'), {
      target: { value: '2024' },
    })
    fireEvent.blur(screen.getByPlaceholderText('All years'))
    fireEvent.click(screen.getByRole('button', { name: 'Needs review' }))
    fireEvent.click(screen.getByRole('button', { name: 'Sort media' }))
    fireEvent.click(await screen.findByRole('menuitemradio', { name: 'Rating' }))

    await waitFor(() =>
      expect(
        requestedPaths.some(
          (path) =>
            path.includes('query=Dune') &&
            path.includes('category=needs_review') &&
            path.includes('year=2024') &&
            path.includes('sort_by=rating') &&
            path.includes('sort_order=desc'),
        ),
      ).toBe(true),
    )
    expect(screen.queryByRole('dialog', { name: 'Filter' })).not.toBeInTheDocument()
    expect(screen.getByTitle('3 active filters')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Sort media' })).toHaveTextContent('Rating')
  })

  it('reports an invalid year without silently changing the server filter', async () => {
    const requestedPaths: string[] = []
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const path = String(input)
        requestedPaths.push(path)
        if (path === '/api/libraries/7') {
          return successfulResponse(library)
        }
        if (path.startsWith('/api/libraries/7/media-items?')) {
          return successfulResponse({ items: [], page: 1, page_size: 60, total: 0 })
        }
        throw new Error(`Unexpected request: ${path}`)
      }),
    )

    renderLibraryPage()
    await screen.findByRole('heading', { name: 'Movies' })
    fireEvent.click(screen.getByRole('button', { name: 'Filter media' }))
    fireEvent.change(screen.getByPlaceholderText('All years'), { target: { value: '0' } })
    fireEvent.blur(screen.getByPlaceholderText('All years'))

    expect(await screen.findByText('Enter a valid year.')).toBeInTheDocument()
    expect(requestedPaths.some((path) => path.includes('year=0'))).toBe(false)
  })

  it('clears library-specific filters but preserves the chosen ordering between libraries', async () => {
    const requestedPaths: string[] = []
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const path = String(input)
        requestedPaths.push(path)
        if (path === '/api/libraries/7') {
          return successfulResponse(library)
        }
        if (path === '/api/libraries/8') {
          return successfulResponse({ ...library, id: 8, name: 'Series' })
        }
        if (/\/api\/libraries\/(7|8)\/media-items\?/.test(path)) {
          return successfulResponse({ items: [], page: 1, page_size: 60, total: 0 })
        }
        throw new Error(`Unexpected request: ${path}`)
      }),
    )

    renderLibraryPage()
    await screen.findByRole('heading', { name: 'Movies' })
    fireEvent.click(screen.getByRole('button', { name: 'Filter media' }))
    fireEvent.change(screen.getByPlaceholderText('Search title or original title'), {
      target: { value: 'Dune' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Needs review' }))
    fireEvent.click(screen.getByRole('button', { name: 'Sort media' }))
    fireEvent.click(await screen.findByRole('menuitemradio', { name: 'Rating' }))
    fireEvent.click(screen.getByRole('link', { name: 'Switch library' }))

    await waitFor(() =>
      expect(
        requestedPaths.some(
          (path) =>
            path.startsWith('/api/libraries/8/media-items?') &&
            path.includes('sort_by=rating') &&
            path.includes('sort_order=desc') &&
            !path.includes('query=') &&
            !path.includes('category=') &&
            !path.includes('year='),
        ),
      ).toBe(true),
    )
  })

  it('resets filters without changing the selected ordering', async () => {
    const requestedPaths: string[] = []
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const path = String(input)
        requestedPaths.push(path)
        if (path === '/api/libraries/7') {
          return successfulResponse(library)
        }
        if (path.startsWith('/api/libraries/7/media-items?')) {
          return successfulResponse({ items: [], page: 1, page_size: 60, total: 0 })
        }
        throw new Error(`Unexpected request: ${path}`)
      }),
    )

    renderLibraryPage()
    await screen.findByRole('heading', { name: 'Movies' })
    fireEvent.click(screen.getByRole('button', { name: 'Sort media' }))
    fireEvent.click(await screen.findByRole('menuitemradio', { name: 'Rating' }))
    fireEvent.click(screen.getByRole('button', { name: 'Filter media' }))
    fireEvent.change(screen.getByPlaceholderText('Search title or original title'), {
      target: { value: 'Dune' },
    })

    await waitFor(() =>
      expect(requestedPaths.some((path) => path.includes('query=Dune'))).toBe(true),
    )
    fireEvent.click(screen.getByRole('button', { name: 'Reset' }))

    await waitFor(() =>
      expect(
        requestedPaths.some(
          (path) =>
            path.startsWith('/api/libraries/7/media-items?') &&
            path.includes('sort_by=rating') &&
            path.includes('sort_order=desc') &&
            !path.includes('query='),
        ),
      ).toBe(true),
    )
    expect(screen.getByRole('button', { name: 'Sort media' })).toHaveTextContent('Rating')
  })
})
