import { describe, expect, it } from 'vitest'
import type { ScanJob } from '../../api/types'
import {
  formatFailedScanCopy,
  formatMediaItemScanStatusCopy,
  formatPendingScanPlaceholderCopy,
  formatScanItemCardProgressLabel,
  formatScanItemProgressCopy,
  formatScanJobStatusCopy,
  getMediaItemScanRuntimeItems,
  getScanItemCardProgressPercent,
  getScanJobProgressPercent,
  hasFailedLibraryScan,
  isLibraryScanActive,
  type LibraryScanRuntime,
  shouldShowScanPlaceholder,
} from './scan-runtime'

const buildScanJob = (overrides: Partial<ScanJob> = {}): ScanJob => ({
  id: 41,
  library_id: 7,
  status: 'running',
  phase: 'discovering',
  total_files: 20,
  scanned_files: 4,
  created_at: '2026-04-07T00:00:00Z',
  started_at: '2026-04-07T00:00:01Z',
  finished_at: null,
  error_message: null,
  ...overrides,
})

describe('scan runtime helpers', () => {
  it('keeps a library in syncing state before any item card is discovered', () => {
    const runtime: LibraryScanRuntime = {
      items: [],
      scanJob: buildScanJob(),
    }

    expect(isLibraryScanActive(null, runtime)).toBe(true)
    expect(shouldShowScanPlaceholder(null, runtime)).toBe(true)
    expect(formatScanJobStatusCopy(null, runtime)).toBe('Scanning files 4/20')
    expect(getScanJobProgressPercent(null, runtime)).toBe(12)
  })

  it('shows discovered file count when the scan total is unknown or stale', () => {
    const runtime: LibraryScanRuntime = {
      items: [],
      scanJob: buildScanJob({
        total_files: 0,
        scanned_files: 169,
      }),
    }

    expect(formatScanJobStatusCopy(null, runtime)).toBe('Discovered 169 files')
    expect(getScanJobProgressPercent(null, runtime)).toBe(12)

    expect(
      formatScanJobStatusCopy(null, {
        ...runtime,
        scanJob: buildScanJob({
          total_files: 12,
          scanned_files: 13,
        }),
      }),
    ).toBe('Discovered 13 files')

    expect(
      formatScanJobStatusCopy(null, {
        ...runtime,
        scanJob: buildScanJob({
          total_files: 791,
          scanned_files: 791,
        }),
      }),
    ).toBe('Discovered 791 files')
  })

  it('shows local analysis copy before metadata enrichment starts', () => {
    const runtime: LibraryScanRuntime = {
      items: [],
      scanJob: buildScanJob({
        phase: 'analyzing',
        total_files: 20,
        scanned_files: 20,
      }),
    }

    expect(formatScanJobStatusCopy(null, runtime)).toBe('Analyzing local media')
    expect(getScanJobProgressPercent(null, runtime)).toBe(46)
  })

  it('falls back to item-level progress copy when only a temporary scan card exists', () => {
    const runtime: LibraryScanRuntime = {
      scanJob: null,
      items: [
        {
          scan_job_id: 41,
          library_id: 7,
          item_key: '/media/movies/interstellar.mkv',
          media_type: 'movie',
          title: 'Interstellar',
          year: null,
          overview: null,
          poster_path: null,
          backdrop_path: null,
          metadata_status: null,
          remote_media_type: null,
          season_number: null,
          episode_number: null,
          item_index: 1,
          total_items: 3,
          stage: 'artwork',
          progress_percent: 68,
        },
      ],
    }

    expect(formatPendingScanPlaceholderCopy(null, runtime, 'Movies')).toBe(
      'Interstellar · Fetching artwork & overview',
    )
    expect(getScanJobProgressPercent(null, runtime)).toBe(68)
    expect(formatScanItemProgressCopy(runtime.items[0])).toBe('Fetching artwork & overview')
    expect(formatScanItemCardProgressLabel(runtime.items[0])).toBe('syncing')
    expect(getScanItemCardProgressPercent(runtime.items[0])).toBe(68)
  })

  it('keeps completed placeholder cards below full progress until real cards replace them', () => {
    const item = {
      scan_job_id: 41,
      library_id: 7,
      item_key: '/media/movies/interstellar.mkv',
      media_type: 'movie',
      title: 'Interstellar',
      year: 2014,
      overview: 'A team travels through a wormhole.',
      poster_path: null,
      backdrop_path: null,
      metadata_status: 'matched',
      remote_media_type: 'movie',
      season_number: null,
      episode_number: null,
      item_index: 1,
      total_items: 1,
      stage: 'completed',
      progress_percent: 100,
    }

    expect(formatScanItemCardProgressLabel(item)).toBe('Updating card')
    expect(getScanItemCardProgressPercent(item)).toBe(96)
  })

  it('matches a movie detail against scan runtime items by title and file path', () => {
    const runtime: LibraryScanRuntime = {
      scanJob: buildScanJob({
        phase: 'enriching',
      }),
      items: [
        {
          scan_job_id: 41,
          library_id: 7,
          item_key: '/media/movies/Interstellar (2014)/Interstellar.mkv',
          media_type: 'movie',
          title: 'Interstellar',
          year: 2014,
          overview: 'A team travels through a wormhole.',
          poster_path: '/cache/interstellar.jpg',
          backdrop_path: null,
          metadata_status: 'matched',
          remote_media_type: 'movie',
          season_number: null,
          episode_number: null,
          item_index: 1,
          total_items: 1,
          stage: 'metadata',
          progress_percent: 36,
        },
      ],
    }

    const matchingItems = getMediaItemScanRuntimeItems(
      {
        media_type: 'movie',
        title: 'Interstellar',
        source_title: 'Interstellar',
        original_title: null,
      },
      runtime,
    )

    expect(matchingItems).toHaveLength(1)
    expect(
      formatMediaItemScanStatusCopy(
        {
          media_type: 'movie',
          title: 'Interstellar',
          source_title: 'Interstellar',
          original_title: null,
        },
        runtime,
      ),
    ).toBe('Fetching metadata')
  })

  it('filters series scan runtime items by the selected season', () => {
    const runtime: LibraryScanRuntime = {
      scanJob: buildScanJob({
        phase: 'enriching',
      }),
      items: [
        {
          scan_job_id: 41,
          library_id: 7,
          item_key: '/media/series/Arcane/Season 1/Arcane.S01E02.mkv',
          media_type: 'episode',
          title: 'Some Mystery',
          year: 2021,
          overview: null,
          poster_path: '/cache/arcane.jpg',
          backdrop_path: null,
          metadata_status: 'matched',
          remote_media_type: 'series',
          season_number: 1,
          episode_number: 2,
          item_index: 1,
          total_items: 2,
          stage: 'artwork',
          progress_percent: 76,
        },
        {
          scan_job_id: 41,
          library_id: 7,
          item_key: '/media/series/Arcane/Season 2/Arcane.S02E01.mkv',
          media_type: 'episode',
          title: 'Another Story',
          year: 2021,
          overview: null,
          poster_path: null,
          backdrop_path: null,
          metadata_status: null,
          remote_media_type: null,
          season_number: 2,
          episode_number: 1,
          item_index: 2,
          total_items: 2,
          stage: 'metadata',
          progress_percent: 36,
        },
      ],
    }

    const matchingItems = getMediaItemScanRuntimeItems(
      {
        media_type: 'series',
        title: 'Arcane',
        source_title: 'Arcane',
        original_title: 'Arcane',
      },
      runtime,
      { seasonNumber: 1 },
    )

    expect(matchingItems).toHaveLength(1)
    expect(matchingItems[0].season_number).toBe(1)
    expect(
      formatMediaItemScanStatusCopy(
        {
          media_type: 'series',
          title: 'Arcane',
          source_title: 'Arcane',
          original_title: 'Arcane',
        },
        runtime,
        { seasonNumber: 1 },
      ),
    ).toBe('Fetching artwork & overview')
  })

  it('surfaces failed scan copy even when the library is no longer actively syncing', () => {
    const runtime: LibraryScanRuntime = {
      scanJob: buildScanJob({
        status: 'failed',
        phase: 'finished',
        error_message: 'Metadata enrichment failed: TMDB request timed out',
      }),
      items: [],
    }

    expect(hasFailedLibraryScan(null, runtime)).toBe(true)
    expect(formatFailedScanCopy(null, runtime)).toBe(
      'Metadata enrichment failed: TMDB request timed out',
    )
    expect(isLibraryScanActive(null, runtime)).toBe(false)
  })
})
