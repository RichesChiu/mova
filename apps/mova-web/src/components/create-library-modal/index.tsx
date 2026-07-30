import type { CreateLibraryInput } from '../../api/types'
import { useI18n } from '../../i18n'
import { CreateLibraryForm } from '../create-library-form'
import { GlassDialog, GlassDialogCloseButton } from '../glass-dialog'

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
  return (
    <GlassDialog
      ariaLabel={l('Create Library')}
      className="create-library-modal"
      closeLabel={l('Close create library dialog')}
      isCloseDisabled={isSubmitting}
      isOpen={isOpen}
      onClose={onClose}
      surfaceClassName="create-library-modal__surface scrollbar-thin"
    >
      <div className="create-library-modal__header">
        <div>
          <h3>{l('Create Library')}</h3>
        </div>

        <GlassDialogCloseButton
          ariaLabel={l('Close create library dialog')}
          className="create-library-modal__close"
          disabled={isSubmitting}
          onClick={onClose}
        />
      </div>

      <CreateLibraryForm error={error} isSubmitting={isSubmitting} onSubmit={onSubmit} />
    </GlassDialog>
  )
}
