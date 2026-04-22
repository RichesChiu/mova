import type { MediaItem, ScanJob } from '../../api/types'
import { translateCurrent } from '../../i18n'

export interface ScanRuntimeItem {
  scan_job_id: number
  library_id: number
  item_key: string
  media_type: string
  title: string
  season_number: number | null
  episode_number: number | null
  item_index: number
  total_items: number
  stage: string
  progress_percent: number
}

export interface LibraryScanRuntime {
  items: ScanRuntimeItem[]
  scanJob: ScanJob | null
}

export type ScanRuntimeByLibrary = Record<number, LibraryScanRuntime>

export const EMPTY_LIBRARY_SCAN_RUNTIME: LibraryScanRuntime = {
  items: [],
  scanJob: null,
}

export const isScanJobActive = (scanJob: ScanJob | null | undefined) =>
  scanJob?.status === 'pending' || scanJob?.status === 'running'

export const getLibraryScanRuntime = (
  scanRuntimeByLibrary: ScanRuntimeByLibrary,
  libraryId: number,
) => scanRuntimeByLibrary[libraryId] ?? EMPTY_LIBRARY_SCAN_RUNTIME

export const getScanRuntimeItems = (runtime: LibraryScanRuntime | null | undefined) =>
  runtime?.items ?? []

export const getPrimaryScanRuntimeItem = (runtime: LibraryScanRuntime | null | undefined) =>
  getScanRuntimeItems(runtime)[0] ?? null

export const getEffectiveScanJob = (
  scanJob: ScanJob | null | undefined,
  runtime: LibraryScanRuntime | null | undefined,
) => runtime?.scanJob ?? scanJob ?? null

export const formatScanJobStatusCopy = (
  scanJob: ScanJob | null | undefined,
  runtime: LibraryScanRuntime | null | undefined,
) => {
  const effectiveScanJob = getEffectiveScanJob(scanJob, runtime)
  const primaryItem = getPrimaryScanRuntimeItem(runtime)
  const activeItemCount = getScanRuntimeItems(runtime).length

  if (!effectiveScanJob) {
    if (primaryItem) {
      return primaryItem.stage === 'artwork'
        ? `${primaryItem.title} · ${translateCurrent('Fetching artwork & overview')}`
        : `${primaryItem.title} · ${translateCurrent('Fetching metadata')}`
    }

    return null
  }

  if (effectiveScanJob.status === 'pending') {
    return translateCurrent('Queued for scan')
  }

  if (effectiveScanJob.status === 'failed') {
    return effectiveScanJob.error_message ?? translateCurrent('Scan failed')
  }

  if (effectiveScanJob.status === 'success') {
    return null
  }

  switch (effectiveScanJob.phase) {
    case 'discovering':
      if (activeItemCount > 0) {
        return activeItemCount > 1
          ? translateCurrent('Discovered {{count}} new items', { count: activeItemCount })
          : translateCurrent('Discovered {{title}}', {
              title: primaryItem?.title ?? translateCurrent('new item'),
            })
      }
      return effectiveScanJob.total_files > 0
        ? translateCurrent('Scanning files {{scanned}}/{{total}}', {
            scanned: effectiveScanJob.scanned_files,
            total: effectiveScanJob.total_files,
          })
        : translateCurrent('Scanning files {{scanned}}', {
            scanned: effectiveScanJob.scanned_files,
          })
    case 'enriching':
      if (primaryItem) {
        if (primaryItem.stage === 'discovered') {
          return `${primaryItem.title} · ${translateCurrent('Waiting for metadata')}`
        }

        if (primaryItem.stage === 'artwork') {
          return `${primaryItem.title} · ${translateCurrent('Fetching artwork & overview')}`
        }

        if (primaryItem.stage === 'completed') {
          return `${primaryItem.title} · ${translateCurrent('Waiting to save')}`
        }

        return `${primaryItem.title} · ${translateCurrent('Fetching metadata')}`
      }
      return translateCurrent('Enriching metadata')
    case 'syncing':
      return activeItemCount > 0
        ? translateCurrent('Saving {{count}} items', { count: activeItemCount })
        : translateCurrent('Saving to library')
    default:
      return translateCurrent('Library sync in progress')
  }
}

