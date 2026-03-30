import { useQueryClient } from '@tanstack/react-query'
import { useEffect } from 'react'
import type { LibraryDetail, ScanJob } from '../../api/types'

const SERVER_EVENTS_URL = '/api/events'

type ScanJobRealtimeEvent =
  | {
      type: 'scan.job.updated'
      scan_job: ScanJob
    }
  | {
      type: 'scan.job.finished'
      scan_job: ScanJob
    }

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null

const isScanJob = (value: unknown): value is ScanJob =>
  isRecord(value) &&
  typeof value.id === 'number' &&
  typeof value.library_id === 'number' &&
  typeof value.status === 'string' &&
  typeof value.total_files === 'number' &&
  typeof value.scanned_files === 'number' &&
  typeof value.created_at === 'string' &&
  (typeof value.started_at === 'string' || value.started_at === null) &&
  (typeof value.finished_at === 'string' || value.finished_at === null) &&
  (typeof value.error_message === 'string' || value.error_message === null)

const parseRealtimeEvent = (raw: string): ScanJobRealtimeEvent | null => {
  try {
    const parsed: unknown = JSON.parse(raw)
    if (!isRecord(parsed) || !isScanJob(parsed.scan_job)) {
      return null
    }

    if (parsed.type === 'scan.job.updated') {
      return {
        type: 'scan.job.updated',
        scan_job: parsed.scan_job,
      }
    }

    if (parsed.type === 'scan.job.finished') {
      return {
        type: 'scan.job.finished',
        scan_job: parsed.scan_job,
      }
    }

    return null
  } catch {
    return null
  }
}

const patchLibraryLastScan = (current: LibraryDetail | undefined, scanJob: ScanJob) => {
  if (!current) {
    return current
  }

  return {
    ...current,
    last_scan: scanJob,
  }
}

export const useServerEvents = ({ enabled }: { enabled: boolean }) => {
  const queryClient = useQueryClient()

  useEffect(() => {
    if (!enabled || typeof EventSource === 'undefined') {
      return
    }

    const eventSource = new EventSource(SERVER_EVENTS_URL)

    const handleScanJobUpdated = (event: MessageEvent<string>) => {
      const payload = parseRealtimeEvent(event.data)
      if (!payload || payload.type !== 'scan.job.updated') {
        return
      }

      queryClient.setQueryData<LibraryDetail | undefined>(
        ['library', payload.scan_job.library_id],
        (current) => patchLibraryLastScan(current, payload.scan_job),
      )
    }

    const handleScanJobFinished = (event: MessageEvent<string>) => {
      const payload = parseRealtimeEvent(event.data)
      if (!payload || payload.type !== 'scan.job.finished') {
        return
      }

      queryClient.setQueryData<LibraryDetail | undefined>(
        ['library', payload.scan_job.library_id],
        (current) => patchLibraryLastScan(current, payload.scan_job),
      )

      // 扫描结束后再统一刷新重查询，避免每条进度事件都触发一轮 HTTP refetch。
      void Promise.all([
        queryClient.invalidateQueries({ queryKey: ['library', payload.scan_job.library_id] }),
        queryClient.invalidateQueries({ queryKey: ['library-media', payload.scan_job.library_id] }),
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-shelf'] }),
      ])
    }

    eventSource.addEventListener('scan.job.updated', handleScanJobUpdated as EventListener)
    eventSource.addEventListener('scan.job.finished', handleScanJobFinished as EventListener)

    return () => {
      eventSource.removeEventListener('scan.job.updated', handleScanJobUpdated as EventListener)
      eventSource.removeEventListener('scan.job.finished', handleScanJobFinished as EventListener)
      eventSource.close()
    }
  }, [enabled, queryClient])
}
