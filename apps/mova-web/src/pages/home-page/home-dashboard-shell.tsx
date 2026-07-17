import { useMutation, useQueryClient } from '@tanstack/react-query'
import { type ReactNode, useEffect, useRef, useState } from 'react'
import { Link, NavLink, useLocation, useNavigate } from 'react-router-dom'
import { logout } from '../../api/client'
import type { UserAccount } from '../../api/types'
import { useI18n } from '../../i18n'
import { getUserDisplayName, getUserInitial } from '../../lib/user-identity'
import { getUserRolePresentation } from '../../lib/user-role'
import { canManageServer } from '../../lib/viewer'
import { HomeIcon, type HomeIconName } from './home-icons'

const homeNavItems = [
  { icon: 'home', label: 'Home', to: '/' },
  { icon: 'libraries', label: 'Libraries', to: '/libraries' },
  { icon: 'clock', label: 'Continue', to: '/continue' },
  { icon: 'search', label: 'Search', to: '/search' },
] as const satisfies ReadonlyArray<{
  icon: HomeIconName
  label: string
  to: string
}>

const HOME_SIDEBAR_COLLAPSED_STORAGE_KEY = 'mova.home.sidebarCollapsed'

const readStoredSidebarCollapsed = () => {
  if (typeof window === 'undefined') {
    return false
  }

  return window.localStorage.getItem(HOME_SIDEBAR_COLLAPSED_STORAGE_KEY) === 'true'
}

const writeStoredSidebarCollapsed = (isCollapsed: boolean) => {
  if (typeof window === 'undefined') {
    return
  }

  window.localStorage.setItem(HOME_SIDEBAR_COLLAPSED_STORAGE_KEY, String(isCollapsed))
}

const isNavItemActive = (label: string, pathname: string) => {
  if (label === 'Home') {
    return pathname === '/'
  }

  if (label === 'Libraries') {
    return pathname === '/libraries' || pathname.startsWith('/libraries/')
  }

  if (label === 'Search') {
    return pathname === '/search'
  }

  if (label === 'Continue') {
    return pathname === '/continue'
  }

  return false
}

interface HomeDashboardShellProps {
  ariaLabel: string
  autoCollapseSidebar?: boolean
  children: ReactNode
  currentUser: UserAccount
  shellClassName?: string
}

