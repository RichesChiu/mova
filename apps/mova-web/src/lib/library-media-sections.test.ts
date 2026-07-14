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

  it('keeps pending local items in their inferred sections', () => {
    expect(
      getLibraryMediaSection({
        media_type: 'movie',
        metadata_status: 'pending',
      }),
    ).toBe('movies')
    expect(
      getLibraryMediaSection({
        media_type: 'series',
        metadata_status: 'pending',
      }),
    ).toBe('series')
  })

  it('keeps final review-status items in inferred sections when remote type confirms them', () => {
    expect(
      getLibraryMediaSection({
        media_type: 'movie',
        metadata_status: 'unmatched',
        remote_media_type: 'movie',
      }),
    ).toBe('movies')
    expect(
      getLibraryMediaSection({
        media_type: 'series',
        metadata_status: 'failed',
        remote_media_type: 'series',
      }),
    ).toBe('series')
    expect(
      getLibraryMediaSection({
        media_type: 'episode',
        metadata_status: 'unmatched',
        remote_media_type: 'series',
      }),
    ).toBe('series')
  })

  it('routes completed items with unknown or conflicting remote types into other', () => {
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
        remote_media_type: 'movie',
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

  it('keeps in-progress scan items in their locally inferred sections', () => {
    expect(
      getLibraryScanSection({
        media_type: 'movie',
        metadata_status: 'unmatched',
        stage: 'discovered',
      }),
    ).toBe('movies')
    expect(
      getLibraryScanSection({
        media_type: 'series',
        metadata_status: 'failed',
        stage: 'artwork',
      }),
    ).toBe('series')
    expect(
      getLibraryScanSection({
        media_type: 'series',
        metadata_status: 'unmatched',
        stage: 'completed',
      }),
    ).toBe('other')
  })

  it('keeps completed scan items in inferred sections when remote type confirms them', () => {
    expect(
      getLibraryScanSection({
        media_type: 'movie',
        metadata_status: 'unmatched',
        remote_media_type: 'movie',
        stage: 'completed',
      }),
    ).toBe('movies')
  })

  it('does not treat a final status without remote type confirmation as typed', () => {
    expect(
      getLibraryMediaSection({
        media_type: 'series',
        metadata_status: 'failed',
      }),
    ).toBe('other')
    expect(
      getLibraryScanSection({
        media_type: 'series',
        metadata_status: 'unmatched',
        stage: 'completed',
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
        remote_media_type: null,
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
        remote_media_type: 'series',
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
          remote_media_type: 'series',
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
        remote_media_type: 'series',
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
          remote_media_type: null,
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
        remote_media_type: null,
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
        remote_media_type: null,
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
          remote_media_type: 'series',
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
