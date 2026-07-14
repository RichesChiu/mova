import type { Library, ServerMediaDirectoryNode, UpdateLibraryInput } from '../api/types'

export const LIBRARY_DESCRIPTION_MAX_LENGTH = 100

export interface LibraryEditorDraft {
  description: string
  metadataLanguage: string
  name: string
}

export const buildLibraryEditorDraft = (library: Library | null): LibraryEditorDraft => ({
  description: library?.description ?? '',
  metadataLanguage: library?.metadata_language ?? 'zh-CN',
  name: library?.name ?? '',
})

export const buildLibraryUpdateInput = (draft: LibraryEditorDraft): UpdateLibraryInput => ({
  name: draft.name.trim(),
  description: draft.description.trim() || null,
  metadata_language: draft.metadataLanguage,
})

export const hasLibraryConfigChanges = (library: Library | null, draft: LibraryEditorDraft) => {
  if (!library) {
    return false
  }

  const normalizedInput = buildLibraryUpdateInput(draft)

  return (
    normalizedInput.name !== library.name ||
    normalizedInput.description !== (library.description ?? null) ||
    normalizedInput.metadata_language !== library.metadata_language
  )
}

export const hasLibraryMetadataLanguageChanged = (
  library: Library | null,
  draft: LibraryEditorDraft,
) => Boolean(library && draft.metadataLanguage !== library.metadata_language)

const mediaTreeContainsPath = (node: ServerMediaDirectoryNode, path: string): boolean => {
  if (node.path === path) {
    return true
  }

  return node.children.some((child) => mediaTreeContainsPath(child, path))
}

export const retainValidLibraryRootPath = (
  tree: ServerMediaDirectoryNode | null,
  selectedPath: string,
) => {
  if (!tree || !selectedPath || !mediaTreeContainsPath(tree, selectedPath)) {
    return ''
  }

  return selectedPath
}
