const ROUTE_CHUNK_RELOAD_STORAGE_KEY = 'mova.routeChunkReloadAttempted'

export interface LazyRouteRecovery {
  clearReloadAttempted: () => void
  hasReloadAttempted: () => boolean
  markReloadAttempted: () => boolean
  reload: () => void
}

const browserRecovery: LazyRouteRecovery = {
  clearReloadAttempted: () => {
    try {
      window.sessionStorage.removeItem(ROUTE_CHUNK_RELOAD_STORAGE_KEY)
    } catch {
      // Storage may be unavailable in privacy-restricted browser contexts.
    }
  },
  hasReloadAttempted: () => {
    try {
      return window.sessionStorage.getItem(ROUTE_CHUNK_RELOAD_STORAGE_KEY) === 'true'
    } catch {
      return true
    }
  },
  markReloadAttempted: () => {
    try {
      window.sessionStorage.setItem(ROUTE_CHUNK_RELOAD_STORAGE_KEY, 'true')
      return true
    } catch {
      return false
    }
  },
  reload: () => window.location.reload(),
}

const neverSettlingPromise = <Result>() => new Promise<Result>(() => {})

export const loadLazyRoute = async <Result>(
  loader: () => Promise<Result>,
  recovery: LazyRouteRecovery = browserRecovery,
): Promise<Result> => {
  try {
    const result = await loader()
    recovery.clearReloadAttempted()
    return result
  } catch (error) {
    if (!recovery.hasReloadAttempted() && recovery.markReloadAttempted()) {
      try {
        recovery.reload()
        return neverSettlingPromise<Result>()
      } catch {
        // Render the route error boundary if this environment cannot reload.
      }
    }

    throw error
  }
}

export const resetLazyRouteRecovery = () => browserRecovery.clearReloadAttempted()
