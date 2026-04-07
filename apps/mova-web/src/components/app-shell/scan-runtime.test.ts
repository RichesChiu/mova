import { describe, expect, it } from 'vitest'
import type { ScanJob } from '../../api/types'
import {
  formatPendingScanPlaceholderCopy,
  formatScanItemProgressCopy,
  formatScanJobStatusCopy,
  getScanJobProgressPercent,
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
    expect(formatScanJobStatusCopy(null, runtime)).toBe('正在发现文件 4/20')
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
      '正在补全 Interstellar 的海报和简介',
    )
    expect(getScanJobProgressPercent(null, runtime)).toBe(68)
    expect(formatScanItemProgressCopy(runtime.items[0])).toBe('正在获取海报、剧照和简介')
  })
})
