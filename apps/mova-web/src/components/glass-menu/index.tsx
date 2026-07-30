import { type ReactNode, type Ref, useEffect, useRef } from 'react'
import { usePresenceTransition } from '../../lib/use-presence-transition'

export interface GlassMenuTriggerProps {
  'aria-expanded': boolean
  'aria-haspopup': 'menu'
  onClick: () => void
  ref: Ref<HTMLButtonElement>
}

interface GlassMenuProps {
  ariaLabel: string
  children: (closeMenu: () => void) => ReactNode
  id?: string
  isOpen: boolean
  onOpenChange: (isOpen: boolean) => void
  popoverClassName?: string
  rootClassName?: string
  trigger: (props: GlassMenuTriggerProps) => ReactNode
}

const getEnabledMenuItems = (root: HTMLElement) =>
  Array.from(
    root.querySelectorAll<HTMLElement>(
      '[role="menuitem"]:not([aria-disabled="true"]), [role="menuitemradio"]:not([aria-disabled="true"])',
    ),
  ).filter((item) => !item.hasAttribute('disabled'))

export const GlassMenu = ({
  ariaLabel,
  children,
  id,
  isOpen,
  onOpenChange,
  popoverClassName,
  rootClassName,
  trigger,
}: GlassMenuProps) => {
  const rootRef = useRef<HTMLDivElement | null>(null)
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const onOpenChangeRef = useRef(onOpenChange)
  const presence = usePresenceTransition(isOpen)

  onOpenChangeRef.current = onOpenChange

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const focusFrame = window.requestAnimationFrame(() => {
      const root = rootRef.current
      if (!root) {
        return
      }

      getEnabledMenuItems(root)[0]?.focus()
    })

    const handlePointerDown = (event: MouseEvent) => {
      if (event.target instanceof Node && rootRef.current?.contains(event.target)) {
        return
      }
      onOpenChangeRef.current(false)
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      const root = rootRef.current
      if (!root) {
        return
      }

      if (event.key === 'Escape') {
        event.preventDefault()
        onOpenChangeRef.current(false)
        triggerRef.current?.focus()
        return
      }

      if (event.key === 'Tab') {
        onOpenChangeRef.current(false)
        return
      }

      if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
        return
      }

      const items = getEnabledMenuItems(root)
      if (items.length === 0) {
        return
      }

      event.preventDefault()
      const activeElement = document.activeElement
      const activeIndex = activeElement instanceof HTMLElement ? items.indexOf(activeElement) : -1
      if (event.key === 'Home') {
        items[0]?.focus()
      } else if (event.key === 'End') {
        items.at(-1)?.focus()
      } else {
        const delta = event.key === 'ArrowDown' ? 1 : -1
        const nextIndex =
          activeIndex < 0
            ? delta > 0
              ? 0
              : items.length - 1
            : (activeIndex + delta + items.length) % items.length
        items[nextIndex]?.focus()
      }
    }

    document.addEventListener('mousedown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)
    return () => {
      window.cancelAnimationFrame(focusFrame)
      document.removeEventListener('mousedown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [isOpen])

  const closeMenu = () => onOpenChange(false)

  return (
    <div
      className={['glass-menu', rootClassName].filter(Boolean).join(' ')}
      data-state={isOpen ? 'open' : 'closed'}
      ref={rootRef}
    >
      {trigger({
        'aria-expanded': isOpen,
        'aria-haspopup': 'menu',
        onClick: () => onOpenChange(!isOpen),
        ref: triggerRef,
      })}
      {presence.shouldRender ? (
        <div
          aria-hidden={!isOpen}
          aria-label={ariaLabel}
          className={[
            'glass-menu__popover',
            'glass-popover-surface',
            'floating-transition',
            popoverClassName,
          ]
            .filter(Boolean)
            .join(' ')}
          data-state={presence.transitionState}
          id={id}
          inert={!isOpen}
          role="menu"
        >
          {children(closeMenu)}
        </div>
      ) : null}
    </div>
  )
}
