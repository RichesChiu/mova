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

  it('routes skipped local items into other for review', () => {
    expect(
      getLibraryMediaSection({
        media_type: 'movie',
        metadata_status: 'skipped',
      }),
    ).toBe('other')
    expect(
      getLibraryMediaSection({
        media_type: 'series',
        metadata_status: 'skipped',
      }),
    ).toBe('other')
  })

  it('keeps review-status items in detected sections when enrichment is visible', () => {
    expect(
      getLibraryMediaSection({
        media_type: 'movie',
        metadata_status: 'unmatched',
        overview: 'A remote overview from TMDB.',
        poster_path: '/api/media-items/915/poster?v=1778470497',
      }),
    ).toBe('movies')
    expect(
      getLibraryMediaSection({
        media_type: 'series',
        metadata_status: 'failed',
        backdrop_path: '/api/media-items/916/backdrop?v=1778470497',
      }),
    ).toBe('series')
    expect(
      getLibraryMediaSection({
        media_type: 'movie',
        metadata_provider_item_id: 123,
        metadata_status: 'unmatched',
      }),
    ).toBe('movies')
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
    expect(getLibraryScanSection({ media_type: 'unknown' })).toBeNull()
  })

  it('keeps failed scan runtime items in other for review', () => {
    expect(getLibraryScanSection({ media_type: 'movie', metadata_status: 'skipped' })).toBe(
      'other',
    )
    expect(getLibraryScanSection({ media_type: 'movie', metadata_status: 'unmatched' })).toBe(
      'other',
    )
    expect(getLibraryScanSection({ media_type: 'series', metadata_status: 'failed' })).toBe(
      'other',
    )
  })

  it('keeps enriched scan runtime items in detected sections', () => {
    expect(
      getLibraryScanSection({
        media_type: 'movie',
        metadata_status: 'unmatched',
        poster_path: '/api/media-items/915/poster?v=1778470497',
      }),
    ).toBe('movies')
  })
})
