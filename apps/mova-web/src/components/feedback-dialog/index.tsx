import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { useI18n } from '../../i18n'
import { usePresenceTransition } from '../../lib/use-presence-transition'

export type FeedbackDialogTone = 'success' | 'err' | 'warn'

interface FeedbackDialogProps {
  durationMs?: number
  isOpen: boolean
  message: string | null
  onClose: () => void
  title?: string
  tone?: FeedbackDialogTone
}

const DEFAULT_DURATION_MS = 3000

const toneTitleKey = {
  err: 'Error',
  success: 'Success',
  warn: 'Warning',
} satisfies Record<FeedbackDialogTone, string>

const toneMarker = {
  err: '×',
  success: '✓',
  warn: '!',
} satisfies Record<FeedbackDialogTone, string>

export const FeedbackDialog = ({
  durationMs = DEFAULT_DURATION_MS,
  isOpen,
  message,
  onClose,
  title,
  tone = 'success',
}: FeedbackDialogProps) => {
  const { l } = useI18n()
  const onCloseRef = useRef(onClose)
  const [visibleFeedback, setVisibleFeedback] = useState<{
    message: string
    title?: string
    tone: FeedbackDialogTone
  } | null>(() => (message ? { message, title, tone } : null))
  const dialogPresence = usePresenceTransition(isOpen && message !== null, 150)

  useEffect(() => {
    onCloseRef.current = onClose
  }, [onClose])

  useEffect(() => {
    if (isOpen && message) {
      setVisibleFeedback({ message, title, tone })
    }
  }, [isOpen, message, title, tone])

  useEffect(() => {
    if (!isOpen || !message || durationMs <= 0) {
      return
    }

    const timeoutId = window.setTimeout(() => onCloseRef.current(), durationMs)
    return () => window.clearTimeout(timeoutId)
  }, [durationMs, isOpen, message])

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onCloseRef.current()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [isOpen])

  if (!dialogPresence.shouldRender || !visibleFeedback) {
    return null
  }

  const visibleTone = visibleFeedback.tone
  const dialogTitle = visibleFeedback.title ?? l(toneTitleKey[visibleTone])
  const role = visibleTone === 'err' ? 'alert' : 'status'
  const ariaLive = visibleTone === 'err' ? 'assertive' : 'polite'

  return createPortal(
    <div
      className={`feedback-dialog feedback-dialog--${visibleTone}`}
      data-state={dialogPresence.transitionState}
    >
      <div
        aria-live={ariaLive}
        className="feedback-dialog__surface glass-popover-surface floating-transition"
        data-state={dialogPresence.transitionState}
        role={role}
      >
        <span aria-hidden="true" className="feedback-dialog__marker">
          {toneMarker[visibleTone]}
        </span>

        <div className="feedback-dialog__copy">
          <strong>{dialogTitle}</strong>
          <p>{visibleFeedback.message}</p>
        </div>

        <button
          aria-label={l('Close notification')}
          className="feedback-dialog__close"
          onClick={onClose}
          type="button"
        >
          <svg aria-hidden="true" fill="none" focusable="false" viewBox="0 0 16 16">
            <path
              d="M4 4L12 12M12 4L4 12"
              stroke="currentColor"
              strokeLinecap="round"
              strokeWidth="1.7"
            />
          </svg>
        </button>
      </div>
    </div>,
    document.body,
  )
}
