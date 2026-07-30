import { useI18n } from '../../i18n'
import { GlassDialog } from '../glass-dialog'

interface ConfirmActionModalProps {
  confirmLabel: string
  confirmTone?: 'danger' | 'primary'
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
  confirmTone = 'danger',
  description,
  error,
  isOpen,
  isSubmitting,
  onClose,
  onConfirm,
  title,
}: ConfirmActionModalProps) => {
  const { l } = useI18n()
  return (
    <GlassDialog
      ariaLabel={title}
      className="confirm-action-modal"
      closeLabel={l('Close confirmation dialog')}
      isCloseDisabled={isSubmitting}
      isOpen={isOpen}
      onClose={onClose}
      surfaceClassName="confirm-action-modal__surface"
    >
      <div className="confirm-action-modal__header">
        <div>
          <p className="eyebrow">{l('Confirm Action')}</p>
          <h3>{title}</h3>
          <p className="muted">{description}</p>
        </div>
      </div>

      {error ? <p className="callout callout--danger">{error}</p> : null}

      <div className="confirm-action-modal__footer">
        <button className="button" disabled={isSubmitting} onClick={onClose} type="button">
          {l('Cancel')}
        </button>
        <button
          className={`button button--${confirmTone}`}
          disabled={isSubmitting}
          onClick={onConfirm}
          type="button"
        >
          {isSubmitting ? l('Working…') : confirmLabel}
        </button>
      </div>
    </GlassDialog>
  )
}
