import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { getRealtimeState } from '../../api/client'
import type { RealtimeState, ScanJob } from '../../api/types'
import {
  getRealtimeResourcesQueryKeys,
  parseLibraryRealtimeResource,
  REALTIME_PROTOCOL_VERSION,
} from './realtime-resources'
import type { ScanRuntimeByLibrary, ScanRuntimeItem } from './scan-runtime'

const SERVER_EVENTS_URL = '/api/realtime/events'
const MAX_SCAN_RUNTIME_ITEMS_PER_LIBRARY = 40
const RESOURCE_REFRESH_RETRY_DELAYS_MS = [250, 1_000] as const
const REALTIME_RECONCILE_RETRY_DELAY_MS = 1_000

interface ResourceChange {
  resource: string
  revision: number
}

interface ResourcesChangedPayload {
  protocol_version: number
  changes: ResourceChange[]
}

interface ScanProgressPayload {
  protocol_version: number
  scan_job: ScanJob
  items: ScanRuntimeItem[]
  changes: ResourceChange[]
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
    if (
      !isRecord(value) ||
      value.protocol_version !== REALTIME_PROTOCOL_VERSION ||
      !Array.isArray(value.changes)
    ) {
      return null
    }
    const changes = value.changes.filter(
      (change): change is ResourceChange =>
        isRecord(change) &&
        typeof change.resource === 'string' &&
        typeof change.revision === 'number',
    )
    return { protocol_version: REALTIME_PROTOCOL_VERSION, changes }
  } catch {
    return null
  }
}

