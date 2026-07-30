import { type FormEvent, useEffect, useState } from 'react'
import { useI18n } from '../../i18n'
import { GlassDialog, GlassDialogCloseButton } from '../glass-dialog'

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
  const { l } = useI18n()
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

  return (
    <GlassDialog
      ariaLabel={l('Reset Password')}
      className="change-password-modal"
      closeLabel={l('Close change password dialog')}
      isCloseDisabled={isSubmitting}
      isOpen={isOpen}
      onClose={onClose}
      surfaceClassName="change-password-modal__surface"
    >
      <div className="change-password-modal__header">
        <div>
          <p className="eyebrow">{l('Security')}</p>
          <h3>{l('Reset Password')}</h3>
        </div>

        <GlassDialogCloseButton
          ariaLabel={l('Close change password dialog')}
          className="change-password-modal__close"
          disabled={isSubmitting}
          onClick={onClose}
        />
      </div>

      <form className="change-password-modal__form" onSubmit={handleSubmit}>
        <label className="field">
          <span>{l('Current Password')}</span>
          <input
            autoComplete="current-password"
            onChange={(event) => setCurrentPassword(event.target.value)}
            type="password"
            value={currentPassword}
          />
        </label>

        <label className="field">
          <span>{l('New Password')}</span>
          <input
            autoComplete="new-password"
            onChange={(event) => setNewPassword(event.target.value)}
            type="password"
            value={newPassword}
          />
        </label>

        <label className="field">
          <span>{l('Confirm New Password')}</span>
          <input
            autoComplete="new-password"
            onChange={(event) => setConfirmPassword(event.target.value)}
            type="password"
            value={confirmPassword}
          />
        </label>

        {passwordsDoNotMatch ? (
          <p className="callout callout--danger">{l('The new passwords do not match.')}</p>
        ) : null}

        {error ? <p className="callout callout--danger">{error}</p> : null}

        <div className="change-password-modal__footer">
          <button className="button" disabled={isSubmitting} onClick={onClose} type="button">
            {l('Cancel')}
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
            {isSubmitting ? l('Updating…') : l('Update Password')}
          </button>
        </div>
      </form>
    </GlassDialog>
  )
}
