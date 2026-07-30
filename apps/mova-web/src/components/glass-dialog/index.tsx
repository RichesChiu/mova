import { type ReactNode, type RefObject, useEffect, useRef } from 'react'
import { OverlayPortal } from '../overlay-portal'

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not(:disabled)',
  'input:not(:disabled)',
  'select:not(:disabled)',
  'textarea:not(:disabled)',
  '[tabindex]:not([tabindex="-1"])',
].join(',')

let scrollLockCount = 0
let bodyOverflowBeforeLock = ''

const lockBodyScroll = () => {
  if (scrollLockCount === 0) {
    bodyOverflowBeforeLock = document.body.style.overflow
    document.body.style.overflow = 'hidden'
  }
  scrollLockCount += 1
}

const unlockBodyScroll = () => {
  scrollLockCount = Math.max(0, scrollLockCount - 1)
  if (scrollLockCount === 0) {
    document.body.style.overflow = bodyOverflowBeforeLock
  }
}

interface GlassDialogProps {
  ariaLabel?: string
  ariaLabelledBy?: string
  children: ReactNode
  className?: string
  closeLabel: string
  isCloseDisabled?: boolean
  isOpen: boolean
  onClose: () => void
  surfaceClassName?: string
  surfaceRef?: RefObject<HTMLDivElement | null>
}

export const GlassDialog = ({
  ariaLabel,
  ariaLabelledBy,
  children,
  className,
  closeLabel,
  isCloseDisabled = false,
  isOpen,
  onClose,
  surfaceClassName,
  surfaceRef,
}: GlassDialogProps) => {
  const fallbackSurfaceRef = useRef<HTMLDivElement | null>(null)
  const activeSurfaceRef = surfaceRef ?? fallbackSurfaceRef
  const restoreFocusRef = useRef<HTMLElement | null>(null)
  const isCloseDisabledRef = useRef(isCloseDisabled)
  const onCloseRef = useRef(onClose)

  isCloseDisabledRef.current = isCloseDisabled
  onCloseRef.current = onClose

  useEffect(() => {
    if (!isOpen) {
      return
    }

    restoreFocusRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null
    lockBodyScroll()

    const focusFrame = window.requestAnimationFrame(() => {
      const surface = activeSurfaceRef.current
      const target =
        surface?.querySelector<HTMLElement>('[autofocus]') ??
        surface?.querySelector<HTMLElement>(FOCUSABLE_SELECTOR) ??
        surface
      target?.focus()
    })

    const handleKeyDown = (event: KeyboardEvent) => {
      const surface = activeSurfaceRef.current
      if (!surface) {
        return
      }

      if (event.key === 'Escape') {
        if (!isCloseDisabledRef.current) {
          event.preventDefault()
          onCloseRef.current()
        }
        return
      }

      if (event.key !== 'Tab') {
        return
      }

      const focusableElements = Array.from(
        surface.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
      ).filter((element) => !element.hasAttribute('disabled') && element.tabIndex >= 0)

      if (focusableElements.length === 0) {
        event.preventDefault()
        surface.focus()
        return
      }

      const first = focusableElements[0]
      const last = focusableElements[focusableElements.length - 1]
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last?.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first?.focus()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.cancelAnimationFrame(focusFrame)
      window.removeEventListener('keydown', handleKeyDown)
      unlockBodyScroll()
      restoreFocusRef.current?.focus()
      restoreFocusRef.current = null
    }
  }, [activeSurfaceRef, isOpen])

  return (
    <OverlayPortal isPresent={isOpen}>
      {(transitionState) => (
        <div
          className={['glass-dialog', 'overlay-transition', className].filter(Boolean).join(' ')}
          data-state={transitionState}
        >
          <button
            aria-label={closeLabel}
            className="glass-dialog__backdrop glass-overlay-backdrop"
            disabled={!isOpen || isCloseDisabled}
            onClick={onClose}
            tabIndex={-1}
            type="button"
          />
          <div
            aria-hidden={!isOpen}
            aria-label={ariaLabel}
            aria-labelledby={ariaLabelledBy}
            aria-modal="true"
            className={['glass-dialog__surface', 'glass-modal-surface', surfaceClassName]
              .filter(Boolean)
              .join(' ')}
            ref={activeSurfaceRef}
            inert={!isOpen}
            role="dialog"
            tabIndex={-1}
          >
            {children}
          </div>
        </div>
      )}
    </OverlayPortal>
  )
}

export const GlassDialogCloseButton = ({
  ariaLabel,
  className,
  disabled = false,
  onClick,
}: {
  ariaLabel: string
  className?: string
  disabled?: boolean
  onClick: () => void
}) => (
  <button
    aria-label={ariaLabel}
    className={['glass-dialog__close', className].filter(Boolean).join(' ')}
    disabled={disabled}
    onClick={onClick}
    type="button"
  >
    <svg aria-hidden="true" fill="none" focusable="false" viewBox="0 0 24 24">
      <path
        d="M6 6L18 18M18 6L6 18"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
    </svg>
  </button>
)
