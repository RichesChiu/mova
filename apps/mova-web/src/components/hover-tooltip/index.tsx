import { type ReactNode, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import {
  resolveTooltipPosition,
  type TooltipPlacement,
  type TooltipPosition,
} from '../../lib/tooltip-position'
import { usePresenceTransition } from '../../lib/use-presence-transition'
import './hover-tooltip.scss'

interface HoverTooltipProps {
  children: ReactNode
  className?: string
  content: ReactNode
  placement?: TooltipPlacement
}

const TOOLTIP_GAP_PX = 12
const VIEWPORT_MARGIN_PX = 12

export const HoverTooltip = ({
  children,
  className,
  content,
  placement = 'top',
}: HoverTooltipProps) => {
  const triggerRef = useRef<HTMLDivElement | null>(null)
  const tooltipRef = useRef<HTMLDivElement | null>(null)
  const [isOpen, setIsOpen] = useState(false)
  const [tooltipPosition, setTooltipPosition] = useState<TooltipPosition | null>(null)
  const tooltipPresence = usePresenceTransition(isOpen, 140)

  useEffect(() => {
    if (!isOpen || !tooltipPresence.shouldRender) {
      return undefined
    }

    const updatePosition = () => {
      const trigger = triggerRef.current
      const tooltip = tooltipRef.current
      if (!trigger || !tooltip) {
        return
      }

      const triggerRect = trigger.getBoundingClientRect()
      const tooltipRect = tooltip.getBoundingClientRect()

      setTooltipPosition(
        resolveTooltipPosition({
          gap: TOOLTIP_GAP_PX,
          margin: VIEWPORT_MARGIN_PX,
          preferredPlacement: placement,
          tooltipHeight: tooltipRect.height,
          tooltipWidth: tooltipRect.width,
          triggerBottom: triggerRect.bottom,
          triggerCenterX: triggerRect.left + triggerRect.width / 2,
          triggerTop: triggerRect.top,
          viewportHeight: window.innerHeight,
          viewportWidth: window.innerWidth,
        }),
      )
    }

    updatePosition()
    window.addEventListener('resize', updatePosition)
    window.addEventListener('scroll', updatePosition, true)

    return () => {
      window.removeEventListener('resize', updatePosition)
      window.removeEventListener('scroll', updatePosition, true)
    }
  }, [isOpen, placement, tooltipPresence.shouldRender])

  useEffect(() => {
    if (!tooltipPresence.shouldRender) {
      setTooltipPosition(null)
    }
  }, [tooltipPresence.shouldRender])

  const tooltip =
    tooltipPresence.shouldRender && typeof document !== 'undefined'
      ? createPortal(
          <div
            className={`hover-tooltip__surface hover-tooltip__surface--${tooltipPosition?.placement ?? placement} floating-transition`}
            data-state={tooltipPosition ? tooltipPresence.transitionState : 'closed'}
            ref={tooltipRef}
            role="tooltip"
            style={
              tooltipPosition
                ? {
                    left: tooltipPosition.left,
                    top: tooltipPosition.top,
                    ['--hover-tooltip-arrow-left' as string]: `${tooltipPosition.arrowLeft}px`,
                  }
                : {
                    left: -9999,
                    top: -9999,
                    visibility: 'hidden',
                  }
            }
          >
            {content}
          </div>,
          document.body,
        )
      : null

  return (
    <>
      {/* biome-ignore lint/a11y/noStaticElementInteractions: the tooltip visually expands text that is already available to assistive technology */}
      <div
        className={['hover-tooltip', className].filter(Boolean).join(' ')}
        onMouseEnter={() => setIsOpen(true)}
        onMouseLeave={() => setIsOpen(false)}
        ref={triggerRef}
      >
        {children}
      </div>
      {tooltip}
    </>
  )
}
