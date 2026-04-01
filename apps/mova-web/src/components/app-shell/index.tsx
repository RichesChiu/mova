import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import { Navigate, Outlet, useLocation, useNavigate } from 'react-router-dom'
import { ApiError, getCurrentUser, listLibraries, logout } from '../../api/client'
import type { Library, UserAccount } from '../../api/types'
import { canManageServer } from '../../lib/viewer'
import { ContentHeader } from '../content-header'
import { useServerEvents } from './use-server-events'

export interface AppShellOutletContext {
  libraries: Library[]
  librariesLoading: boolean
  currentUser: UserAccount
}

export const AppShell = () => {
  const location = useLocation()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const contentRef = useRef<HTMLElement | null>(null)
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

  useServerEvents({ enabled: currentUserQuery.isSuccess })

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
          <h3>Loading session…</h3>
          <p className="muted">正在校验当前登录状态。</p>
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
            : 'Failed to load current user'}
        </p>
      </div>
    )
  }

  const currentUser = currentUserQuery.data

  if (!currentUser) {
    return (
      <div className="page-stack">
        <p className="callout callout--danger">Current user is not available.</p>
      </div>
    )
  }

  return (
    <div className="app-shell">
      <main className="content" ref={contentRef}>
        <div className="content-shell">
          <ContentHeader
            canManageServer={canManageServer(currentUser)}
            currentUser={currentUser}
            isSigningOut={logoutMutation.isPending}
            onSignOut={() => logoutMutation.mutate()}
          />

          <div className="content-body">
            {librariesQuery.isError ? (
              <p className="callout callout--danger">
                {librariesQuery.error instanceof Error
                  ? librariesQuery.error.message
                  : 'Failed to load libraries'}
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
                } as AppShellOutletContext
              }
            />
          </div>
        </div>
      </main>
    </div>
  )
}
