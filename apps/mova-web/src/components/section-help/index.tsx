import { type ReactNode, useEffect, useId, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { resolveTooltipPosition, type TooltipPosition } from '../../lib/tooltip-position'
import { usePresenceTransition } from '../../lib/use-presence-transition'

interface SectionHelpProps {
  detail: ReactNode
  placement?: 'bottom' | 'top'
  title: string
  variant?: 'help' | 'notice'
}

const TOOLTIP_GAP_PX = 12
const VIEWPORT_MARGIN_PX = 12

export const SectionHelp = ({
  detail,
  placement = 'top',
  title,
  variant = 'help',
}: SectionHelpProps) => {
  const tooltipId = useId()
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const tooltipRef = useRef<HTMLSpanElement | null>(null)
  const [isOpen, setIsOpen] = useState(false)
  const [tooltipPosition, setTooltipPosition] = useState<TooltipPosition | null>(null)
  const tooltipPresence = usePresenceTransition(isOpen, 140)

  useEffect(() => {
    if (!tooltipPresence.shouldRender) {
      return undefined
    }

    if (!isOpen) {
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

  const handleOpen = () => setIsOpen(true)
  const handleClose = () => setIsOpen(false)

  const tooltip =
    tooltipPresence.shouldRender && typeof document !== 'undefined'
      ? createPortal(
          <span
            className={
              tooltipPosition?.placement === 'bottom'
                ? 'section-help__tooltip section-help__tooltip--portal section-help__tooltip--bottom floating-transition'
                : 'section-help__tooltip section-help__tooltip--portal section-help__tooltip--top floating-transition'
            }
            data-state={tooltipPosition ? tooltipPresence.transitionState : 'closed'}
            id={tooltipId}
            ref={tooltipRef}
            role="tooltip"
            style={
              tooltipPosition
                ? {
                    left: tooltipPosition.left,
                    top: tooltipPosition.top,
                    ['--section-help-arrow-left' as string]: `${tooltipPosition.arrowLeft}px`,
                  }
                : {
                    left: -9999,
                    top: -9999,
                    visibility: 'hidden',
                  }
            }
          >
            {detail}
          </span>,
          document.body,
        )
      : null

  return (
    <>
      <span className="section-help">
        <button
          aria-describedby={isOpen ? tooltipId : undefined}
          aria-label={title}
          className={
            variant === 'notice'
              ? 'section-help__trigger section-help__trigger--notice'
              : 'section-help__trigger'
          }
          onBlur={handleClose}
          onFocus={handleOpen}
          onMouseEnter={handleOpen}
          onMouseLeave={handleClose}
          ref={triggerRef}
          type="button"
        >
          {variant === 'notice' ? (
            <svg aria-hidden="true" fill="none" focusable="false" viewBox="0 0 20 20">
              <circle cx="10" cy="10" r="8.25" stroke="currentColor" strokeWidth="1.5" />
              <path
                d="M10 5.75V11.05"
                stroke="currentColor"
                strokeLinecap="round"
                strokeWidth="1.7"
              />
              <circle cx="10" cy="14.25" fill="currentColor" r="0.95" />
            </svg>
          ) : (
            <svg aria-hidden="true" fill="none" focusable="false" viewBox="0 0 20 20">
              <circle cx="10" cy="10" r="8.25" stroke="currentColor" strokeWidth="1.5" />
              <path
                d="M7.9 7.55C8.14 6.56 8.97 5.9 10.04 5.9C11.31 5.9 12.16 6.71 12.16 7.81C12.16 8.66 11.73 9.14 10.82 9.71C10.06 10.18 9.74 10.56 9.74 11.3V11.56"
                stroke="currentColor"
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="1.5"
              />
              <circle cx="9.98" cy="14.18" fill="currentColor" r="0.9" />
            </svg>
          )}
        </button>
      </span>
      {tooltip}
    </>
  )
}
