import { type FormEvent, useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import type { Library, UpdateLibraryInput } from '../../api/types'
import { useI18n } from '../../i18n'
import {
  buildLibraryEditorDraft,
  buildLibraryUpdateInput,
  hasLibraryConfigChanges,
} from '../../lib/library-config'
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
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [metadataLanguage, setMetadataLanguage] = useState('zh-CN')

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const draft = buildLibraryEditorDraft(library)
    setName(draft.name)
    setDescription(draft.description)
    setMetadataLanguage(draft.metadataLanguage)
  }, [isOpen, library])

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const previousOverflow = document.body.style.overflow
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose()
      }
    }

    document.body.style.overflow = 'hidden'
    window.addEventListener('keydown', handleKeyDown)

    return () => {
      document.body.style.overflow = previousOverflow
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [isOpen, onClose])

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    if (!library) {
      return
    }

    await onUpdate(
      library.id,
      buildLibraryUpdateInput({
        name,
        description,
        metadataLanguage,
      }),
    )
    onClose()
  }

  if (!isOpen || !library) {
    return null
  }

  const normalizedName = name.trim()
  const hasChanges = hasLibraryConfigChanges(library, {
    name,
    description,
    metadataLanguage,
  })
  const metadataLanguageOptions: GlassSelectOption[] = [
    { value: 'zh-CN', label: `${l('Chinese')} (zh-CN)` },
    { value: 'en-US', label: `${l('English')} (en-US)` },
  ]

  return createPortal(
    <div className="library-editor-modal">
      <button
        aria-label={l('Close library editor dialog')}
        className="library-editor-modal__backdrop glass-overlay-backdrop"
        onClick={onClose}
        type="button"
      />

      <div
        aria-modal="true"
        className="library-editor-modal__surface glass-modal-surface"
        role="dialog"
      >
        <div className="library-editor-modal__header">
          <div className="library-editor-modal__identity">
            <div className="library-editor-modal__badge">{libraryBadge(library)}</div>
            <div>
              <p className="eyebrow">{l('Library Management')}</p>
              <h3>{l('Edit Library')}</h3>
            </div>
          </div>

          <button
            aria-label={l('Close library editor dialog')}
            className="library-editor-modal__close"
            onClick={onClose}
            type="button"
          >
            <svg
              aria-hidden="true"
              className="library-editor-modal__close-icon"
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

        <form className="stack" onSubmit={handleSubmit}>
          <label className="field">
            <span>{l('Library Name')}</span>
            <input
              onChange={(event) => setName(event.target.value)}
              placeholder={l('Media')}
              type="text"
              value={name}
            />
          </label>

          <div className="library-editor-modal__facts">
            <article className="library-editor-modal__fact">
              <div className="field__label">
                <span className="field__label-copy">{l('Detection')}</span>
                <SectionHelp
                  detail={l(
                    'Libraries now detect movies and series automatically from the imported files. No manual type selection is required.',
                  )}
                  title={l('Automatic detection')}
                />
              </div>
              <strong>{l('Automatic')}</strong>
            </article>
            <article className="library-editor-modal__fact">
              <span>{l('Metadata Language')}</span>
              <strong>{metadataLanguage}</strong>
            </article>
          </div>

          <label className="field">
            <span>{l('Description')}</span>
            <textarea
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
                  'This shows the in-container path. The host MOVA_MEDIA_ROOT is mounted into the container as /media, so the /media/... value shown here is the real scan path used by the app.',
                )}
                title={l('Root path help')}
              />
            </div>
            <code className="library-editor-modal__path">{library.root_path}</code>
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
      </div>
    </div>,
    document.body,
  )
}
