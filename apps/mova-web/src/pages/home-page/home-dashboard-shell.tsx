import { type ReactNode, useEffect, useState } from 'react'
import { Link, NavLink, useLocation } from 'react-router-dom'
import type { UserAccount } from '../../api/types'
import { useI18n } from '../../i18n'
import { getUserDisplayName, getUserInitial } from '../../lib/user-identity'
import { canManageServer } from '../../lib/viewer'
import { HomeIcon, type HomeIconName } from './home-icons'

const homeNavItems = [
  { icon: 'home', label: 'Home', to: '/' },
  { icon: 'libraries', label: 'Libraries', to: '/libraries' },
  { icon: 'clock', label: 'Continue', to: '/continue' },
  { icon: 'search', label: 'Search', to: '/search' },
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

  if (label === 'Settings') {
    return pathname === '/settings'
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
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState(
    () => autoCollapseSidebar || readStoredSidebarCollapsed(),
  )
  const displayName = getUserDisplayName(currentUser)
  const userInitial = getUserInitial(currentUser)
  const isAdmin = canManageServer(currentUser)

  // biome-ignore lint/correctness/useExhaustiveDependencies: route changes must reapply auto-collapse.
  useEffect(() => {
    if (autoCollapseSidebar) {
      setIsSidebarCollapsed(true)
    }
  }, [autoCollapseSidebar, location.pathname])

  const handleSidebarToggle = () => {
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
            if (item.label === 'Settings' && !isAdmin) {
              return null
            }

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
        {children}
      </section>
    </div>
  )
}
