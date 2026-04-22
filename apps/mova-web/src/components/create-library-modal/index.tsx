import { useEffect } from 'react'
import { createPortal } from 'react-dom'
import type { CreateLibraryInput } from '../../api/types'
import { useI18n } from '../../i18n'
import { CreateLibraryForm } from '../create-library-form'

interface CreateLibraryModalProps {
  error: string | null
  isOpen: boolean
  isSubmitting: boolean
  onClose: () => void
  onSubmit: (input: CreateLibraryInput) => Promise<unknown>
}

export const CreateLibraryModal = ({
  error,
  isOpen,
  isSubmitting,
  onClose,
  onSubmit,
}: CreateLibraryModalProps) => {
  const { l } = useI18n()
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
    <div className="create-library-modal">
      <button
        aria-label={l('Close create library dialog')}
        className="create-library-modal__backdrop glass-overlay-backdrop"
        disabled={isSubmitting}
        onClick={onClose}
        type="button"
      />

      <div
        aria-modal="true"
        className="create-library-modal__surface glass-modal-surface"
        role="dialog"
      >
        <div className="create-library-modal__header">
          <div>
            <p className="eyebrow">{l('Library Management')}</p>
            <h3>{l('Create Library')}</h3>
          </div>

          <button
            aria-label={l('Close create library dialog')}
            className="create-library-modal__close"
            disabled={isSubmitting}
            onClick={onClose}
            type="button"
          >
            <svg
              aria-hidden="true"
              className="create-library-modal__close-icon"
              fill="none"
              viewBox="0 0 24 24"
            >
              <path
                d="M6 6L18 18M18 6L6 18"
                stroke="currentColor"
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="1.8"
              />
            </svg>
          </button>
        </div>

        <CreateLibraryForm error={error} isSubmitting={isSubmitting} onSubmit={onSubmit} />
      </div>
    </div>,
    document.body,
  )
}
