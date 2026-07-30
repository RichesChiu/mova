import { type FormEvent, useEffect, useState } from 'react'
import type { Library, UpdateLibraryInput } from '../../api/types'
import { useI18n } from '../../i18n'
import {
  buildLibraryEditorDraft,
  buildLibraryUpdateInput,
  hasLibraryConfigChanges,
  hasLibraryMetadataLanguageChanged,
  LIBRARY_DESCRIPTION_MAX_LENGTH,
} from '../../lib/library-config'
import { ConfirmActionModal } from '../confirm-action-modal'
import { GlassDialog, GlassDialogCloseButton } from '../glass-dialog'
import { GlassSelect, type GlassSelectOption } from '../glass-select'
import { SectionHelp } from '../section-help'

interface LibraryEditorModalProps {
  error: string | null
  isOpen: boolean
  isSubmitting: boolean
  library: Library | null
  onClose: () => void
  onUpdate: (libraryId: number, input: UpdateLibraryInput) => Promise<unknown>
}

const libraryBadge = (library: Library | null) =>
  library?.name.trim().charAt(0).toUpperCase() || 'L'

export const LibraryEditorModal = ({
  error,
  isOpen,
  isSubmitting,
  library,
  onClose,
  onUpdate,
}: LibraryEditorModalProps) => {
  const { l } = useI18n()
  const [visibleLibrary, setVisibleLibrary] = useState<Library | null>(library)
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [metadataLanguage, setMetadataLanguage] = useState('zh-CN')
  const [pendingMetadataLanguageUpdate, setPendingMetadataLanguageUpdate] =
    useState<UpdateLibraryInput | null>(null)

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const draft = buildLibraryEditorDraft(library)
    setVisibleLibrary(library)
    setName(draft.name)
    setDescription(draft.description)
    setMetadataLanguage(draft.metadataLanguage)
    setPendingMetadataLanguageUpdate(null)
  }, [isOpen, library])

  const submitUpdate = async (input: UpdateLibraryInput) => {
    if (!library) {
      return
    }

    try {
      await onUpdate(library.id, input)
      setPendingMetadataLanguageUpdate(null)
      onClose()
    } catch {
      return
    }
  }

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    if (!library) {
      return
    }

    const draft = { name, description, metadataLanguage }
    const input = buildLibraryUpdateInput(draft)

    if (hasLibraryMetadataLanguageChanged(library, draft)) {
      setPendingMetadataLanguageUpdate(input)
      return
    }

    void submitUpdate(input)
  }

  const activeLibrary = library ?? visibleLibrary

  if (!activeLibrary) {
    return null
  }

  const normalizedName = name.trim()
  const hasChanges = hasLibraryConfigChanges(activeLibrary, {
    name,
    description,
    metadataLanguage,
  })
  const metadataLanguageOptions: GlassSelectOption[] = [
    { value: 'zh-CN', label: `${l('Chinese')} (zh-CN)` },
    { value: 'en-US', label: `${l('English')} (en-US)` },
  ]

  return (
    <>
      <GlassDialog
        ariaLabel={l('Edit Library')}
        className="library-editor-modal"
        closeLabel={l('Close library editor dialog')}
        isCloseDisabled={isSubmitting || pendingMetadataLanguageUpdate !== null}
        isOpen={isOpen && library !== null}
        onClose={onClose}
        surfaceClassName="library-editor-modal__surface scrollbar-thin"
      >
        <div className="library-editor-modal__header">
          <div className="library-editor-modal__identity">
            <div className="library-editor-modal__badge">{libraryBadge(activeLibrary)}</div>
            <div>
              <p className="eyebrow">{l('Library Management')}</p>
              <h3>{l('Edit Library')}</h3>
            </div>
          </div>

          <GlassDialogCloseButton
            ariaLabel={l('Close library editor dialog')}
            className="library-editor-modal__close"
            disabled={isSubmitting || pendingMetadataLanguageUpdate !== null}
            onClick={onClose}
          />
        </div>

        <form className="stack" onSubmit={handleSubmit}>
          <label className="field">
            <span>{l('Library Name')}</span>
            <input
              onChange={(event) => setName(event.target.value)}
              placeholder={l('Library Name')}
              required
              type="text"
              value={name}
            />
          </label>

          <label className="field">
            <span>{l('Description')}</span>
            <textarea
              className="library-description-input"
              maxLength={LIBRARY_DESCRIPTION_MAX_LENGTH}
              onChange={(event) => setDescription(event.target.value)}
              placeholder={l('What is this library for?')}
              rows={4}
              value={description}
            />
          </label>

          <div className="field">
            <span>{l('Metadata Language')}</span>
            <GlassSelect
              ariaLabel={l('Metadata Language')}
              onChange={setMetadataLanguage}
              options={metadataLanguageOptions}
              value={metadataLanguage}
            />
          </div>

          <div className="field">
            <div className="field__label">
              <span className="field__label-copy">{l('Root Path')}</span>
              <SectionHelp
                detail={l(
                  'This shows the in-container path. The host media directory configured in Docker Compose is mounted at /media, so the /media/... value shown here is the real scan path used by the app.',
                )}
                title={l('Root path help')}
              />
            </div>
            <code className="library-editor-modal__path">{activeLibrary.root_path}</code>
          </div>

          {error ? <p className="callout callout--danger">{error}</p> : null}

          <div className="library-editor-modal__footer">
            <button className="button" onClick={onClose} type="button">
              {l('Cancel')}
            </button>
            <button
              className="button button--primary"
              disabled={isSubmitting || normalizedName.length === 0 || !hasChanges}
              type="submit"
            >
              {isSubmitting ? l('Saving…') : l('Save Changes')}
            </button>
          </div>
        </form>
      </GlassDialog>

      <ConfirmActionModal
        confirmLabel={l('Save and scan')}
        confirmTone="primary"
        description={l(
          'Changing the metadata language from {{from}} to {{to}} will trigger a full metadata scan for this library. Continue?',
          {
            from: activeLibrary.metadata_language,
            to: metadataLanguage,
          },
        )}
        error={error}
        isOpen={pendingMetadataLanguageUpdate !== null}
        isSubmitting={isSubmitting}
        onClose={() => setPendingMetadataLanguageUpdate(null)}
        onConfirm={() => {
          if (pendingMetadataLanguageUpdate) {
            void submitUpdate(pendingMetadataLanguageUpdate)
          }
        }}
        title={l('Change metadata language?')}
      />
    </>
  )
}
