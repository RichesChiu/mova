import type { Library, UpdateLibraryInput } from '../api/types'

export interface LibraryEditorDraft {
  description: string
  isEnabled: boolean
  metadataLanguage: string
  name: string
}

export const buildLibraryEditorDraft = (library: Library | null): LibraryEditorDraft => ({
  description: library?.description ?? '',
  isEnabled: library?.is_enabled ?? true,
  metadataLanguage: library?.metadata_language ?? 'zh-CN',
  name: library?.name ?? '',
})

export const buildLibraryUpdateInput = (draft: LibraryEditorDraft): UpdateLibraryInput => ({
  name: draft.name.trim(),
  description: draft.description.trim() || null,
  metadata_language: draft.metadataLanguage,
  is_enabled: draft.isEnabled,
})

export const hasLibraryConfigChanges = (library: Library | null, draft: LibraryEditorDraft) => {
  if (!library) {
    return false
  }

  const normalizedInput = buildLibraryUpdateInput(draft)

  return (
    normalizedInput.name !== library.name ||
    normalizedInput.description !== (library.description ?? null) ||
    normalizedInput.metadata_language !== library.metadata_language ||
    normalizedInput.is_enabled !== library.is_enabled
  )
}
