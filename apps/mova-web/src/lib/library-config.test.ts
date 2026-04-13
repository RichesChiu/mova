import { describe, expect, it } from 'vitest'
import type { Library } from '../api/types'
import {
  buildLibraryEditorDraft,
  buildLibraryUpdateInput,
  hasLibraryConfigChanges,
} from './library-config'

const library: Library = {
  id: 7,
  name: 'Movies',
  description: 'Family movie library',
  metadata_language: 'zh-CN',
  root_path: '/media/movies',
  is_enabled: true,
  created_at: '2026-04-07T00:00:00Z',
  updated_at: '2026-04-07T00:00:00Z',
}

describe('library config helpers', () => {
  it('builds a stable editor draft from a library record', () => {
    expect(buildLibraryEditorDraft(library)).toEqual({
      name: 'Movies',
      description: 'Family movie library',
      metadataLanguage: 'zh-CN',
    })
  })

  it('normalizes editable fields into the update payload', () => {
    expect(
      buildLibraryUpdateInput({
        name: '  Cinema  ',
        description: '   ',
        metadataLanguage: 'en-US',
      }),
    ).toEqual({
      name: 'Cinema',
      description: null,
      metadata_language: 'en-US',
    })
  })

  it('detects whether the draft changed any persisted field', () => {
    expect(hasLibraryConfigChanges(library, buildLibraryEditorDraft(library))).toBe(false)
    expect(
      hasLibraryConfigChanges(library, {
        ...buildLibraryEditorDraft(library),
        description: '',
        metadataLanguage: 'en-US',
      }),
    ).toBe(true)
  })
})
