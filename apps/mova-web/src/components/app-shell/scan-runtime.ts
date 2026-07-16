import type { MediaItem, ScanJob } from '../../api/types'
import { translateCurrent } from '../../i18n'
import { formatLibraryMediaTypeLabel } from '../../lib/media-type-label'

export interface ScanRuntimeItem {
  scan_job_id: number
  library_id: number
  item_key: string
  media_type: string
  title: string
  year: number | null
  overview: string | null
  poster_path: string | null
  backdrop_path: string | null
  metadata_status: string | null
  remote_media_type: string | null
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
      return effectiveScanJob.total_files > 0 &&
        effectiveScanJob.scanned_files < effectiveScanJob.total_files
        ? translateCurrent('Scanning files {{scanned}}/{{total}}', {
            scanned: effectiveScanJob.scanned_files,
            total: effectiveScanJob.total_files,
          })
        : translateCurrent('Discovered {{count}} files', {
            count: effectiveScanJob.scanned_files,
          })
    case 'processing':
      if (primaryItem) {
        if (primaryItem.stage === 'analyzed') {
          return `${primaryItem.title} · ${translateCurrent('Analyzing local media')}`
        }

        if (primaryItem.stage === 'pending_committed') {
          return `${primaryItem.title} · ${translateCurrent('Waiting for metadata')}`
        }

        if (primaryItem.stage === 'artwork') {
          return `${primaryItem.title} · ${translateCurrent('Fetching artwork & overview')}`
        }

        if (primaryItem.stage === 'completed') {
          return `${primaryItem.title} · ${translateCurrent('Saved to library')}`
        }

        return `${primaryItem.title} · ${translateCurrent('Fetching metadata')}`
      }
      return effectiveScanJob.local_committed_files < effectiveScanJob.total_files
        ? translateCurrent('Analyzing local media')
        : translateCurrent('Enriching metadata')
    case 'finalizing':
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
  return effectiveScanJob ? Math.max(0, Math.min(100, effectiveScanJob.progress_percent)) : 0
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

  const typeLabel = formatLibraryMediaTypeLabel(item.media_type, translateCurrent)

  return item.year ? `${typeLabel} · ${item.year}` : typeLabel
}

export const formatScanItemProgressCopy = (item: ScanRuntimeItem) => {
  switch (item.stage) {
    case 'analyzed':
      return translateCurrent('Analyzing local media')
    case 'pending_committed':
      return translateCurrent('Waiting for metadata')
    case 'artwork':
      return translateCurrent('Fetching artwork & overview')
    case 'completed':
      return translateCurrent('Saved to library')
    default:
      return translateCurrent('Fetching metadata')
  }
}

export const formatScanItemCardProgressLabel = (item: ScanRuntimeItem) =>
  item.stage === 'completed' ? translateCurrent('Saved to library') : translateCurrent('syncing')

export const getScanItemCardProgressPercent = (item: ScanRuntimeItem) =>
  Math.max(0, Math.min(100, item.progress_percent))

export const formatScanItemCardSummary = (item: ScanRuntimeItem) =>
  item.stage === 'completed' && item.overview ? item.overview : formatScanItemProgressCopy(item)

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
