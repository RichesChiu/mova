import type { ReactNode } from 'react'
import { createPortal } from 'react-dom'
import {
  type PresenceTransitionState,
  usePresenceTransition,
} from '../../lib/use-presence-transition'

interface OverlayPortalProps {
  children: (transitionState: PresenceTransitionState) => ReactNode
  exitDurationMs?: number
  isPresent: boolean
}

export const OverlayPortal = ({ children, exitDurationMs, isPresent }: OverlayPortalProps) => {
  const presence = usePresenceTransition(isPresent, exitDurationMs)

  if (!presence.shouldRender) {
    return null
  }

  return createPortal(children(presence.transitionState), document.body)
}
