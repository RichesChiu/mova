import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { matchPath, useLocation, useNavigate } from 'react-router-dom'
import { getRealtimeState } from '../../api/client'
import type { HomeResponse, RealtimeState, ScanJob } from '../../api/types'
import type { ScanRuntimeByLibrary, ScanRuntimeItem } from './scan-runtime'

const SERVER_EVENTS_URL = '/api/realtime/events'
const MAX_SCAN_RUNTIME_ITEMS_PER_LIBRARY = 40

interface ResourceChange {
  resource: string
  revision: number
}

interface ResourcesChangedPayload {
  version: number
  changes: ResourceChange[]
}

interface ScanProgressPayload {
  version: number
  scan_job: ScanJob
  items: ScanRuntimeItem[]
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null

const isScanJob = (value: unknown): value is ScanJob =>
  isRecord(value) &&
  typeof value.id === 'number' &&
  typeof value.library_id === 'number' &&
  typeof value.status === 'string' &&
  (typeof value.phase === 'string' || value.phase === null || value.phase === undefined) &&
  typeof value.total_files === 'number' &&
  typeof value.scanned_files === 'number'

const isScanRuntimeItem = (value: unknown): value is ScanRuntimeItem =>
  isRecord(value) &&
  typeof value.scan_job_id === 'number' &&
  typeof value.library_id === 'number' &&
  typeof value.item_key === 'string' &&
  typeof value.media_type === 'string' &&
  typeof value.title === 'string' &&
  typeof value.item_index === 'number' &&
  typeof value.total_items === 'number' &&
  typeof value.stage === 'string' &&
  typeof value.progress_percent === 'number'

const parseResourcesChanged = (raw: string): ResourcesChangedPayload | null => {
  try {
    const value: unknown = JSON.parse(raw)
    if (!isRecord(value) || value.version !== 1 || !Array.isArray(value.changes)) {
      return null
    }
    const changes = value.changes.filter(
      (change): change is ResourceChange =>
        isRecord(change) &&
        typeof change.resource === 'string' &&
        typeof change.revision === 'number',
    )
    return { version: 1, changes }
  } catch {
    return null
  }
}

const parseScanProgress = (raw: string): ScanProgressPayload | null => {
  try {
    const value: unknown = JSON.parse(raw)
    if (
      !isRecord(value) ||
      value.version !== 1 ||
      !isScanJob(value.scan_job) ||
      !Array.isArray(value.items)
    ) {
      return null
    }
    return {
      version: 1,
      scan_job: value.scan_job,
      items: value.items.filter(isScanRuntimeItem),
    }
  } catch {
    return null
  }
}

const mergeScanRuntimeItems = (
  current: ScanRuntimeItem[],
  incoming: ScanRuntimeItem[],
  scanJobId: number,
) => {
  const latestByKey = new Map(
    current.filter((item) => item.scan_job_id === scanJobId).map((item) => [item.item_key, item]),
  )
  for (const item of incoming) {
    latestByKey.set(item.item_key, item)
  }
  return [...latestByKey.values()]
    .sort((left, right) => right.item_index - left.item_index)
    .slice(0, MAX_SCAN_RUNTIME_ITEMS_PER_LIBRARY)
}

const applyScanPayload = (
  current: ScanRuntimeByLibrary,
  payload: ScanProgressPayload,
): ScanRuntimeByLibrary => ({
  ...current,
  [payload.scan_job.library_id]: {
    scanJob: payload.scan_job,
    items: mergeScanRuntimeItems(
      current[payload.scan_job.library_id]?.items ?? [],
      payload.items,
      payload.scan_job.id,
    ),
  },
})

const buildActiveScanRuntime = (
  state: RealtimeState,
  current: ScanRuntimeByLibrary = {},
): ScanRuntimeByLibrary =>
  Object.fromEntries(
    state.active_scans.map((scanJob) => {
      const currentRuntime = current[scanJob.library_id]
      return [
        scanJob.library_id,
        {
          scanJob,
          items: currentRuntime?.scanJob?.id === scanJob.id ? currentRuntime.items : [],
        },
      ]
    }),
  )

const libraryResource = (resource: string) => {
  const match = /^library:(\d+):(settings|catalog|scan)$/.exec(resource)
  if (!match) {
    return null
  }
  return { id: Number(match[1]), kind: match[2] }
}

export const useServerEvents = ({ enabled }: { enabled: boolean }) => {
  const queryClient = useQueryClient()
  const location = useLocation()
  const navigate = useNavigate()
  const pathnameRef = useRef(location.pathname)
  const serverEpochRef = useRef<string | null>(null)
  const appliedRevisionsRef = useRef(new Map<string, number>())
  const requestedRevisionsRef = useRef(new Map<string, number>())
  const inFlightResourcesRef = useRef(new Set<string>())
  const [scanRuntimeByLibrary, setScanRuntimeByLibrary] = useState<ScanRuntimeByLibrary>({})

  useEffect(() => {
    pathnameRef.current = location.pathname
  }, [location.pathname])

  useEffect(() => {
    if (!enabled || typeof EventSource === 'undefined') {
      setScanRuntimeByLibrary({})
      serverEpochRef.current = null
      appliedRevisionsRef.current.clear()
      requestedRevisionsRef.current.clear()
      inFlightResourcesRef.current.clear()
      return
    }

    let disposed = false
    const eventSource = new EventSource(SERVER_EVENTS_URL)

    const invalidateResource = async (resource: string) => {
      const tasks: Promise<unknown>[] = []
      const library = libraryResource(resource)

      if (resource === 'libraries' || resource.endsWith(':libraries')) {
        tasks.push(
          queryClient.invalidateQueries({ queryKey: ['libraries'] }),
          queryClient.invalidateQueries({ queryKey: ['home'] }),
        )
      } else if (library?.kind === 'settings') {
        tasks.push(
          queryClient.invalidateQueries({ queryKey: ['libraries'] }),
          queryClient.invalidateQueries({ queryKey: ['library', library.id] }),
          queryClient.invalidateQueries({ queryKey: ['home'] }),
        )
      } else if (library?.kind === 'catalog') {
        tasks.push(
          queryClient.invalidateQueries({ queryKey: ['library', library.id] }),
          queryClient.invalidateQueries({ queryKey: ['library-media', library.id] }),
          queryClient.invalidateQueries({ queryKey: ['home'] }),
        )

        const mediaMatch = matchPath('/media-items/:mediaItemId', pathnameRef.current)
        const mediaItemId = Number(mediaMatch?.params.mediaItemId)
        if (Number.isFinite(mediaItemId)) {
          tasks.push(
            queryClient.invalidateQueries({ queryKey: ['media-item', mediaItemId] }),
            queryClient.invalidateQueries({ queryKey: ['media-episode-outline', mediaItemId] }),
          )
        }
      } else if (library?.kind === 'scan') {
        tasks.push(
          queryClient.invalidateQueries({ queryKey: ['library', library.id] }),
          queryClient.invalidateQueries({ queryKey: ['home'] }),
        )
      } else if (resource.endsWith(':continue-watching')) {
        tasks.push(
          queryClient.invalidateQueries({ queryKey: ['continue-watching'] }),
          queryClient.invalidateQueries({ queryKey: ['home'] }),
        )
      } else if (resource.endsWith(':profile')) {
        tasks.push(
          queryClient.invalidateQueries({ queryKey: ['current-user'] }),
          queryClient.invalidateQueries({ queryKey: ['home'] }),
        )
      } else if (resource === 'admin:users') {
        tasks.push(queryClient.invalidateQueries({ queryKey: ['users'] }))
      } else {
        tasks.push(queryClient.invalidateQueries({ queryKey: ['home'] }))
      }

      await Promise.all(tasks)
    }

    const queueResourceRefresh = (resource: string, revision: number) => {
      if (revision <= (appliedRevisionsRef.current.get(resource) ?? 0)) {
        return
      }
      requestedRevisionsRef.current.set(
        resource,
        Math.max(revision, requestedRevisionsRef.current.get(resource) ?? 0),
      )
      if (inFlightResourcesRef.current.has(resource)) {
        return
      }

      inFlightResourcesRef.current.add(resource)
      const run = async () => {
        while (!disposed) {
          const targetRevision = requestedRevisionsRef.current.get(resource) ?? 0
          try {
            await invalidateResource(resource)
            appliedRevisionsRef.current.set(resource, targetRevision)
          } catch {
            return
          }
          if ((requestedRevisionsRef.current.get(resource) ?? 0) <= targetRevision) {
            return
          }
        }
      }

      void run().finally(() => {
        inFlightResourcesRef.current.delete(resource)
      })
    }

    const reconcileState = async () => {
      if (serverEpochRef.current === null) {
        await queryClient.refetchQueries({ queryKey: ['home'], type: 'active' })
      }
      const state = await getRealtimeState()
      if (disposed || state.protocol_version !== 1) {
        return
      }

      setScanRuntimeByLibrary((current) => buildActiveScanRuntime(state, current))
      const homeSnapshot = queryClient.getQueryData<HomeResponse>(['home'])
      if (serverEpochRef.current === null && homeSnapshot?.realtime) {
        serverEpochRef.current = homeSnapshot.realtime.server_epoch
        appliedRevisionsRef.current = new Map(Object.entries(homeSnapshot.realtime.resources))
      }
      const firstSnapshot = serverEpochRef.current === null
      const epochChanged = !firstSnapshot && serverEpochRef.current !== state.server_epoch
      serverEpochRef.current = state.server_epoch

      if (epochChanged) {
        requestedRevisionsRef.current.clear()
        appliedRevisionsRef.current.clear()
        await queryClient.invalidateQueries()
        if (!disposed) {
          appliedRevisionsRef.current = new Map(Object.entries(state.resources))
        }
        return
      }

      for (const [resource, revision] of Object.entries(state.resources)) {
        if (firstSnapshot) {
          appliedRevisionsRef.current.set(resource, revision)
        } else if (epochChanged || revision > (appliedRevisionsRef.current.get(resource) ?? 0)) {
          queueResourceRefresh(resource, revision)
        }
      }
    }

    const handleOpen = () => {
      void reconcileState()
    }

    const handleResourcesChanged = (event: MessageEvent<string>) => {
      const payload = parseResourcesChanged(event.data)
      if (!payload) {
        return
      }
      let shouldReconcileActiveScans = false
      for (const change of payload.changes) {
        queueResourceRefresh(change.resource, change.revision)
        if (libraryResource(change.resource)?.kind === 'scan') {
          shouldReconcileActiveScans = true
        }
      }
      if (shouldReconcileActiveScans) {
        void reconcileState()
      }
    }

    const applyScanProgressPayload = (payload: ScanProgressPayload) => {
      setScanRuntimeByLibrary((current) => applyScanPayload(current, payload))
      queryClient.setQueryData(['library', payload.scan_job.library_id], (current: unknown) =>
        isRecord(current) ? { ...current, last_scan: payload.scan_job } : current,
      )
    }

    const handleScanProgress = (event: MessageEvent<string>) => {
      const payload = parseScanProgress(event.data)
      if (!payload) {
        return
      }
      applyScanProgressPayload(payload)
    }

    const handleScanFinished = (event: MessageEvent<string>) => {
      const payload = parseScanProgress(event.data)
      if (!payload) {
        return
      }
      applyScanProgressPayload(payload)

      const clearFinishedRuntime = () => {
        if (disposed) return
        setScanRuntimeByLibrary((current) => {
          if (current[payload.scan_job.library_id]?.scanJob?.id !== payload.scan_job.id) {
            return current
          }
          const next = { ...current }
          delete next[payload.scan_job.library_id]
          return next
        })
      }

      void invalidateResource(`library:${payload.scan_job.library_id}:catalog`)
        .then(clearFinishedRuntime)
        .catch(() => {
          void reconcileState()
        })
    }

    const handleResyncRequired = () => {
      void reconcileState()
    }

    const handleSessionInvalidated = () => {
      eventSource.close()
      void queryClient.invalidateQueries({ queryKey: ['current-user'] }).finally(() => {
        navigate('/login', { replace: true })
      })
    }

    eventSource.addEventListener('open', handleOpen as EventListener)
    eventSource.addEventListener('resources.changed', handleResourcesChanged as EventListener)
    eventSource.addEventListener('scan.progress', handleScanProgress as EventListener)
    eventSource.addEventListener('scan.finished', handleScanFinished as EventListener)
    eventSource.addEventListener('resync.required', handleResyncRequired as EventListener)
    eventSource.addEventListener('session.invalidated', handleSessionInvalidated as EventListener)

    return () => {
      disposed = true
      eventSource.close()
      eventSource.removeEventListener('open', handleOpen as EventListener)
      eventSource.removeEventListener('resources.changed', handleResourcesChanged as EventListener)
      eventSource.removeEventListener('scan.progress', handleScanProgress as EventListener)
      eventSource.removeEventListener('scan.finished', handleScanFinished as EventListener)
      eventSource.removeEventListener('resync.required', handleResyncRequired as EventListener)
      eventSource.removeEventListener(
        'session.invalidated',
        handleSessionInvalidated as EventListener,
      )
    }
  }, [enabled, navigate, queryClient])

  return scanRuntimeByLibrary
}