export const getScanJobProgressPercent = (
  scanJob: ScanJob | null | undefined,
  runtime: LibraryScanRuntime | null | undefined,
) => {
  const effectiveScanJob = getEffectiveScanJob(scanJob, runtime)

  if (!effectiveScanJob) {
    const currentItem = getPrimaryScanRuntimeItem(runtime)
    return currentItem ? Math.max(10, currentItem.progress_percent) : 0
  }

  if (effectiveScanJob.status === 'success') {
    return 100
  }

  if (effectiveScanJob.status === 'pending') {
    return 4
  }

  if (effectiveScanJob.phase === 'discovering') {
    if (effectiveScanJob.total_files <= 0) {
      return 12
    }

    return Math.max(
      12,
      Math.min(
        45,
        Math.round((effectiveScanJob.scanned_files / effectiveScanJob.total_files) * 45),
      ),
    )
  }

  if (effectiveScanJob.phase === 'enriching') {
    const currentItem = getPrimaryScanRuntimeItem(runtime)
    if (!currentItem || currentItem.total_items <= 0) {
      return 52
    }

    const completedItems = Math.max(0, currentItem.item_index - 1)
    const itemFraction = Math.max(0, Math.min(1, currentItem.progress_percent / 100))
    const totalFraction = (completedItems + itemFraction) / currentItem.total_items

    return Math.max(46, Math.min(90, Math.round(45 + totalFraction * 45)))
  }

  if (effectiveScanJob.phase === 'syncing') {
    return 94
  }

  if (effectiveScanJob.status === 'failed') {
    if (effectiveScanJob.total_files > 0) {
      return Math.max(
        8,
        Math.min(
          94,
          Math.round((effectiveScanJob.scanned_files / effectiveScanJob.total_files) * 94),
        ),
      )
    }

    return 8
  }

  return 16
}

export const isLibraryScanActive = (
  scanJob: ScanJob | null | undefined,
  runtime: LibraryScanRuntime | null | undefined,
) => {
  const effectiveScanJob = getEffectiveScanJob(scanJob, runtime)

  if (isScanJobActive(effectiveScanJob)) {
    return true
  }

  return getScanRuntimeItems(runtime).length > 0
}

export const hasFailedLibraryScan = (
  scanJob: ScanJob | null | undefined,
  runtime: LibraryScanRuntime | null | undefined,
) => getEffectiveScanJob(scanJob, runtime)?.status === 'failed'

export const formatFailedScanCopy = (
  scanJob: ScanJob | null | undefined,
  runtime: LibraryScanRuntime | null | undefined,
) => {
  const effectiveScanJob = getEffectiveScanJob(scanJob, runtime)
  if (effectiveScanJob?.status !== 'failed') {
    return null
  }

  return effectiveScanJob.error_message ?? translateCurrent('The most recent scan failed')
}

export const shouldShowScanPlaceholder = (
  scanJob: ScanJob | null | undefined,
  runtime: LibraryScanRuntime | null | undefined,
) => {
  if (getScanRuntimeItems(runtime).length > 0) {
    return true
  }

  return isScanJobActive(getEffectiveScanJob(scanJob, runtime))
}

export const formatPendingScanPlaceholderCopy = (
  scanJob: ScanJob | null | undefined,
  runtime: LibraryScanRuntime | null | undefined,
  fallbackTitle: string,
) => {
  const scanCopy = formatScanJobStatusCopy(scanJob, runtime)
  if (scanCopy) {
    return scanCopy
  }

  return translateCurrent('Syncing {{title}}', { title: fallbackTitle })
}

