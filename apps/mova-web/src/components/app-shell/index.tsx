import { useQuery } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { Navigate, Outlet, useLocation } from 'react-router-dom'
import { ApiError, getCurrentUser, listLibraries } from '../../api/client'
import type { Library, UserAccount } from '../../api/types'
import { useI18n } from '../../i18n'
import type { ScanRuntimeByLibrary } from './scan-runtime'
import { useServerEvents } from './use-server-events'

export interface AppShellOutletContext {
  libraries: Library[]
  librariesLoading: boolean
  currentUser: UserAccount
  scanRuntimeByLibrary: ScanRuntimeByLibrary
}

const readDevMockApiEnabled = import.meta.env.DEV
  ? async () => {
      const { isMockApiEnabled } = await import('../../api/mock-control')
      return isMockApiEnabled()
    }
  : async () => false

export const AppShell = () => {
  const { l } = useI18n()
  const location = useLocation()
  const contentRef = useRef<HTMLElement | null>(null)
  const [isDevMockApiEnabled, setIsDevMockApiEnabled] = useState(import.meta.env.DEV)
  const currentUserQuery = useQuery({
    queryKey: ['current-user'],
    queryFn: getCurrentUser,
    retry: false,
  })
  const librariesQuery = useQuery({
    enabled: currentUserQuery.isSuccess,
    queryKey: ['libraries'],
    queryFn: listLibraries,
  })

  const scanRuntimeByLibrary = useServerEvents({
    enabled: currentUserQuery.isSuccess && !isDevMockApiEnabled,
  })
  const isDesktopDashboardRoute =
    location.pathname === '/' ||
    location.pathname === '/libraries' ||
    location.pathname.startsWith('/libraries/')

  useEffect(() => {
    let isActive = true

    void readDevMockApiEnabled().then((mockApiEnabled) => {
      if (isActive) {
        setIsDevMockApiEnabled(mockApiEnabled)
      }
    })

    return () => {
      isActive = false
    }
  }, [location.search])

  useEffect(() => {
    if (!location.pathname) {
      return
    }
    // The shell owns the scroll container, so route changes should reset this container instead of
    // relying on browser-level page scrolling.
    contentRef.current?.scrollTo({ top: 0, behavior: 'auto' })
  }, [location.pathname])

  if (currentUserQuery.isLoading) {
    return (
      <div className="page-stack">
        <section className="empty-panel">
          <h3>{l('Loading session…')}</h3>
          <p className="muted">{l('Validating the current signed-in session.')}</p>
        </section>
      </div>
    )
  }

  if (currentUserQuery.isError) {
    if (currentUserQuery.error instanceof ApiError && currentUserQuery.error.status === 401) {
      return <Navigate replace to="/login" />
    }

    return (
      <div className="page-stack">
        <p className="callout callout--danger">
          {currentUserQuery.error instanceof Error
            ? currentUserQuery.error.message
            : l('Failed to load current user')}
        </p>
      </div>
    )
  }

  const currentUser = currentUserQuery.data

  if (!currentUser) {
    return (
      <div className="page-stack">
        <p className="callout callout--danger">{l('Current user is not available.')}</p>
      </div>
    )
  }

  return (
    <div className="app-shell">
      <main className="content" ref={contentRef}>
        <div className="content-shell">
          <div
            className={
              isDesktopDashboardRoute ? 'content-body content-body--home' : 'content-body'
            }
          >
            {librariesQuery.isError ? (
              <p className="callout callout--danger">
                {librariesQuery.error instanceof Error
                  ? librariesQuery.error.message
                  : l('Failed to load libraries')}
              </p>
            ) : null}

            {/* Keep shell concerns here and pass data downward; pages should not re-fetch the
                authenticated user or the visible library list on their own. */}
            <Outlet
              context={
                {
                  libraries: librariesQuery.data ?? [],
                  librariesLoading: librariesQuery.isLoading,
                  currentUser,
                  scanRuntimeByLibrary,
                } as AppShellOutletContext
              }
            />
          </div>
        </div>
      </main>
    </div>
  )
}
