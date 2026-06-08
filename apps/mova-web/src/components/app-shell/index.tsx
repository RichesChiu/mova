import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { type UIEvent, useCallback, useEffect, useRef, useState, type WheelEvent } from 'react'
import { Navigate, Outlet, useLocation, useNavigate } from 'react-router-dom'
import { ApiError, getCurrentUser, listLibraries, logout } from '../../api/client'
import type { Library, UserAccount } from '../../api/types'
import { useI18n } from '../../i18n'
import { canManageServer } from '../../lib/viewer'
import { ContentHeader } from '../content-header'
import type { ScanRuntimeByLibrary } from './scan-runtime'
import { useServerEvents } from './use-server-events'

export interface AppShellOutletContext {
  libraries: Library[]
  librariesLoading: boolean
  currentUser: UserAccount
  scanRuntimeByLibrary: ScanRuntimeByLibrary
}

const HEADER_FLOAT_THRESHOLD = 28
const HEADER_SCROLL_DELTA = 2

const readDevMockApiEnabled = import.meta.env.DEV
  ? async () => {
      const { isMockApiEnabled } = await import('../../api/mock-control')
      return isMockApiEnabled()
    }
  : async () => false

export const AppShell = () => {
  const { l } = useI18n()
  const location = useLocation()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const contentRef = useRef<HTMLElement | null>(null)
  const lastScrollTopRef = useRef(0)
  const [isHeaderFloating, setIsHeaderFloating] = useState(false)
  const [isHeaderHidden, setIsHeaderHidden] = useState(false)
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
  const logoutMutation = useMutation({
    mutationFn: logout,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['current-user'] })
      await queryClient.invalidateQueries({ queryKey: ['libraries'] })
      navigate('/login', { replace: true })
    },
  })

  const scanRuntimeByLibrary = useServerEvents({
    enabled: currentUserQuery.isSuccess && !isDevMockApiEnabled,
  })
  const isHomeRoute = location.pathname === '/'

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

  const handleContentScroll = useCallback((event: UIEvent<HTMLElement>) => {
    const contentElement = event.currentTarget
    const nextScrollTop = contentElement.scrollTop
    const delta = nextScrollTop - lastScrollTopRef.current
    const headerHeight =
      contentElement.querySelector<HTMLElement>('.content-header')?.offsetHeight ??
      HEADER_FLOAT_THRESHOLD

    setIsHeaderFloating(nextScrollTop > HEADER_FLOAT_THRESHOLD)

    if (nextScrollTop <= HEADER_FLOAT_THRESHOLD) {
      setIsHeaderHidden(false)
    } else if (delta < 0) {
      setIsHeaderHidden(false)
    } else if (delta > 0 && nextScrollTop > headerHeight) {
      setIsHeaderHidden(true)
    }

    if (Math.abs(delta) >= HEADER_SCROLL_DELTA || nextScrollTop <= HEADER_FLOAT_THRESHOLD) {
      lastScrollTopRef.current = nextScrollTop
    }
  }, [])

  const handleContentWheelCapture = useCallback((event: WheelEvent<HTMLElement>) => {
    const contentElement = event.currentTarget
    const headerHeight =
      contentElement.querySelector<HTMLElement>('.content-header')?.offsetHeight ??
      HEADER_FLOAT_THRESHOLD

    if (event.deltaY > HEADER_SCROLL_DELTA && contentElement.scrollTop > headerHeight) {
      setIsHeaderFloating(true)
      setIsHeaderHidden(true)
    } else if (event.deltaY < -HEADER_SCROLL_DELTA) {
      setIsHeaderHidden(false)
      setIsHeaderFloating(contentElement.scrollTop > HEADER_FLOAT_THRESHOLD)
    }
  }, [])

  useEffect(() => {
    if (!location.pathname) {
      return
    }
    // The shell owns the scroll container, so route changes should reset this container instead of
    // relying on browser-level page scrolling.
    lastScrollTopRef.current = 0
    setIsHeaderFloating(false)
    setIsHeaderHidden(false)
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
      <main
        className="content"
        onScroll={handleContentScroll}
        onWheelCapture={handleContentWheelCapture}
        ref={contentRef}
      >
        <div className="content-shell">
          {!isHomeRoute ? (
            <ContentHeader
              canManageServer={canManageServer(currentUser)}
              currentUser={currentUser}
              isFloating={isHeaderFloating}
              isHidden={isHeaderHidden}
              isSigningOut={logoutMutation.isPending}
              onSignOut={() => logoutMutation.mutate()}
            />
          ) : null}

          <div className={isHomeRoute ? 'content-body content-body--home' : 'content-body'}>
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
