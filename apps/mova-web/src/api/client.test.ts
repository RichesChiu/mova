import { afterEach, describe, expect, it, vi } from 'vitest'
import { getBootstrapStatus, listLibraryMediaItems, listNotifications, logout } from './client'

describe('API client request headers', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('does not declare an empty logout request as JSON', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ code: 200, data: null, message: 'logged out' }), {
        headers: { 'Content-Type': 'application/json' },
        status: 200,
      }),
    )
    vi.stubGlobal('fetch', fetchMock)

    await logout()

    expect(fetchMock).toHaveBeenCalledOnce()
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(new Headers(init.headers).has('Content-Type')).toBe(false)
  })

  it('requests only unread notifications when the client opts into the filter', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          code: 200,
          data: { items: [], total_unread: 0, unread_by_category: {} },
          message: 'ok',
        }),
        {
          headers: { 'Content-Type': 'application/json' },
          status: 200,
        },
      ),
    )
    vi.stubGlobal('fetch', fetchMock)

    await listNotifications({ unreadOnly: true })

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/notifications?limit=20&unread_only=true',
      expect.any(Object),
    )
  })

  it('serializes library media filters and ordering into the public API contract', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          code: 200,
          data: { items: [], page: 2, page_size: 24, total: 0 },
          message: 'ok',
        }),
        {
          headers: { 'Content-Type': 'application/json' },
          status: 200,
        },
      ),
    )
    vi.stubGlobal('fetch', fetchMock)

    await listLibraryMediaItems(7, {
      page: 2,
      pageSize: 24,
      category: 'needs_review',
      query: 'Dune',
      sortBy: 'rating',
      sortOrder: 'desc',
      year: 2024,
    })

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/libraries/7/media-items?page=2&page_size=24&query=Dune&category=needs_review&year=2024&sort_by=rating&sort_order=desc',
      expect.any(Object),
    )
  })

  it('normalizes fetch failures into a stable localizable network error', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')))

    await expect(getBootstrapStatus()).rejects.toMatchObject({
      diagnosticMessage: 'Failed to fetch',
      errorCode: 'network_error',
      status: 0,
    })
  })

  it('rejects malformed successful envelopes with a stable error', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ code: 200, data: {}, message: null }), {
          headers: { 'Content-Type': 'application/json' },
          status: 200,
        }),
      ),
    )

    await expect(getBootstrapStatus()).rejects.toMatchObject({
      errorCode: 'invalid_response',
      status: 200,
    })
  })

  it('falls back to the HTTP error contract when error params are malformed', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            code: 429,
            data: null,
            error_code: 'rate_limited',
            message: 'upstream diagnostic',
            params: 'invalid',
          }),
          {
            headers: { 'Content-Type': 'application/json' },
            status: 429,
            statusText: 'Too Many Requests',
          },
        ),
      ),
    )

    await expect(getBootstrapStatus()).rejects.toMatchObject({
      errorCode: 'rate_limited',
      params: {},
      status: 429,
    })
  })
})
