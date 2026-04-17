import { useEffect } from 'react'
import { createPortal } from 'react-dom'

interface ConfirmActionModalProps {
  confirmLabel: string
  description: string
  error: string | null
  isOpen: boolean
  isSubmitting: boolean
  onClose: () => void
  onConfirm: () => void
  title: string
}

export const ConfirmActionModal = ({
  confirmLabel,
  description,
  error,
  isOpen,
  isSubmitting,
  onClose,
  onConfirm,
  title,
}: ConfirmActionModalProps) => {
  useEffect(() => {
    if (!isOpen) {
      return
    }

    const previousOverflow = document.body.style.overflow
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !isSubmitting) {
        onClose()
      }
    }

    document.body.style.overflow = 'hidden'
    window.addEventListener('keydown', handleKeyDown)

    return () => {
      document.body.style.overflow = previousOverflow
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [isOpen, isSubmitting, onClose])

  if (!isOpen) {
    return null
  }

  return createPortal(
    <div className="confirm-action-modal">
      <button
        aria-label="Close confirmation dialog"
        className="confirm-action-modal__backdrop glass-overlay-backdrop"
        disabled={isSubmitting}
        onClick={onClose}
        type="button"
      />

      <div
        aria-modal="true"
        className="confirm-action-modal__surface glass-modal-surface"
        role="dialog"
      >
        <div className="confirm-action-modal__header">
          <div>
            <p className="eyebrow">Confirm Action</p>
            <h3>{title}</h3>
            <p className="muted">{description}</p>
          </div>
        </div>

        {error ? <p className="callout callout--danger">{error}</p> : null}

        <div className="confirm-action-modal__footer">
          <button className="button" disabled={isSubmitting} onClick={onClose} type="button">
            Cancel
          </button>
          <button
            className="button button--danger"
            disabled={isSubmitting}
            onClick={onConfirm}
            type="button"
          >
            {isSubmitting ? 'Working…' : confirmLabel}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  )
}
