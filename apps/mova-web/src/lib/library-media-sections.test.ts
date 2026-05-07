import { describe, expect, it } from 'vitest'
import {
  getLibraryMediaSection,
  getLibraryScanSection,
  hasMatchedMetadata,
} from './library-media-sections'

describe('library-media-sections', () => {
  it('keeps matched movies and series in their typed sections', () => {
    expect(
      getLibraryMediaSection({
        media_type: 'movie',
        metadata_provider_item_id: 101,
      }),
    ).toBe('movies')
    expect(
      getLibraryMediaSection({
        media_type: 'series',
        metadata_provider_item_id: 202,
      }),
    ).toBe('series')
  })

  it('routes unmatched or unknown media items into other', () => {
    expect(
      getLibraryMediaSection({
        media_type: 'movie',
        metadata_provider_item_id: null,
      }),
    ).toBe('other')
    expect(
      getLibraryMediaSection({
        media_type: 'series',
        metadata_provider_item_id: null,
      }),
    ).toBe('other')
    expect(
      getLibraryMediaSection({
        media_type: 'episode',
        metadata_provider_item_id: 303,
      }),
    ).toBe('other')
    expect(hasMatchedMetadata({ media_type: 'movie', metadata_provider_item_id: null })).toBe(false)
  })

  it('routes scan runtime items by known media type only', () => {
    expect(getLibraryScanSection({ media_type: 'movie' })).toBe('movies')
    expect(getLibraryScanSection({ media_type: 'series' })).toBe('series')
    expect(getLibraryScanSection({ media_type: 'episode' })).toBe('other')
  })
})
