import { describe, expect, it, vi } from 'vitest'
import { type LazyRouteRecovery, loadLazyRoute } from './lazy-route'

const createRecovery = (reloadAttempted = false) => {
  let attempted = reloadAttempted
  const recovery: LazyRouteRecovery = {
    clearReloadAttempted: vi.fn(() => {
      attempted = false
    }),
    hasReloadAttempted: vi.fn(() => attempted),
    markReloadAttempted: vi.fn(() => {
      attempted = true
      return true
    }),
    reload: vi.fn(),
  }

  return recovery
}

describe('lazy route recovery', () => {
  it('clears the reload fence after a successful chunk load', async () => {
    const recovery = createRecovery(true)

    await expect(loadLazyRoute(async () => 'loaded', recovery)).resolves.toBe('loaded')
    expect(recovery.clearReloadAttempted).toHaveBeenCalledTimes(1)
  })

  it('reloads once and leaves the first failed import pending while the page unloads', async () => {
    const recovery = createRecovery()
    const routePromise = loadLazyRoute(
      async () => Promise.reject(new Error('chunk unavailable')),
      recovery,
    )

    await vi.waitFor(() => expect(recovery.reload).toHaveBeenCalledTimes(1))
    expect(recovery.markReloadAttempted).toHaveBeenCalledTimes(1)

    const state = await Promise.race([
      routePromise.then(
        () => 'resolved',
        () => 'rejected',
      ),
      new Promise<string>((resolve) => window.setTimeout(() => resolve('pending'), 0)),
    ])
    expect(state).toBe('pending')
  })

  it('rejects to the error boundary after the automatic reload was already attempted', async () => {
    const recovery = createRecovery(true)
    const error = new Error('chunk still unavailable')

    await expect(loadLazyRoute(async () => Promise.reject(error), recovery)).rejects.toBe(error)
    expect(recovery.reload).not.toHaveBeenCalled()
    expect(recovery.markReloadAttempted).not.toHaveBeenCalled()
  })
})
