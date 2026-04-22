import type { ReactNode } from 'react'
import { useEffect, useRef, useState } from 'react'
import { translateCurrent } from '../../i18n'

interface ScrollableRailProps {
  children: ReactNode
  hint?: string
  resetKey?: number | string | null
  viewportClassName?: string
}

export const ScrollableRail = ({
  children,
  hint = translateCurrent('Scroll, drag, or click arrows to move sideways.'),
  resetKey,
  viewportClassName,
}: ScrollableRailProps) => {
  const listRef = useRef<HTMLDivElement | null>(null)
  const [canScrollLeft, setCanScrollLeft] = useState(false)
  const [canScrollRight, setCanScrollRight] = useState(false)

  const updateScrollState = () => {
    const list = listRef.current
    if (!list) {
      return
    }

    const maxLeft = Math.max(0, list.scrollWidth - list.clientWidth)
    setCanScrollLeft(list.scrollLeft > 8)
    setCanScrollRight(maxLeft - list.scrollLeft > 8)
  }

  useEffect(() => {
    const list = listRef.current
    if (!list) {
      return
    }

    const track = list.querySelector<HTMLElement>('.scrollable-rail__track')
    const resizeObserver = new ResizeObserver(() => updateScrollState())
    const handleWheel = (event: globalThis.WheelEvent) => {
      const maxLeft = Math.max(0, list.scrollWidth - list.clientWidth)
      if (maxLeft <= 0) {
        return
      }

      const primaryDelta =
        Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY

      if (Math.abs(primaryDelta) < 0.5) {
        return
      }

      const nextLeft = list.scrollLeft + primaryDelta
      const clampedLeft = Math.max(0, Math.min(maxLeft, nextLeft))

      if (Math.abs(clampedLeft - list.scrollLeft) < 0.5) {
        return
      }

      event.preventDefault()
      list.scrollLeft = clampedLeft
    }

    updateScrollState()
    resizeObserver.observe(list)
    if (track) {
      resizeObserver.observe(track)
    }
    list.addEventListener('scroll', updateScrollState, { passive: true })
    list.addEventListener('wheel', handleWheel, { passive: false })
    window.addEventListener('resize', updateScrollState)

    return () => {
      resizeObserver.disconnect()
      list.removeEventListener('scroll', updateScrollState)
      list.removeEventListener('wheel', handleWheel)
      window.removeEventListener('resize', updateScrollState)
    }
  }, [])

  useEffect(() => {
    updateScrollState()
  }, [children])

  useEffect(() => {
    const list = listRef.current
    if (!list) {
      return
    }

    list.scrollLeft = 0
    updateScrollState()
  }, [resetKey])

  const scrollList = (direction: -1 | 1) => {
    const list = listRef.current
    if (!list) {
      return
    }

    const distance = Math.max(260, Math.floor(list.clientWidth * 0.72))
    list.scrollBy({
      left: distance * direction,
      behavior: 'smooth',
    })
  }

  return (
    <div className="scrollable-rail">
      <div className="scrollable-rail__frame">
        <button
          aria-label={translateCurrent('Scroll left')}
          className="scrollable-rail__nav"
          disabled={!canScrollLeft}
          onClick={() => scrollList(-1)}
          type="button"
        >
          <span aria-hidden="true">‹</span>
        </button>

        <div
          className={
            viewportClassName
              ? `scrollable-rail__viewport ${viewportClassName}`
              : 'scrollable-rail__viewport'
          }
          ref={listRef}
        >
          <div className="scrollable-rail__track">{children}</div>
        </div>

        <button
          aria-label={translateCurrent('Scroll right')}
          className="scrollable-rail__nav"
          disabled={!canScrollRight}
          onClick={() => scrollList(1)}
          type="button"
        >
          <span aria-hidden="true">›</span>
        </button>
      </div>

      <p className="scrollable-rail__hint">{hint}</p>
    </div>
  )
}