const parseScanProgress = (raw: string): ScanProgressPayload | null => {
  try {
    const value: unknown = JSON.parse(raw)
    if (
      !isRecord(value) ||
      value.protocol_version !== REALTIME_PROTOCOL_VERSION ||
      !isScanJob(value.scan_job) ||
      !Array.isArray(value.items)
    ) {
      return null
    }
    return {
      protocol_version: REALTIME_PROTOCOL_VERSION,
      scan_job: value.scan_job,
      items: value.items.filter(isScanRuntimeItem),
      changes: Array.isArray(value.changes)
        ? value.changes.filter(
            (change): change is ResourceChange =>
              isRecord(change) &&
              typeof change.resource === 'string' &&
              typeof change.revision === 'number',
          )
        : [],
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

const mergeActiveScanRuntime = (
  state: RealtimeState,
  current: ScanRuntimeByLibrary,
): ScanRuntimeByLibrary => {
  const next = { ...current }
  for (const scanJob of state.active_scans) {
    const currentRuntime = current[scanJob.library_id]
    next[scanJob.library_id] = {
      scanJob,
      items: currentRuntime?.scanJob?.id === scanJob.id ? currentRuntime.items : [],
    }
  }
  return next
}

export const useServerEvents = ({ enabled }: { enabled: boolean }) => {
  const queryClient = useQueryClient()
  const navigate = useNavigate()
  const serverEpochRef = useRef<string | null>(null)
  const appliedRevisionsRef = useRef(new Map<string, number>())
  const requestedRevisionsRef = useRef(new Map<string, number>())
  const inFlightResourcesRef = useRef(new Set<string>())
  const activeScanLibraryIdsRef = useRef(new Set<number>())
  const [scanRuntimeByLibrary, setScanRuntimeByLibrary] = useState<ScanRuntimeByLibrary>({})

  useEffect(() => {
    if (!enabled || typeof EventSource === 'undefined') {
      setScanRuntimeByLibrary({})
      serverEpochRef.current = null
      appliedRevisionsRef.current.clear()
      requestedRevisionsRef.current.clear()
      inFlightResourcesRef.current.clear()
      activeScanLibraryIdsRef.current.clear()
      return
    }

    let disposed = false
    const eventSource = new EventSource(SERVER_EVENTS_URL)
    const pendingTimers = new Set<number>()
    const resourceRefreshAttempts = new Map<string, number>()
    const resourceWaiters = new Map<string, Array<{ revision: number; resolve: () => void }>>()
    let resourceRefreshScheduled = false
    let reconcileScheduled = false
    let runResourceRefreshBatch: () => Promise<void> = async () => {}
    let reconcileState: () => Promise<void> = async () => {}

    const scheduleTimer = (callback: () => void, delayMs: number) => {
      const timer = window.setTimeout(() => {
        pendingTimers.delete(timer)
        callback()
      }, delayMs)
      pendingTimers.add(timer)
    }

    const invalidateQueryKeys = async (
      queryKeys: ReturnType<typeof getRealtimeResourcesQueryKeys>,
    ) => {
      await Promise.all(
        queryKeys.map((queryKey) =>
          queryClient.invalidateQueries({ queryKey }, { throwOnError: true }),
        ),
      )
    }

    const scheduleReconcile = (delayMs = 0) => {
      if (disposed || reconcileScheduled) {
        return
      }

      reconcileScheduled = true
      scheduleTimer(() => {
        reconcileScheduled = false
        void reconcileState().catch(() => {
          scheduleReconcile(REALTIME_RECONCILE_RETRY_DELAY_MS)
        })
      }, delayMs)
    }

    const scheduleResourceRefresh = (delayMs = 0) => {
      if (disposed || resourceRefreshScheduled) {
        return
      }

      resourceRefreshScheduled = true
      scheduleTimer(() => {
        resourceRefreshScheduled = false
        void runResourceRefreshBatch()
      }, delayMs)
    }

    const resolveResourceWaiters = (resource: string) => {
      const appliedRevision = appliedRevisionsRef.current.get(resource) ?? 0
      const waiters = resourceWaiters.get(resource) ?? []
      const pending = waiters.filter((waiter) => {
        if (waiter.revision <= appliedRevision) {
          waiter.resolve()
          return false
        }
        return true
      })

      if (pending.length > 0) {
        resourceWaiters.set(resource, pending)
      } else {
        resourceWaiters.delete(resource)
      }
    }

    const resolveAllResourceWaiters = () => {
      for (const waiters of resourceWaiters.values()) {
        waiters.forEach((waiter) => {
          waiter.resolve()
        })
      }
      resourceWaiters.clear()
    }

    const queueResourceRefresh = (resource: string, revision: number, schedule = true) => {
      if (revision <= (appliedRevisionsRef.current.get(resource) ?? 0)) {
        return
      }
      requestedRevisionsRef.current.set(
        resource,
        Math.max(revision, requestedRevisionsRef.current.get(resource) ?? 0),
      )
      if (schedule) {
        scheduleResourceRefresh()
      }
    }

    const waitForResourceChanges = (changes: ResourceChange[]) => {
      if (changes.length === 0) {
        return Promise.resolve()
      }

      const waits = changes.map((change) => {
        queueResourceRefresh(change.resource, change.revision)
        if (change.revision <= (appliedRevisionsRef.current.get(change.resource) ?? 0)) {
          return Promise.resolve()
        }

        return new Promise<void>((resolve) => {
          const waiters = resourceWaiters.get(change.resource) ?? []
          waiters.push({ revision: change.revision, resolve })
          resourceWaiters.set(change.resource, waiters)
        })
      })

      return Promise.all(waits).then(() => undefined)
    }

    runResourceRefreshBatch = async () => {
      const resources = [...requestedRevisionsRef.current.entries()]
        .filter(
          ([resource, revision]) =>
            revision > (appliedRevisionsRef.current.get(resource) ?? 0) &&
            !inFlightResourcesRef.current.has(resource),
        )
        .map(([resource]) => resource)

      if (resources.length === 0 || disposed) {
        return
      }

      const targetRevisions = new Map(
        resources.map((resource) => [resource, requestedRevisionsRef.current.get(resource) ?? 0]),
      )
      resources.forEach((resource) => {
        inFlightResourcesRef.current.add(resource)
      })
      let refreshSucceeded = false

      try {
        await invalidateQueryKeys(getRealtimeResourcesQueryKeys(resources))
        if (disposed) {
          return
        }
        for (const [resource, revision] of targetRevisions) {
          appliedRevisionsRef.current.set(resource, revision)
          resourceRefreshAttempts.delete(resource)
          resolveResourceWaiters(resource)
        }

        const finishedLibraryIds = resources.flatMap((resource) => {
          const library = parseLibraryRealtimeResource(resource)
          return library?.kind === 'scan' && !activeScanLibraryIdsRef.current.has(library.id)
            ? [library.id]
            : []
        })
        if (finishedLibraryIds.length > 0) {
          setScanRuntimeByLibrary((current) => {
            const next = { ...current }
            finishedLibraryIds.forEach((libraryId) => {
              delete next[libraryId]
            })
            return next
          })
        }
        refreshSucceeded = true
      } catch {
        const nextAttempt = Math.max(
          ...resources.map((resource) => (resourceRefreshAttempts.get(resource) ?? 0) + 1),
        )
        resources.forEach((resource) => {
          resourceRefreshAttempts.set(resource, nextAttempt)
        })

        const retryDelay = RESOURCE_REFRESH_RETRY_DELAYS_MS[nextAttempt - 1]
        if (retryDelay !== undefined) {
          scheduleResourceRefresh(retryDelay)
        } else {
          resources.forEach((resource) => {
            resourceRefreshAttempts.delete(resource)
          })
          scheduleReconcile(REALTIME_RECONCILE_RETRY_DELAY_MS)
        }
      } finally {
        resources.forEach((resource) => {
          inFlightResourcesRef.current.delete(resource)
        })
      }

      if (
        refreshSucceeded &&
        [...requestedRevisionsRef.current.entries()].some(
          ([resource, revision]) => revision > (appliedRevisionsRef.current.get(resource) ?? 0),
        )
      ) {
        scheduleResourceRefresh()
      }
    }

    reconcileState = async () => {
      const state = await getRealtimeState()
      if (disposed || state.protocol_version !== REALTIME_PROTOCOL_VERSION) {
        return
      }

      const firstSnapshot = serverEpochRef.current === null
      const epochChanged = !firstSnapshot && serverEpochRef.current !== state.server_epoch
      activeScanLibraryIdsRef.current = new Set(
        state.active_scans.map((scanJob) => scanJob.library_id),
      )
      setScanRuntimeByLibrary((current) =>
        firstSnapshot || epochChanged
          ? buildActiveScanRuntime(state, current)
          : mergeActiveScanRuntime(state, current),
      )

      if (firstSnapshot || epochChanged) {
        if (epochChanged) {
          resolveAllResourceWaiters()
          requestedRevisionsRef.current.clear()
        }
        appliedRevisionsRef.current.clear()
        await invalidateQueryKeys(getRealtimeResourcesQueryKeys(Object.keys(state.resources)))
        if (!disposed) {
          serverEpochRef.current = state.server_epoch
          appliedRevisionsRef.current = new Map(Object.entries(state.resources))
          for (const resource of Object.keys(state.resources)) {
            resolveResourceWaiters(resource)
          }
          if (
            [...requestedRevisionsRef.current.entries()].some(
              ([resource, revision]) => revision > (appliedRevisionsRef.current.get(resource) ?? 0),
            )
          ) {
            scheduleResourceRefresh()
          }
          if (epochChanged) {
            scheduleReconcile()
          }
        }
        return
      }

      serverEpochRef.current = state.server_epoch
      for (const [resource, revision] of Object.entries(state.resources)) {
        if (revision > (appliedRevisionsRef.current.get(resource) ?? 0)) {
          queueResourceRefresh(resource, revision)
        }
      }
    }

    const handleOpen = () => {
      scheduleReconcile()
    }

    const handleResourcesChanged = (event: MessageEvent<string>) => {
      const payload = parseResourcesChanged(event.data)
      if (!payload) {
        return
      }
      let shouldReconcileActiveScans = false
      for (const change of payload.changes) {
        if (parseLibraryRealtimeResource(change.resource)?.kind === 'scan') {
          shouldReconcileActiveScans = true
        }
        queueResourceRefresh(change.resource, change.revision, false)
      }
      if (shouldReconcileActiveScans) {
        scheduleReconcile()
      } else {
        scheduleResourceRefresh()
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

      if (payload.changes.length === 0) {
        scheduleReconcile()
        return
      }

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

      void waitForResourceChanges(payload.changes)
        .then(clearFinishedRuntime)
        .catch(() => {
          scheduleReconcile()
        })
    }

    const handleResyncRequired = () => {
      scheduleReconcile()
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
      pendingTimers.forEach((timer) => {
        window.clearTimeout(timer)
      })
      pendingTimers.clear()
      resolveAllResourceWaiters()
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
