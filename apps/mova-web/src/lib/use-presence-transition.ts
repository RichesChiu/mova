import { useEffect, useState } from 'react'

export type PresenceTransitionState = 'closed' | 'open'

export const usePresenceTransition = (isPresent: boolean, exitDurationMs = 180) => {
  const [shouldRender, setShouldRender] = useState(isPresent)
  const transitionState: PresenceTransitionState = isPresent ? 'open' : 'closed'

  useEffect(() => {
    if (isPresent) {
      setShouldRender(true)
      return undefined
    }

    if (!shouldRender) {
      return undefined
    }

    const timeoutId = window.setTimeout(() => setShouldRender(false), exitDurationMs)
    return () => window.clearTimeout(timeoutId)
  }, [exitDurationMs, isPresent, shouldRender])

  return {
    shouldRender,
    transitionState,
  }
}
