import { type FormEvent, useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import type { Library, UpdateLibraryInput } from '../../api/types'
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
  library?.name.trim().charAt(0).toUpperCase() ||
  library?.library_type?.charAt(0).toUpperCase() ||
  'L'

const metadataLanguageOptions: GlassSelectOption[] = [
  { value: 'zh-CN', label: 'Chinese (zh-CN)' },
  { value: 'en-US', label: 'English (en-US)' },
]

const LIBRARY_TYPE_HELP = (
  <span className="section-help__tooltip-list">
    <span className="section-help__tooltip-item">
      <span className="section-help__tooltip-label">Mixed</span>
      <span>Automatically sorts movies and series by filename. Best for mixed folders.</span>
    </span>
    <span className="section-help__tooltip-item">
      <span className="section-help__tooltip-label">Movie</span>
      <span>Organizes only movies. Best for movie-only folders.</span>
    </span>
    <span className="section-help__tooltip-item">
      <span className="section-help__tooltip-label">Series</span>
      <span>Organizes only series. Best for dedicated TV show folders.</span>
    </span>
  </span>
)

const ROOT_PATH_HELP =
  'This shows the in-container path. The host MOVA_MEDIA_ROOT is mounted into the container as /media, so the /media/... value shown here is the real scan path used by the app.'

export const LibraryEditorModal = ({
  error,
  isOpen,
  isSubmitting,
  library,
  onClose,
  onUpdate,
}: LibraryEditorModalProps) => {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [metadataLanguage, setMetadataLanguage] = useState('zh-CN')
  const [isEnabled, setIsEnabled] = useState(true)

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const draft = buildLibraryEditorDraft(library)
    setName(draft.name)
    setDescription(draft.description)
    setMetadataLanguage(draft.metadataLanguage)
    setIsEnabled(draft.isEnabled)
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
        isEnabled,
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
    isEnabled,
  })

  return createPortal(
    <div className="library-editor-modal">
      <button
        aria-label="Close library editor dialog"
        className="library-editor-modal__backdrop"
        onClick={onClose}
        type="button"
      />

      <div aria-modal="true" className="library-editor-modal__surface" role="dialog">
        <div className="library-editor-modal__header">
          <div className="library-editor-modal__identity">
            <div className="library-editor-modal__badge">{libraryBadge(library)}</div>
            <div>
              <p className="eyebrow">Library Management</p>
              <h3>Edit Library</h3>
            </div>
          </div>

          <button
            aria-label="Close library editor dialog"
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
            <span>Library Name</span>
            <input
              onChange={(event) => setName(event.target.value)}
              placeholder="Media"
              type="text"
              value={name}
            />
          </label>

          <div className="library-editor-modal__facts">
            <article className="library-editor-modal__fact">
              <div className="field__label">
                <span className="field__label-copy">Library Type</span>
                <SectionHelp detail={LIBRARY_TYPE_HELP} title="Library type help" />
              </div>
              <strong>{library.library_type}</strong>
            </article>
            <article className="library-editor-modal__fact">
              <span>Metadata Language</span>
              <strong>{metadataLanguage}</strong>
            </article>
            <article className="library-editor-modal__fact">
              <span>Library Status</span>
              <strong>{isEnabled ? 'Enabled' : 'Disabled'}</strong>
            </article>
          </div>

          <label className="field">
            <span>Description</span>
            <textarea
              onChange={(event) => setDescription(event.target.value)}
              placeholder="What is this library for?"
              rows={4}
              value={description}
            />
          </label>

          <div className="field">
            <span>Metadata Language</span>
            <GlassSelect
              ariaLabel="Library metadata language"
              onChange={setMetadataLanguage}
              options={metadataLanguageOptions}
              value={metadataLanguage}
            />
          </div>

          <label className="toggle">
            <input
              checked={isEnabled}
              onChange={(event) => setIsEnabled(event.target.checked)}
              type="checkbox"
            />
            <span>Enable library</span>
          </label>

          <div className="field">
            <div className="field__label">
              <span className="field__label-copy">Root Path</span>
              <SectionHelp detail={ROOT_PATH_HELP} title="Root path help" />
            </div>
            <code className="library-editor-modal__path">{library.root_path}</code>
          </div>

          {error ? <p className="callout callout--danger">{error}</p> : null}

          <div className="library-editor-modal__footer">
            <button className="button" onClick={onClose} type="button">
              Cancel
            </button>
            <button
              className="button button--primary"
              disabled={isSubmitting || normalizedName.length === 0 || !hasChanges}
              type="submit"
            >
              {isSubmitting ? 'Saving…' : 'Save Changes'}
            </button>
          </div>
        </form>
      </div>
    </div>,
    document.body,
  )
}
