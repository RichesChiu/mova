import type { ReactNode } from 'react'
import { useEffect, useRef, useState } from 'react'

interface ScrollableRailProps {
  children: ReactNode
  hint?: string
  viewportClassName?: string
}

export const ScrollableRail = ({
  children,
  hint = 'Drag or click arrows to scroll horizontally.',
  viewportClassName,
}: ScrollableRailProps) => {
  const listRef = useRef<HTMLDivElement | null>(null)
  const [canScrollLeft, setCanScrollLeft] = useState(false)
  const [canScrollRight, setCanScrollRight] = useState(false)

  useEffect(() => {
    const list = listRef.current
    if (!list) {
      return
    }

    const updateScrollState = () => {
      const maxLeft = Math.max(0, list.scrollWidth - list.clientWidth)
      setCanScrollLeft(list.scrollLeft > 8)
      setCanScrollRight(maxLeft - list.scrollLeft > 8)
    }

    updateScrollState()
    list.addEventListener('scroll', updateScrollState, { passive: true })
    window.addEventListener('resize', updateScrollState)

    return () => {
      list.removeEventListener('scroll', updateScrollState)
      window.removeEventListener('resize', updateScrollState)
    }
  }, [])

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
          aria-label="Scroll left"
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
          {children}
        </div>

        <button
          aria-label="Scroll right"
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