export const formatScanItemMeta = (item: ScanRuntimeItem) => {
  if (
    typeof item.season_number === 'number' &&
    Number.isFinite(item.season_number) &&
    typeof item.episode_number === 'number' &&
    Number.isFinite(item.episode_number)
  ) {
    return `S${String(item.season_number).padStart(2, '0')} · E${String(item.episode_number).padStart(2, '0')}`
  }

  return item.media_type === 'series' ? translateCurrent('Series') : translateCurrent('Movie')
}

export const formatScanItemProgressCopy = (item: ScanRuntimeItem) => {
  switch (item.stage) {
    case 'discovered':
      return translateCurrent('Waiting for metadata')
    case 'artwork':
      return translateCurrent('Fetching artwork & overview')
    case 'completed':
      return translateCurrent('Waiting to save to library')
    default:
      return translateCurrent('Fetching metadata')
  }
}

const normalizeScanMatchText = (value: string | null | undefined) =>
  (value ?? '')
    .toLocaleLowerCase()
    .replace(/[\s._\-()[\]{}:/\\|'"`,!?]+/g, '')
    .trim()

const buildMediaItemScanMatchCandidates = (
  mediaItem: Pick<MediaItem, 'title' | 'source_title' | 'original_title'>,
) =>
  [...new Set([mediaItem.title, mediaItem.source_title, mediaItem.original_title])]
    .map((value) => normalizeScanMatchText(value))
    .filter((value) => value.length > 0)

const matchesMediaItemScanCandidate = (item: ScanRuntimeItem, candidates: string[]) => {
  const scanTexts = [item.title, item.item_key].map((value) => normalizeScanMatchText(value))

  return candidates.some((candidate) => scanTexts.some((scanText) => scanText.includes(candidate)))
}

export const getMediaItemScanRuntimeItems = (
  mediaItem:
    | Pick<MediaItem, 'media_type' | 'title' | 'source_title' | 'original_title'>
    | null
    | undefined,
  runtime: LibraryScanRuntime | null | undefined,
  options?: {
    seasonNumber?: number | null
  },
) => {
  if (!mediaItem) {
    return []
  }

  const candidates = buildMediaItemScanMatchCandidates(mediaItem)
  if (candidates.length === 0) {
    return []
  }

  return getScanRuntimeItems(runtime)
    .filter((item) => {
      if (!matchesMediaItemScanCandidate(item, candidates)) {
        return false
      }

      if (mediaItem.media_type === 'movie') {
        return item.media_type === 'movie'
      }

      if (mediaItem.media_type === 'series') {
        if (item.media_type === 'movie') {
          return false
        }

        if (
          typeof options?.seasonNumber === 'number' &&
          Number.isFinite(options.seasonNumber) &&
          item.season_number !== options.seasonNumber
        ) {
          return false
        }

        return true
      }

      return false
    })
    .sort((left, right) => {
      const seasonDiff = (left.season_number ?? 0) - (right.season_number ?? 0)
      if (seasonDiff !== 0) {
        return seasonDiff
      }

      const episodeDiff = (left.episode_number ?? 0) - (right.episode_number ?? 0)
      if (episodeDiff !== 0) {
        return episodeDiff
      }

      return left.item_index - right.item_index
    })
}

export const formatMediaItemScanStatusCopy = (
  mediaItem:
    | Pick<MediaItem, 'media_type' | 'title' | 'source_title' | 'original_title'>
    | null
    | undefined,
  runtime: LibraryScanRuntime | null | undefined,
  options?: {
    seasonNumber?: number | null
  },
) => {
  const matchingItems = getMediaItemScanRuntimeItems(mediaItem, runtime, options)
  const primaryItem = matchingItems[0]

  if (!primaryItem) {
    return formatScanJobStatusCopy(null, runtime)
  }

  const progressCopy = formatScanItemProgressCopy(primaryItem)
  if (matchingItems.length > 1) {
    return `${progressCopy} · ${translateCurrent('{{count}} more related items syncing', {
      count: matchingItems.length - 1,
    })}`
  }

  return progressCopy
}
