import type { ReactNode } from 'react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { translateCurrent } from '../../i18n'

interface ScrollableRailProps {
  children: ReactNode
  hint?: string
  resetKey?: number | string | null
  viewportClassName?: string
}

export const ScrollableRail = ({
  children,
  hint = translateCurrent('Use horizontal scrolling or click arrows to move sideways.'),
  resetKey,
  viewportClassName,
}: ScrollableRailProps) => {
  const listRef = useRef<HTMLDivElement | null>(null)
  const previousResetKeyRef = useRef(resetKey)
  const [canScrollLeft, setCanScrollLeft] = useState(false)
  const [canScrollRight, setCanScrollRight] = useState(false)

  const updateScrollState = useCallback(() => {
    const list = listRef.current
    if (!list) {
      return
    }

    const maxLeft = Math.max(0, list.scrollWidth - list.clientWidth)
    setCanScrollLeft(list.scrollLeft > 8)
    setCanScrollRight(maxLeft - list.scrollLeft > 8)
  }, [])

  useEffect(() => {
    const list = listRef.current
    if (!list) {
      return
    }

    const track = list.querySelector<HTMLElement>('.scrollable-rail__track')
    const resizeObserver = new ResizeObserver(() => updateScrollState())
    updateScrollState()
    resizeObserver.observe(list)
    if (track) {
      resizeObserver.observe(track)
    }
    list.addEventListener('scroll', updateScrollState, { passive: true })
    window.addEventListener('resize', updateScrollState)

    return () => {
      resizeObserver.disconnect()
      list.removeEventListener('scroll', updateScrollState)
      window.removeEventListener('resize', updateScrollState)
    }
  }, [updateScrollState])

  useEffect(() => {
    updateScrollState()
  })

  useEffect(() => {
    if (previousResetKeyRef.current === resetKey) {
      return
    }
    previousResetKeyRef.current = resetKey

    const list = listRef.current
    if (!list) {
      return
    }

    list.scrollLeft = 0
    updateScrollState()
  })

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
          <svg
            aria-hidden="true"
            fill="none"
            focusable="false"
            stroke="currentColor"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="2"
            viewBox="0 0 24 24"
          >
            <path d="m15 6-6 6 6 6" />
          </svg>
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
          <svg
            aria-hidden="true"
            fill="none"
            focusable="false"
            stroke="currentColor"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="2"
            viewBox="0 0 24 24"
          >
            <path d="m9 6 6 6-6 6" />
          </svg>
        </button>
      </div>

      <p className="scrollable-rail__hint">{hint}</p>
    </div>
  )
}
