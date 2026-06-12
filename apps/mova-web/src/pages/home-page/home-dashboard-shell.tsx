import { type ReactNode, useEffect, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link, NavLink, useLocation } from 'react-router-dom'
import { listContinueWatching } from '../../api/client'
import type { UserAccount } from '../../api/types'
import { useI18n } from '../../i18n'
import { getUserDisplayName, getUserInitial } from '../../lib/user-identity'
import { canManageServer } from '../../lib/viewer'
import { HomeIcon, type HomeIconName } from './home-icons'

const homeNavItems = [
  { icon: 'home', label: 'Home', to: '/' },
  { icon: 'libraries', label: 'Libraries', to: '/libraries' },
  { icon: 'clock', label: 'Continue', to: '/' },
  { icon: 'search', label: 'Search', to: '/' },
  { icon: 'settings', label: 'Settings', to: '/settings' },
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

const isNavItemActive = (label: string, pathname: string) => {
  if (label === 'Home') {
    return pathname === '/'
  }

  if (label === 'Libraries') {
    return pathname === '/libraries' || pathname.startsWith('/libraries/')
  }

  if (label === 'Settings') {
    return pathname === '/settings'
  }

  return false
}

interface HomeDashboardShellProps {
  ariaLabel: string
  children: ReactNode
  currentUser: UserAccount
}

export const HomeDashboardShell = ({
  ariaLabel,
  children,
  currentUser,
}: HomeDashboardShellProps) => {
  const { l } = useI18n()
  const location = useLocation()
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState(readStoredSidebarCollapsed)
  const displayName = getUserDisplayName(currentUser)
  const userInitial = getUserInitial(currentUser)
  const isAdmin = canManageServer(currentUser)
  const continueWatchingNavQuery = useQuery({
    queryKey: ['continue-watching', 'nav', 1],
    queryFn: () => listContinueWatching(1),
  })
  const shouldShowContinueNav =
    continueWatchingNavQuery.isLoading || Boolean(continueWatchingNavQuery.data?.length)

  useEffect(() => {
    window.localStorage.setItem(HOME_SIDEBAR_COLLAPSED_STORAGE_KEY, String(isSidebarCollapsed))
  }, [isSidebarCollapsed])

  return (
    <div className={isSidebarCollapsed ? 'home-shell home-shell--sidebar-collapsed' : 'home-shell'}>
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
            onClick={() => setIsSidebarCollapsed((current) => !current)}
            type="button"
          >
            <HomeIcon name="chevronRight" />
          </button>
        </div>

        <nav className="home-sidebar__nav">
          {homeNavItems.map((item) => {
            const isDisabledLocalAction =
              (item.label === 'Continue' || item.label === 'Search') && item.to === '/'

            if (item.label === 'Settings' && !isAdmin) {
              return null
            }

            if (item.label === 'Continue' && !shouldShowContinueNav) {
              return null
            }

            return (
              <NavLink
                aria-disabled={isDisabledLocalAction}
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

        <Link
          className="home-sidebar__user"
          title={isSidebarCollapsed ? displayName : undefined}
          to="/profile"
        >
          <span className="home-sidebar__avatar" aria-hidden="true">
            {userInitial}
          </span>
          <span className="home-sidebar__user-copy">
            <strong>{displayName}</strong>
            <em>{currentUser.role === 'admin' ? l('Administrator') : l('Member')}</em>
          </span>
          <span aria-hidden="true" className="home-sidebar__user-arrow">
            <HomeIcon name="chevronRight" />
          </span>
        </Link>
      </aside>

      <section className="home-dashboard" aria-label={ariaLabel}>
        <header className="home-dashboard__topbar">
          <label className="home-search">
            <span aria-hidden="true">
              <HomeIcon name="search" />
            </span>
            <input readOnly placeholder={l('Search media in your libraries…')} />
            <kbd>⌘K</kbd>
          </label>

          <div className="home-dashboard__actions">
            <button className="home-icon-button" type="button" aria-label={l('Notifications')}>
              <HomeIcon name="bell" />
            </button>
          </div>
        </header>

        {children}
      </section>
    </div>
  )
}
