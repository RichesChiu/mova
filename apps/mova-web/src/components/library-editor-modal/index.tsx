import { type FormEvent, useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import type { Library, UpdateLibraryInput } from '../../api/types'

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

export const LibraryEditorModal = ({
  error,
  isOpen,
  isSubmitting,
  library,
  onClose,
  onUpdate,
}: LibraryEditorModalProps) => {
  const [name, setName] = useState('')

  useEffect(() => {
    if (!isOpen) {
      return
    }

    setName(library?.name ?? '')
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

    await onUpdate(library.id, {
      name: name.trim(),
    })
    onClose()
  }

  if (!isOpen || !library) {
    return null
  }

  const normalizedName = name.trim()

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
              <p className="muted">当前先开放库名编辑，其他配置仍保持只读展示。</p>
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
              <span>Library Type</span>
              <strong>{library.library_type}</strong>
            </article>
            <article className="library-editor-modal__fact">
              <span>Metadata Language</span>
              <strong>{library.metadata_language}</strong>
            </article>
            <article className="library-editor-modal__fact">
              <span>Watcher Status</span>
              <strong>{library.is_enabled ? 'Enabled' : 'Disabled'}</strong>
            </article>
          </div>

          <div className="field">
            <span>Root Path</span>
            <code className="library-editor-modal__path">{library.root_path}</code>
          </div>

          <div className="field">
            <span>Description</span>
            <p className="library-editor-modal__description">
              {library.description ?? 'No description'}
            </p>
          </div>

          {error ? <p className="callout callout--danger">{error}</p> : null}

          <div className="library-editor-modal__footer">
            <button className="button" onClick={onClose} type="button">
              Cancel
            </button>
            <button
              className="button button--primary"
              disabled={isSubmitting || normalizedName.length === 0}
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
