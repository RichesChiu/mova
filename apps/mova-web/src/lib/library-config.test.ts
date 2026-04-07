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
  description: '家庭电影库',
  library_type: 'movie',
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
      description: '家庭电影库',
      metadataLanguage: 'zh-CN',
      isEnabled: true,
    })
  })

  it('normalizes description and booleans into the update payload', () => {
    expect(
      buildLibraryUpdateInput({
        name: '  Cinema  ',
        description: '   ',
        metadataLanguage: 'en-US',
        isEnabled: false,
      }),
    ).toEqual({
      name: 'Cinema',
      description: null,
      metadata_language: 'en-US',
      is_enabled: false,
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
