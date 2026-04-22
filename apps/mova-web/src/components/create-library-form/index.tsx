import { useQuery } from '@tanstack/react-query'
import { type FormEvent, useEffect, useState } from 'react'
import { getServerMediaTree } from '../../api/client'
import type { CreateLibraryInput, ServerMediaDirectoryNode } from '../../api/types'
import { useI18n } from '../../i18n'
import { GlassSelect, type GlassSelectOption } from '../glass-select'
import { MediaDirectoryTree } from '../media-directory-tree'
import { SectionHelp } from '../section-help'

interface CreateLibraryFormProps {
  error: string | null
  isSubmitting: boolean
  onSubmit: (input: CreateLibraryInput) => Promise<unknown>
}

const treeContainsPath = (node: ServerMediaDirectoryNode, path: string): boolean => {
  if (node.path === path) {
    return true
  }

  return node.children.some((child) => treeContainsPath(child, path))
}

export const CreateLibraryForm = ({ error, isSubmitting, onSubmit }: CreateLibraryFormProps) => {
  const { l } = useI18n()
  const [name, setName] = useState('Media')
  const [description, setDescription] = useState('')
  const [metadataLanguage, setMetadataLanguage] = useState('zh-CN')
  const [rootPath, setRootPath] = useState('')
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
        metadata_language: metadataLanguage,
        root_path: normalizedRootPath,
      })
    } catch {
      // Mutation state already exposes the error message in the form.
    }
  }

  const mediaTree = mediaTreeQuery.data ?? null
  const metadataLanguageOptions: GlassSelectOption[] = [
    { value: 'zh-CN', label: `${l('Chinese')} (zh-CN)` },
    { value: 'en-US', label: `${l('English')} (en-US)` },
  ]
  return (
    <form className="stack" onSubmit={handleSubmit}>
      <label className="field">
        <span>{l('Name')}</span>
        <input
          onChange={(event) => setName(event.target.value)}
          placeholder={l('Library Name')}
          type="text"
          value={name}
        />
      </label>

      <label className="field">
        <span>{l('Description')}</span>
        <textarea
          onChange={(event) => setDescription(event.target.value)}
          placeholder={l('What is this library for?')}
          rows={3}
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
              'This picker shows in-container paths. The host MOVA_MEDIA_ROOT is mounted into the container as /media, so every /media/... path here maps to the actual library root available to the app.',
            )}
            title={l('Root path help')}
          />
        </div>

        {mediaTree ? (
          <div className="root-path-picker">
            <div className="media-tree">
              <div className="media-tree__selected">
                <span className="media-tree__selected-label">{l('Selected')}</span>
                <code>{rootPath}</code>
              </div>

              <MediaDirectoryTree onSelect={setRootPath} selectedPath={rootPath} tree={mediaTree} />
            </div>
          </div>
        ) : null}

        {mediaTreeQuery.isLoading ? (
          <p className="root-path-picker__hint">{l('Reading the in-container `/media` tree…')}</p>
        ) : null}
        {mediaTreeQuery.isError ? (
          <p className="root-path-picker__hint">
            {mediaTreeQuery.error instanceof Error
              ? `Failed to read directories: ${mediaTreeQuery.error.message}`
              : l('Failed to read directories. Check the Docker volume mapping.')}
          </p>
        ) : null}
        {!mediaTreeQuery.isLoading && !mediaTreeQuery.isError && !mediaTree ? (
          <p className="root-path-picker__hint">
            {l(
              'No in-container `/media` directory was detected yet. Make sure `.env` sets `MOVA_MEDIA_ROOT`, then restart Docker Compose.',
            )}
          </p>
        ) : null}
      </div>

      {error ? <p className="callout callout--danger">{error}</p> : null}

      <button
        className="button button--primary"
        disabled={isSubmitting || !mediaTree || rootPath.trim().length === 0}
        type="submit"
      >
        {isSubmitting ? l('Creating…') : l('Create Library')}
      </button>
    </form>
  )
}
