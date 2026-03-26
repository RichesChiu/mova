import { useQuery } from '@tanstack/react-query'
import { type FormEvent, useEffect, useState } from 'react'
import { getServerMediaTree } from '../../api/client'
import type {
  CreateLibraryInput,
  LibraryType,
  ServerMediaDirectoryNode,
} from '../../api/types'
import { GlassSelect, type GlassSelectOption } from '../glass-select'

interface CreateLibraryFormProps {
  error: string | null
  isSubmitting: boolean
  onSubmit: (input: CreateLibraryInput) => Promise<unknown>
}

interface MediaTreeNodeProps {
  depth: number
  node: ServerMediaDirectoryNode
  onSelect: (path: string) => void
  selectedPath: string
}

const libraryTypeOptions: Array<{ value: LibraryType; label: string }> = [
  { value: 'mixed', label: 'Mixed' },
  { value: 'movie', label: 'Movie' },
  { value: 'series', label: 'Series' },
]

const metadataLanguageOptions: GlassSelectOption[] = [
  { value: 'zh-CN', label: '中文 (zh-CN)' },
  { value: 'en-US', label: 'English (en-US)' },
]

const treeContainsPath = (node: ServerMediaDirectoryNode, path: string): boolean => {
  if (node.path === path) {
    return true
  }

  return node.children.some((child) => treeContainsPath(child, path))
}

const MediaTreeNode = ({ depth, node, onSelect, selectedPath }: MediaTreeNodeProps) => {
  const isSelected = node.path === selectedPath
  const buttonClassName = isSelected
    ? 'media-tree__button media-tree__button--selected'
    : 'media-tree__button'

  return (
    <li className="media-tree__item">
      <button
        className={buttonClassName}
        onClick={() => onSelect(node.path)}
        style={{ paddingLeft: `${depth * 18 + 14}px` }}
        type="button"
      >
        <span className="media-tree__name">{node.name}</span>
        <span className="media-tree__meta">{node.path}</span>
      </button>

      {node.children.length > 0 ? (
        <ul className="media-tree__list">
          {node.children.map((child) => (
            <MediaTreeNode
              depth={depth + 1}
              key={child.path}
              node={child}
              onSelect={onSelect}
              selectedPath={selectedPath}
            />
          ))}
        </ul>
      ) : null}
    </li>
  )
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
        <span>Library Type</span>
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
        <span>Root Path</span>

        {mediaTree ? (
          <div className="root-path-picker">
            <p className="root-path-picker__hint">
              已读取容器内 `/media` 目录树。点击任意文件夹作为库源。
            </p>
            <div className="media-tree">
              <div className="media-tree__selected">
                <span className="media-tree__selected-label">Selected</span>
                <code>{rootPath}</code>
              </div>

              <ul className="media-tree__list media-tree__list--root">
                <MediaTreeNode
                  depth={0}
                  node={mediaTree}
                  onSelect={setRootPath}
                  selectedPath={rootPath}
                />
              </ul>
            </div>
          </div>
        ) : null}

        {mediaTreeQuery.isLoading ? (
          <p className="root-path-picker__hint">正在读取容器内 `/media` 目录树…</p>
        ) : null}
        {mediaTreeQuery.isError ? (
          <p className="root-path-picker__hint">
            {mediaTreeQuery.error instanceof Error
              ? `读取目录失败：${mediaTreeQuery.error.message}`
              : '读取目录失败，请检查 docker 挂载配置'}
          </p>
        ) : null}
        {!mediaTreeQuery.isLoading && !mediaTreeQuery.isError && !mediaTree ? (
          <p className="root-path-picker__hint">
            暂未检测到容器内 `/media` 目录，请确认 `.env` 已配置 `MOVA_MEDIA_ROOT`，并且已重新启动 docker compose。
          </p>
        ) : null}
      </div>

      <label className="toggle">
        <input
          checked={isEnabled}
          onChange={(event) => setIsEnabled(event.target.checked)}
          type="checkbox"
        />
        <span>Enable watcher and initial scan immediately</span>
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