export const HomeDashboardShell = ({
  ariaLabel,
  autoCollapseSidebar = false,
  children,
  currentUser,
  shellClassName,
}: HomeDashboardShellProps) => {
  const { l } = useI18n()
  const location = useLocation()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState(
    () => autoCollapseSidebar || readStoredSidebarCollapsed(),
  )
  const [isAccountMenuOpen, setIsAccountMenuOpen] = useState(false)
  const accountMenuRef = useRef<HTMLDivElement | null>(null)
  const accountMenuTriggerRef = useRef<HTMLButtonElement | null>(null)
  const displayName = getUserDisplayName(currentUser)
  const userInitial = getUserInitial(currentUser)
  const isAdmin = canManageServer(currentUser)
  const isAccountRoute = location.pathname === '/profile' || location.pathname === '/settings'

  const logoutMutation = useMutation({
    mutationFn: logout,
    onSuccess: () => {
      queryClient.removeQueries()
      navigate('/login', { replace: true })
    },
  })

  // biome-ignore lint/correctness/useExhaustiveDependencies: route changes must reapply auto-collapse.
  useEffect(() => {
    if (autoCollapseSidebar) {
      setIsSidebarCollapsed(true)
    }

    setIsAccountMenuOpen(false)
  }, [autoCollapseSidebar, location.pathname])

  useEffect(() => {
    if (!isAccountMenuOpen) {
      return
    }

    const handlePointerDown = (event: MouseEvent) => {
      if (event.target instanceof Node && accountMenuRef.current?.contains(event.target)) {
        return
      }

      setIsAccountMenuOpen(false)
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') {
        return
      }

      setIsAccountMenuOpen(false)
      accountMenuTriggerRef.current?.focus()
    }

    document.addEventListener('mousedown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)

    return () => {
      document.removeEventListener('mousedown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [isAccountMenuOpen])

  const handleSidebarToggle = () => {
    setIsAccountMenuOpen(false)
    setIsSidebarCollapsed((current) => {
      const next = !current

      writeStoredSidebarCollapsed(next)
      return next
    })
  }

  const shellClasses = [
    'home-shell',
    isSidebarCollapsed ? 'home-shell--sidebar-collapsed' : null,
    shellClassName,
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <div className={shellClasses}>
      <aside className="home-sidebar" aria-label={l('Home navigation')}>
        <div className="home-sidebar__top">
          <Link className="home-sidebar__brand" to="/" aria-label={l('Mova home')}>
            <img alt="" src="/mova-logo-web-64.png" />
            <span>MOVA</span>
          </Link>
          <button
            aria-expanded={!isSidebarCollapsed}
            aria-label={isSidebarCollapsed ? l('Expand sidebar') : l('Collapse sidebar')}
            className="home-sidebar__toggle"
            onClick={handleSidebarToggle}
            type="button"
          >
            <HomeIcon name="chevronRight" />
          </button>
        </div>

        <nav className="home-sidebar__nav">
          {homeNavItems.map((item) => {
            return (
              <NavLink
                className={() =>
                  isNavItemActive(item.label, location.pathname)
                    ? 'home-sidebar__nav-item home-sidebar__nav-item--active'
                    : 'home-sidebar__nav-item'
                }
                key={item.label}
                to={item.to}
                title={isSidebarCollapsed ? l(item.label) : undefined}
              >
                <span aria-hidden="true">
                  <HomeIcon name={item.icon} />
                </span>
                <strong>{l(item.label)}</strong>
              </NavLink>
            )
          })}
        </nav>

        <div className="home-sidebar__account-menu" ref={accountMenuRef}>
          <button
            aria-controls="home-sidebar-account-menu"
            aria-expanded={isAccountMenuOpen}
            aria-haspopup="menu"
            aria-label={l('Open account menu')}
            className={
              isAccountRoute
                ? 'home-sidebar__user home-sidebar__user--active'
                : 'home-sidebar__user'
            }
            onClick={() => {
              logoutMutation.reset()
              setIsAccountMenuOpen((current) => !current)
            }}
            ref={accountMenuTriggerRef}
            title={isSidebarCollapsed ? displayName : undefined}
            type="button"
          >
            <span className="home-sidebar__avatar" aria-hidden="true">
              {userInitial}
            </span>
            <span className="home-sidebar__user-copy">
              <strong>{displayName}</strong>
              <em>{l(getUserRolePresentation(currentUser).label)}</em>
            </span>
            <span aria-hidden="true" className="home-sidebar__user-arrow">
              <HomeIcon name="chevronRight" />
            </span>
          </button>

          {isAccountMenuOpen ? (
            <div
              aria-label={l('Account menu')}
              className="home-sidebar__account-popover glass-popover-surface floating-transition"
              data-state="open"
              id="home-sidebar-account-menu"
              role="menu"
            >
              {isAdmin ? (
                <Link
                  className={
                    location.pathname === '/settings'
                      ? 'home-sidebar__account-action home-sidebar__account-action--active'
                      : 'home-sidebar__account-action'
                  }
                  onClick={() => setIsAccountMenuOpen(false)}
                  role="menuitem"
                  to="/settings"
                >
                  <HomeIcon name="settings" />
                  <span>{l('Server Settings')}</span>
                </Link>
              ) : null}

              <Link
                className={
                  location.pathname === '/profile'
                    ? 'home-sidebar__account-action home-sidebar__account-action--active'
                    : 'home-sidebar__account-action'
                }
                onClick={() => setIsAccountMenuOpen(false)}
                role="menuitem"
                to="/profile"
              >
                <HomeIcon name="user" />
                <span>{l('Personal Settings')}</span>
              </Link>

              <button
                className="home-sidebar__account-action home-sidebar__account-action--danger"
                disabled={logoutMutation.isPending}
                onClick={() => logoutMutation.mutate()}
                role="menuitem"
                type="button"
              >
                <HomeIcon name="logout" />
                <span>{logoutMutation.isPending ? l('Logging out…') : l('Log out')}</span>
              </button>

              {logoutMutation.isError ? (
                <p className="home-sidebar__account-error" role="alert">
                  {l('Failed to log out')}
                </p>
              ) : null}
            </div>
          ) : null}
        </div>
      </aside>

      <section className="home-dashboard" aria-label={ariaLabel}>
        {children}
      </section>
    </div>
  )
}
