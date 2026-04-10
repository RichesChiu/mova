import { describe, expect, it } from 'vitest'
import type { ScanJob } from '../../api/types'
import {
  formatFailedScanCopy,
  formatMediaItemScanStatusCopy,
  formatPendingScanPlaceholderCopy,
  formatScanItemProgressCopy,
  formatScanJobStatusCopy,
  getMediaItemScanRuntimeItems,
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
