import { afterEach, describe, expect, it, vi } from 'vitest'
import { getBootstrapStatus, logout } from './client'

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
