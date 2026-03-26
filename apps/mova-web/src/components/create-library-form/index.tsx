import { useQuery } from '@tanstack/react-query'
import { type FormEvent, useEffect, useState } from 'react'
import { listServerRootPaths } from '../../api/client'
import type { CreateLibraryInput, LibraryType } from '../../api/types'
import { GlassSelect, type GlassSelectOption } from '../glass-select'

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
  { value: 'zh-CN', label: '中文 (zh-CN)' },
  { value: 'en-US', label: 'English (en-US)' },
]

const NO_MAPPED_ROOT_PATH_VALUE = '__no_mapped_root_path__'

export const CreateLibraryForm = ({ error, isSubmitting, onSubmit }: CreateLibraryFormProps) => {
  const [name, setName] = useState('Media')
  const [description, setDescription] = useState('')
  const [libraryType, setLibraryType] = useState<LibraryType>('mixed')
  const [metadataLanguage, setMetadataLanguage] = useState('zh-CN')
  const [rootPath, setRootPath] = useState('')
  const [isEnabled, setIsEnabled] = useState(true)
  const rootPathQuery = useQuery({
    queryKey: ['server-root-paths'],
    queryFn: listServerRootPaths,
  })

  useEffect(() => {
    const options = rootPathQuery.data ?? []
    if (options.length === 0) {
      if (rootPath.length > 0) {
        setRootPath('')
      }
      return
    }

    const hasCurrentPath = options.some((option) => option.path === rootPath)
    if (!hasCurrentPath) {
      setRootPath(options[0].path)
    }
  }, [rootPathQuery.data, rootPath])

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

  const rootPathOptions = rootPathQuery.data ?? []
  const hasMappedRootPaths = rootPathOptions.length > 0
  const selectedRootPathValue = hasMappedRootPaths ? rootPath : NO_MAPPED_ROOT_PATH_VALUE
  const libraryTypeSelectOptions: GlassSelectOption[] = libraryTypeOptions.map((option) => ({
    value: option.value,
    label: option.label,
  }))
  const rootPathSelectOptions: GlassSelectOption[] = hasMappedRootPaths
    ? rootPathOptions.map((option) => ({
        value: option.path,
        label: option.path,
      }))
    : [
        {
          value: NO_MAPPED_ROOT_PATH_VALUE,
          label: 'No mapped path found',
          disabled: true,
        },
      ]
  const rootPathHint =
    rootPathOptions.length > 0 ? (
      <p className="root-path-picker__hint">
        已从服务器配置中发现 {rootPathOptions.length} 个可选路径。
      </p>
    ) : (
      <p className="root-path-picker__hint">
        暂未发现可用路径，请确认 `.env` 已配置容器路径 `MOVA_LIBRARY_ROOTS=/media/...`，且 docker compose 已挂载对应目录。
      </p>
    )

  const handleRootPathSelectChange = (value: string) => {
    if (value === NO_MAPPED_ROOT_PATH_VALUE) {
      return
    }

    setRootPath(value)
  }

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
        <div className="root-path-picker">
          <GlassSelect
            ariaLabel="Root path"
            disabled={!hasMappedRootPaths || rootPathQuery.isLoading}
            onChange={handleRootPathSelectChange}
            options={rootPathSelectOptions}
            value={selectedRootPathValue}
          />

          {rootPathHint}
        </div>

        {rootPathQuery.isLoading ? (
          <p className="root-path-picker__hint">正在读取服务器挂载目录…</p>
        ) : null}
        {rootPathQuery.isError ? (
          <p className="root-path-picker__hint">
            {rootPathQuery.error instanceof Error
              ? `读取目录失败：${rootPathQuery.error.message}`
              : '读取目录失败，请检查服务端挂载配置'}
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
        disabled={isSubmitting || !hasMappedRootPaths || rootPath.trim().length === 0}
        type="submit"
      >
        {isSubmitting ? 'Creating…' : 'Create Library'}
      </button>
    </form>
  )
}
