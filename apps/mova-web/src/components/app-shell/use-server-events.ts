import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import { matchPath, useLocation, useNavigate } from 'react-router-dom'
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
  | {
      type: 'library.updated'
      library_id: number
    }
  | {
      type: 'library.deleted'
      library_id: number
    }
  | {
      type: 'media_item.metadata.updated'
      library_id: number
      media_item_id: number
    }

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null

const isNumber = (value: unknown): value is number => typeof value === 'number'

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
    if (!isRecord(parsed)) {
      return null
    }

    if (parsed.type === 'scan.job.updated' && isScanJob(parsed.scan_job)) {
      return {
        type: 'scan.job.updated',
        scan_job: parsed.scan_job,
      }
    }

    if (parsed.type === 'scan.job.finished' && isScanJob(parsed.scan_job)) {
      return {
        type: 'scan.job.finished',
        scan_job: parsed.scan_job,
      }
    }

    if (parsed.type === 'library.updated' && isNumber(parsed.library_id)) {
      return {
        type: 'library.updated',
        library_id: parsed.library_id,
      }
    }

    if (parsed.type === 'library.deleted' && isNumber(parsed.library_id)) {
      return {
        type: 'library.deleted',
        library_id: parsed.library_id,
      }
    }

    if (
      parsed.type === 'media_item.metadata.updated' &&
      isNumber(parsed.library_id) &&
      isNumber(parsed.media_item_id)
    ) {
      return {
        type: 'media_item.metadata.updated',
        library_id: parsed.library_id,
        media_item_id: parsed.media_item_id,
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
  const location = useLocation()
  const navigate = useNavigate()
  const pathnameRef = useRef(location.pathname)
  const hasOpenedRef = useRef(false)
  const shouldRecoverRef = useRef(false)

  useEffect(() => {
    pathnameRef.current = location.pathname
  }, [location.pathname])

  useEffect(() => {
    if (!enabled || typeof EventSource === 'undefined') {
      return
    }

    const eventSource = new EventSource(SERVER_EVENTS_URL)

    const recoverQueriesAfterReconnect = () => {
      const activeLibraryMatch = matchPath('/libraries/:libraryId', pathnameRef.current)
      const activeMediaItemMatch = matchPath('/media-items/:mediaItemId', pathnameRef.current)
      const activeLibraryId =
        activeLibraryMatch?.params.libraryId !== undefined
          ? Number(activeLibraryMatch.params.libraryId)
          : Number.NaN
      const activeMediaItemId =
        activeMediaItemMatch?.params.mediaItemId !== undefined
          ? Number(activeMediaItemMatch.params.mediaItemId)
          : Number.NaN

      const recoveryTasks = [
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['continue-watching'] }),
        queryClient.invalidateQueries({ queryKey: ['watch-history'] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-detail'] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-shelf'] }),
      ]

      if (Number.isFinite(activeLibraryId)) {
        recoveryTasks.push(
          queryClient.invalidateQueries({ queryKey: ['library', activeLibraryId] }),
          queryClient.invalidateQueries({ queryKey: ['library-media', activeLibraryId] }),
        )
      }

      if (Number.isFinite(activeMediaItemId)) {
        recoveryTasks.push(
          queryClient.invalidateQueries({ queryKey: ['media-item', activeMediaItemId] }),
          queryClient.invalidateQueries({ queryKey: ['media-episode-outline', activeMediaItemId] }),
          queryClient.invalidateQueries({
            queryKey: ['media-item-playback-progress', activeMediaItemId],
          }),
        )
      }

      void Promise.all(recoveryTasks)
    }

    const handleOpen = () => {
      if (!hasOpenedRef.current) {
        hasOpenedRef.current = true
        shouldRecoverRef.current = false
        return
      }

      if (!shouldRecoverRef.current) {
        return
      }

      shouldRecoverRef.current = false
      recoverQueriesAfterReconnect()
    }

    const handleError = () => {
      shouldRecoverRef.current = true
    }

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
        queryClient.invalidateQueries({
          queryKey: ['home-library-detail', payload.scan_job.library_id],
        }),
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({
          queryKey: ['home-library-shelf', payload.scan_job.library_id],
        }),
      ])
    }

    const handleLibraryUpdated = (event: MessageEvent<string>) => {
      const payload = parseRealtimeEvent(event.data)
      if (!payload || payload.type !== 'library.updated') {
        return
      }

      void Promise.all([
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['library', payload.library_id] }),
        queryClient.invalidateQueries({ queryKey: ['library-media', payload.library_id] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-detail', payload.library_id] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-shelf', payload.library_id] }),
      ])
    }

    const handleLibraryDeleted = (event: MessageEvent<string>) => {
      const payload = parseRealtimeEvent(event.data)
      if (!payload || payload.type !== 'library.deleted') {
        return
      }

      const activeLibraryMatch = matchPath('/libraries/:libraryId', pathnameRef.current)
      const activeLibraryId =
        activeLibraryMatch?.params.libraryId !== undefined
          ? Number(activeLibraryMatch.params.libraryId)
          : Number.NaN

      queryClient.removeQueries({ queryKey: ['library', payload.library_id] })
      queryClient.removeQueries({ queryKey: ['library-media', payload.library_id] })
      queryClient.removeQueries({ queryKey: ['home-library-detail', payload.library_id] })
      queryClient.removeQueries({ queryKey: ['home-library-shelf', payload.library_id] })

      void queryClient.invalidateQueries({ queryKey: ['libraries'] })

      if (activeLibraryId === payload.library_id) {
        window.alert('当前媒体库已被删除。点击确认后将返回主页。')
        navigate('/', { replace: true })
      }
    }

    const handleMediaItemMetadataUpdated = (event: MessageEvent<string>) => {
      const payload = parseRealtimeEvent(event.data)
      if (!payload || payload.type !== 'media_item.metadata.updated') {
        return
      }

      void Promise.all([
        queryClient.invalidateQueries({ queryKey: ['media-item', payload.media_item_id] }),
        queryClient.invalidateQueries({
          queryKey: ['media-episode-outline', payload.media_item_id],
        }),
        queryClient.invalidateQueries({ queryKey: ['library-media', payload.library_id] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-shelf', payload.library_id] }),
        queryClient.invalidateQueries({ queryKey: ['continue-watching'] }),
        queryClient.invalidateQueries({ queryKey: ['watch-history'] }),
      ])
    }

    eventSource.addEventListener('scan.job.updated', handleScanJobUpdated as EventListener)
    eventSource.addEventListener('scan.job.finished', handleScanJobFinished as EventListener)
    eventSource.addEventListener('library.updated', handleLibraryUpdated as EventListener)
    eventSource.addEventListener('library.deleted', handleLibraryDeleted as EventListener)
    eventSource.addEventListener(
      'media_item.metadata.updated',
      handleMediaItemMetadataUpdated as EventListener,
    )
    eventSource.addEventListener('open', handleOpen as EventListener)
    eventSource.addEventListener('error', handleError as EventListener)

    return () => {
      eventSource.removeEventListener('scan.job.updated', handleScanJobUpdated as EventListener)
      eventSource.removeEventListener('scan.job.finished', handleScanJobFinished as EventListener)
      eventSource.removeEventListener('library.updated', handleLibraryUpdated as EventListener)
      eventSource.removeEventListener('library.deleted', handleLibraryDeleted as EventListener)
      eventSource.removeEventListener(
        'media_item.metadata.updated',
        handleMediaItemMetadataUpdated as EventListener,
      )
      eventSource.removeEventListener('open', handleOpen as EventListener)
      eventSource.removeEventListener('error', handleError as EventListener)
      eventSource.close()
    }
  }, [enabled, navigate, queryClient])
}
