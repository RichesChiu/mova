import { afterEach, describe, expect, it, vi } from 'vitest'
import { logout } from './client'

describe('API client request headers', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('does not declare an empty logout request as JSON', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ code: 0, data: null, message: 'logged out' }), {
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
})
