import { useQuery } from '@tanstack/react-query'
import { type FormEvent, useEffect, useState } from 'react'
import { getServerMediaTree } from '../../api/client'
import type { CreateLibraryInput, LibraryType, ServerMediaDirectoryNode } from '../../api/types'
import { GlassSelect, type GlassSelectOption } from '../glass-select'
import { MediaDirectoryTree } from '../media-directory-tree'
import { SectionHelp } from '../section-help'

interface CreateLibraryFormProps {
  error: string | null
  isSubmitting: boolean
  onSubmit: (input: CreateLibraryInput) => Promise<unknown>
}

const libraryTypeOptions: Array<{ value: LibraryType; label: string }> = [
  { value: 'mixed', label: 'Mixed' },
  { value: 'movie', label: 'Movie' },
  { value: 'series', label: 'Series' },
]

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
  'This picker shows in-container paths. The host MOVA_MEDIA_ROOT is mounted into the container as /media, so every /media/... path here maps to the actual library root available to the app.'

const treeContainsPath = (node: ServerMediaDirectoryNode, path: string): boolean => {
  if (node.path === path) {
    return true
  }

  return node.children.some((child) => treeContainsPath(child, path))
}

export const CreateLibraryForm = ({ error, isSubmitting, onSubmit }: CreateLibraryFormProps) => {
  const [name, setName] = useState('Media')
  const [description, setDescription] = useState('')
  const [libraryType, setLibraryType] = useState<LibraryType>('mixed')
  const [metadataLanguage, setMetadataLanguage] = useState('zh-CN')
  const [rootPath, setRootPath] = useState('')
  const [isEnabled, setIsEnabled] = useState(true)
  const mediaTreeQuery = useQuery({
    queryKey: ['server-media-tree'],
    queryFn: getServerMediaTree,
  })

  useEffect(() => {
    const mediaTree = mediaTreeQuery.data
    if (!mediaTree) {
      if (rootPath.length > 0) {
        setRootPath('')
      }
      return
    }

    if (!treeContainsPath(mediaTree, rootPath)) {
      setRootPath(mediaTree.path)
    }
  }, [mediaTreeQuery.data, rootPath])

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    const normalizedRootPath = rootPath.trim()
    if (!normalizedRootPath) {
      return
    }

    try {
      const normalizedDescription = description.trim()
      await onSubmit({
        name,
        description: normalizedDescription || undefined,
        library_type: libraryType,
        metadata_language: metadataLanguage,
        root_path: normalizedRootPath,
        is_enabled: isEnabled,
      })
    } catch {
      // Mutation state already exposes the error message in the form.
    }
  }

  const mediaTree = mediaTreeQuery.data ?? null
  const libraryTypeSelectOptions: GlassSelectOption[] = libraryTypeOptions.map((option) => ({
    value: option.value,
    label: option.label,
  }))

  return (
    <form className="stack" onSubmit={handleSubmit}>
      <label className="field">
        <span>Name</span>
        <input
          onChange={(event) => setName(event.target.value)}
          placeholder="Media"
          type="text"
          value={name}
        />
      </label>

      <label className="field">
        <span>Description</span>
        <textarea
          onChange={(event) => setDescription(event.target.value)}
          placeholder="What is this library for?"
          rows={3}
          value={description}
        />
      </label>

      <div className="field">
        <div className="field__label">
          <span className="field__label-copy">Library Type</span>
          <SectionHelp detail={LIBRARY_TYPE_HELP} title="Library type help" />
        </div>
        <GlassSelect
          ariaLabel="Library type"
          onChange={(value) => setLibraryType(value as LibraryType)}
          options={libraryTypeSelectOptions}
          value={libraryType}
        />
      </div>

      <div className="field">
        <span>Metadata Language</span>
        <GlassSelect
          ariaLabel="Metadata language"
          onChange={setMetadataLanguage}
          options={metadataLanguageOptions}
          value={metadataLanguage}
        />
      </div>

      <div className="field">
        <div className="field__label">
          <span className="field__label-copy">Root Path</span>
          <SectionHelp detail={ROOT_PATH_HELP} title="Root path help" />
        </div>

        {mediaTree ? (
          <div className="root-path-picker">
            <div className="media-tree">
              <div className="media-tree__selected">
                <span className="media-tree__selected-label">Selected</span>
                <code>{rootPath}</code>
              </div>

              <MediaDirectoryTree onSelect={setRootPath} selectedPath={rootPath} tree={mediaTree} />
            </div>
          </div>
        ) : null}

        {mediaTreeQuery.isLoading ? (
          <p className="root-path-picker__hint">Reading the in-container `/media` tree…</p>
        ) : null}
        {mediaTreeQuery.isError ? (
          <p className="root-path-picker__hint">
            {mediaTreeQuery.error instanceof Error
              ? `Failed to read directories: ${mediaTreeQuery.error.message}`
              : 'Failed to read directories. Check the Docker volume mapping.'}
          </p>
        ) : null}
        {!mediaTreeQuery.isLoading && !mediaTreeQuery.isError && !mediaTree ? (
          <p className="root-path-picker__hint">
            No in-container `/media` directory was detected yet. Make sure `.env` sets
            `MOVA_MEDIA_ROOT`, then restart Docker Compose.
          </p>
        ) : null}
      </div>

      <label className="toggle">
        <input
          checked={isEnabled}
          onChange={(event) => setIsEnabled(event.target.checked)}
          type="checkbox"
        />
        <span>Enable library</span>
      </label>

      {error ? <p className="callout callout--danger">{error}</p> : null}

      <button
        className="button button--primary"
        disabled={isSubmitting || !mediaTree || rootPath.trim().length === 0}
        type="submit"
      >
        {isSubmitting ? 'Creating…' : 'Create Library'}
      </button>
    </form>
  )
}
