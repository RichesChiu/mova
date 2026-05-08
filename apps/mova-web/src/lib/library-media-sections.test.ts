import { describe, expect, it } from 'vitest'
import { getLibraryMediaSection, getLibraryScanSection } from './library-media-sections'

describe('library-media-sections', () => {
  it('routes local media items by their detected media type', () => {
    expect(
      getLibraryMediaSection({
        media_type: 'movie',
        metadata_status: 'matched',
      }),
    ).toBe('movies')
    expect(
      getLibraryMediaSection({
        media_type: 'series',
        metadata_status: 'matched',
      }),
    ).toBe('series')
    expect(
      getLibraryMediaSection({
        media_type: 'episode',
        metadata_status: 'matched',
      }),
    ).toBe('series')
  })

  it('keeps skipped local items in their detected section', () => {
    expect(
      getLibraryMediaSection({
        media_type: 'movie',
        metadata_status: 'skipped',
      }),
    ).toBe('movies')
    expect(
      getLibraryMediaSection({
        media_type: 'series',
        metadata_status: 'skipped',
      }),
    ).toBe('series')
  })

  it('routes unmatched movie-like items into other for review', () => {
    expect(
      getLibraryMediaSection({
        media_type: 'movie',
        metadata_status: 'unmatched',
      }),
    ).toBe('other')
    expect(
      getLibraryMediaSection({
        media_type: 'series',
        metadata_status: 'failed',
      }),
    ).toBe('other')
  })

  it('routes unknown media items into other', () => {
    expect(
      getLibraryMediaSection({
        media_type: 'unknown',
        metadata_status: 'matched',
      }),
    ).toBe('other')
  })

  it('routes scan runtime items by known media type only', () => {
    expect(getLibraryScanSection({ media_type: 'movie' })).toBe('movies')
    expect(getLibraryScanSection({ media_type: 'series' })).toBe('series')
    expect(getLibraryScanSection({ media_type: 'episode' })).toBe('series')
    expect(getLibraryScanSection({ media_type: 'unknown' })).toBe('other')
  })
})
