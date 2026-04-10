import { type ReactNode, useEffect, useId, useRef, useState } from 'react'
import { createPortal } from 'react-dom'

interface SectionHelpProps {
  detail: ReactNode
  placement?: 'bottom' | 'top'
  title: string
}

interface TooltipPosition {
  arrowLeft: number
  left: number
  placement: 'bottom' | 'top'
  top: number
}

const TOOLTIP_GAP_PX = 12
const VIEWPORT_MARGIN_PX = 12

export const SectionHelp = ({ detail, placement = 'top', title }: SectionHelpProps) => {
  const tooltipId = useId()
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const tooltipRef = useRef<HTMLSpanElement | null>(null)
  const [isOpen, setIsOpen] = useState(false)
  const [tooltipPosition, setTooltipPosition] = useState<TooltipPosition | null>(null)

  useEffect(() => {
    if (!isOpen) {
      setTooltipPosition(null)
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
      const viewportWidth = window.innerWidth
      const viewportHeight = window.innerHeight

      const canPlaceAbove =
        triggerRect.top >= tooltipRect.height + TOOLTIP_GAP_PX + VIEWPORT_MARGIN_PX
      const canPlaceBelow =
        viewportHeight - triggerRect.bottom >=
        tooltipRect.height + TOOLTIP_GAP_PX + VIEWPORT_MARGIN_PX

      const resolvedPlacement =
        placement === 'top'
          ? canPlaceAbove || !canPlaceBelow
            ? 'top'
            : 'bottom'
          : canPlaceBelow || !canPlaceAbove
            ? 'bottom'
            : 'top'

      const centeredLeft = triggerRect.left + triggerRect.width / 2 - tooltipRect.width / 2
      const maxLeft = viewportWidth - tooltipRect.width - VIEWPORT_MARGIN_PX
      const left = Math.max(VIEWPORT_MARGIN_PX, Math.min(centeredLeft, maxLeft))
      const top =
        resolvedPlacement === 'top'
          ? triggerRect.top - tooltipRect.height - TOOLTIP_GAP_PX
          : triggerRect.bottom + TOOLTIP_GAP_PX
      const arrowLeft = Math.max(
        18,
        Math.min(tooltipRect.width - 18, triggerRect.left + triggerRect.width / 2 - left),
      )

      setTooltipPosition({
        arrowLeft,
        left,
        placement: resolvedPlacement,
        top,
      })
    }

    updatePosition()
    window.addEventListener('resize', updatePosition)
    window.addEventListener('scroll', updatePosition, true)

    return () => {
      window.removeEventListener('resize', updatePosition)
      window.removeEventListener('scroll', updatePosition, true)
    }
  }, [isOpen, placement])

  const handleOpen = () => setIsOpen(true)
  const handleClose = () => setIsOpen(false)

  const tooltip =
    isOpen && typeof document !== 'undefined'
      ? createPortal(
          <span
            className={
              tooltipPosition?.placement === 'bottom'
                ? 'section-help__tooltip section-help__tooltip--portal section-help__tooltip--bottom'
                : 'section-help__tooltip section-help__tooltip--portal section-help__tooltip--top'
            }
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
          className="section-help__trigger"
          onBlur={handleClose}
          onFocus={handleOpen}
          onMouseEnter={handleOpen}
          onMouseLeave={handleClose}
          ref={triggerRef}
          type="button"
        >
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
        </button>
      </span>
      {tooltip}
    </>
  )
}
