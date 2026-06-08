import { describe, expect, it } from 'vitest'
import {
  filterLibraryMediaItemsForScanRuntime,
  getLibraryMediaSection,
  getLibraryScanSection,
} from './library-media-sections'

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

  it('keeps review-status items in detected sections when textual enrichment or binding is visible', () => {
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
        original_title: 'Liang Chen Mei Jin',
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
    expect(getLibraryScanSection({ media_type: 'movie', metadata_status: 'skipped' })).toBe('other')
    expect(getLibraryScanSection({ media_type: 'movie', metadata_status: 'unmatched' })).toBe(
      'other',
    )
    expect(getLibraryScanSection({ media_type: 'series', metadata_status: 'failed' })).toBe('other')
  })

  it('keeps enriched scan runtime items in detected sections', () => {
    expect(
      getLibraryScanSection({
        media_type: 'movie',
        metadata_status: 'unmatched',
        overview: 'A remote overview from TMDB.',
        poster_path: '/api/media-items/915/poster?v=1778470497',
      }),
    ).toBe('movies')
  })

  it('does not treat artwork alone as enrichment for review-status items', () => {
    expect(
      getLibraryMediaSection({
        media_type: 'series',
        metadata_status: 'failed',
        poster_path: '/api/media-items/916/poster?v=1778470497',
        backdrop_path: '/api/media-items/916/backdrop?v=1778470497',
      }),
    ).toBe('other')
    expect(
      getLibraryScanSection({
        media_type: 'series',
        metadata_status: 'unmatched',
        poster_path: 'https://image.tmdb.org/t/p/original/episode-still.jpg',
      }),
    ).toBe('other')
  })

  it('hides stale media cards while a matching scan card is promoted out of other', () => {
    const mediaItems = [
      {
        id: 1,
        media_type: 'series',
        metadata_provider_item_id: null,
        metadata_status: 'failed',
        title: '良陈美锦',
        source_title: '良陈美锦',
        original_title: null,
        year: null,
        overview: null,
        poster_path: null,
        backdrop_path: null,
      },
      {
        id: 2,
        media_type: 'series',
        metadata_provider_item_id: 839,
        metadata_status: 'matched',
        title: '月鳞绮纪',
        source_title: '月鳞绮纪',
        original_title: null,
        year: 2026,
        overview: 'A fantasy series.',
        poster_path: '/api/media-items/2/poster',
        backdrop_path: null,
      },
    ]

    expect(
      filterLibraryMediaItemsForScanRuntime(mediaItems, [
        {
          media_type: 'series',
          metadata_status: 'matched',
          title: '良陈美锦',
          year: null,
          overview: 'Remote overview is being fetched.',
          poster_path: null,
          backdrop_path: null,
        },
      ]).map((item) => item.id),
    ).toEqual([2])
  })

  it('does not let a review-only scan card hide an already detected section card', () => {
    const mediaItems = [
      {
        id: 1,
        media_type: 'series',
        metadata_provider_item_id: 456,
        metadata_status: 'matched',
        title: '良陈美锦',
        source_title: '良陈美锦',
        original_title: null,
        year: null,
        overview: 'A saved overview.',
        poster_path: '/api/media-items/1/poster',
        backdrop_path: null,
      },
    ]

    expect(
      filterLibraryMediaItemsForScanRuntime(mediaItems, [
        {
          media_type: 'series',
          metadata_status: 'failed',
          title: '良陈美锦',
          year: null,
          overview: null,
          poster_path: null,
          backdrop_path: null,
        },
      ]).map((item) => item.id),
    ).toEqual([1])
  })

  it('keeps same-title media items when the scan type or year is different', () => {
    const mediaItems = [
      {
        id: 1,
        media_type: 'movie',
        metadata_provider_item_id: null,
        metadata_status: 'failed',
        title: 'Traffic',
        source_title: 'Traffic',
        original_title: null,
        year: 2000,
        overview: null,
        poster_path: null,
        backdrop_path: null,
      },
      {
        id: 2,
        media_type: 'series',
        metadata_provider_item_id: null,
        metadata_status: 'failed',
        title: 'Traffic',
        source_title: 'Traffic',
        original_title: null,
        year: 2004,
        overview: null,
        poster_path: null,
        backdrop_path: null,
      },
    ]

    expect(
      filterLibraryMediaItemsForScanRuntime(mediaItems, [
        {
          media_type: 'series',
          metadata_status: 'matched',
          title: 'Traffic',
          year: 2026,
          overview: 'A different show.',
          poster_path: null,
          backdrop_path: null,
        },
      ]).map((item) => item.id),
    ).toEqual([1, 2])
  })
})
