import { type FormEvent, useEffect, useState } from 'react'
import { createPortal } from 'react-dom'

interface ChangePasswordModalProps {
  error: string | null
  isOpen: boolean
  isSubmitting: boolean
  onClose: () => void
  onSubmit: (input: { current_password: string; new_password: string }) => Promise<unknown>
}

export const ChangePasswordModal = ({
  error,
  isOpen,
  isSubmitting,
  onClose,
  onSubmit,
}: ChangePasswordModalProps) => {
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')

  useEffect(() => {
    if (!isOpen) {
      return
    }

    setCurrentPassword('')
    setNewPassword('')
    setConfirmPassword('')
  }, [isOpen])

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

  const passwordsDoNotMatch =
    newPassword.length > 0 && confirmPassword.length > 0 && newPassword !== confirmPassword

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    if (passwordsDoNotMatch) {
      return
    }

    await onSubmit({
      current_password: currentPassword,
      new_password: newPassword,
    })
    onClose()
  }

  return createPortal(
    <div className="change-password-modal">
      <button
        aria-label="Close change password dialog"
        className="change-password-modal__backdrop glass-overlay-backdrop"
        disabled={isSubmitting}
        onClick={onClose}
        type="button"
      />

      <div
        aria-modal="true"
        className="change-password-modal__surface glass-modal-surface"
        role="dialog"
      >
        <div className="change-password-modal__header">
          <div>
            <p className="eyebrow">Security</p>
            <h3>Reset Password</h3>
          </div>

          <button
            aria-label="Close change password dialog"
            className="change-password-modal__close"
            disabled={isSubmitting}
            onClick={onClose}
            type="button"
          >
            <svg
              aria-hidden="true"
              className="change-password-modal__close-icon"
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

        <form className="change-password-modal__form" onSubmit={handleSubmit}>
          <label className="field">
            <span>Current Password</span>
            <input
              autoComplete="current-password"
              onChange={(event) => setCurrentPassword(event.target.value)}
              type="password"
              value={currentPassword}
            />
          </label>

          <label className="field">
            <span>New Password</span>
            <input
              autoComplete="new-password"
              onChange={(event) => setNewPassword(event.target.value)}
              type="password"
              value={newPassword}
            />
          </label>

          <label className="field">
            <span>Confirm New Password</span>
            <input
              autoComplete="new-password"
              onChange={(event) => setConfirmPassword(event.target.value)}
              type="password"
              value={confirmPassword}
            />
          </label>

          {passwordsDoNotMatch ? (
            <p className="callout callout--danger">The new passwords do not match.</p>
          ) : null}

          {error ? <p className="callout callout--danger">{error}</p> : null}

          <div className="change-password-modal__footer">
            <button className="button" disabled={isSubmitting} onClick={onClose} type="button">
              Cancel
            </button>
            <button
              className="button button--primary"
              disabled={
                isSubmitting ||
                currentPassword.length === 0 ||
                newPassword.length < 8 ||
                confirmPassword.length < 8 ||
                passwordsDoNotMatch
              }
              type="submit"
            >
              {isSubmitting ? 'Updating…' : 'Update Password'}
            </button>
          </div>
        </form>
      </div>
    </div>,
    document.body,
  )
}
