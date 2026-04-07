import type { ScanJob } from '../../api/types'

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
        ? `正在补全 ${primaryItem.title} 的海报和简介`
        : `正在匹配 ${primaryItem.title} 的元数据`
    }

    return null
  }

  if (effectiveScanJob.status === 'pending') {
    return '等待开始扫描'
  }

  if (effectiveScanJob.status === 'failed') {
    return effectiveScanJob.error_message ?? '扫描失败'
  }

  if (effectiveScanJob.status === 'success') {
    return '扫描完成'
  }

  switch (effectiveScanJob.phase) {
    case 'discovering':
      if (activeItemCount > 0) {
        return activeItemCount > 1
          ? `已发现 ${activeItemCount} 个新条目，正在继续扫描目录`
          : `已发现 ${primaryItem?.title ?? '新条目'}，正在继续扫描目录`
      }
      return effectiveScanJob.total_files > 0
        ? `正在发现文件 ${effectiveScanJob.scanned_files}/${effectiveScanJob.total_files}`
        : `正在发现文件 ${effectiveScanJob.scanned_files}`
    case 'enriching':
      if (primaryItem) {
        if (primaryItem.stage === 'discovered') {
          return `已发现 ${primaryItem.title}，等待补全元数据`
        }

        if (primaryItem.stage === 'artwork') {
          return `正在补全 ${primaryItem.title} 的海报和简介`
        }

        if (primaryItem.stage === 'completed') {
          return `已准备好 ${primaryItem.title} 的元数据，等待写入媒体库`
        }

        return `正在匹配 ${primaryItem.title} 的元数据`
      }
      return '正在补全条目元数据'
    case 'syncing':
      return activeItemCount > 0 ? `正在写入 ${activeItemCount} 个新条目到媒体库` : '正在写入媒体库'
    default:
      return '正在扫描媒体库'
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

  return `正在准备 ${fallbackTitle} 的扫描结果`
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

  return item.media_type
}

export const formatScanItemProgressCopy = (item: ScanRuntimeItem) => {
  switch (item.stage) {
    case 'discovered':
      return '已发现文件，等待获取元数据'
    case 'artwork':
      return '正在获取海报、剧照和简介'
    case 'completed':
      return '元数据已准备好，等待写入媒体库'
    default:
      return '正在匹配元数据'
  }
}
