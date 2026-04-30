import { useEffect, useRef, useState } from 'react'
import { Link, NavLink } from 'react-router-dom'
import type { UserAccount } from '../../api/types'
import { useI18n } from '../../i18n'
import { getUserDisplayName, getUserInitial } from '../../lib/user-identity'
import { SettingsGearIcon } from '../settings-gear-icon'

interface ContentHeaderProps {
  currentUser: UserAccount
  canManageServer: boolean
  isSigningOut: boolean
  onSignOut: () => void
}

export const ContentHeader = ({
  currentUser,
  canManageServer,
  isSigningOut,
  onSignOut,
}: ContentHeaderProps) => {
  const { l } = useI18n()
  const [isUserMenuOpen, setIsUserMenuOpen] = useState(false)
  const userMenuRef = useRef<HTMLFieldSetElement | null>(null)
  const displayName = getUserDisplayName(currentUser)
  const userInitial = getUserInitial(currentUser)

  useEffect(() => {
    if (!isUserMenuOpen) {
      return
    }

    // Keep the header menu lightweight: hover can open it, but outside click and Escape
    // must always close it so the sticky header never leaves floating UI behind.
    const handlePointerDown = (event: MouseEvent) => {
      if (
        userMenuRef.current &&
        event.target instanceof Node &&
        !userMenuRef.current.contains(event.target)
      ) {
        setIsUserMenuOpen(false)
      }
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setIsUserMenuOpen(false)
      }
    }

    document.addEventListener('mousedown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)

    return () => {
      document.removeEventListener('mousedown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [isUserMenuOpen])

  return (
    <header className="content-header">
      <Link
        aria-label={l('Go to home')}
        className="brand-lockup content-header__brand"
        title={l('Mova home')}
        to="/"
      >
        <img alt="Mova logo" className="brand-mark" src="/mova-logo-web-64.png" />
      </Link>

      <div className="content-header__actions">
        <fieldset
          className="toolbar-user"
          onBlur={(event) => {
            if (
              event.relatedTarget instanceof Node &&
              userMenuRef.current?.contains(event.relatedTarget)
            ) {
              return
            }
            setIsUserMenuOpen(false)
          }}
          // Hover keeps the menu fast for desktop use; click state still exists so menu items can
          // close deterministically after navigation or sign-out.
          onMouseEnter={() => setIsUserMenuOpen(true)}
          onMouseLeave={() => setIsUserMenuOpen(false)}
          ref={userMenuRef}
        >
          <button
            aria-expanded={isUserMenuOpen}
            aria-haspopup="menu"
            className="toolbar-user__trigger"
            onClick={() => setIsUserMenuOpen((open) => !open)}
            type="button"
          >
            <div className="toolbar-user__identity">
              <strong>{displayName}</strong>
            </div>
            <span aria-hidden="true" className="toolbar-user__avatar">
              {userInitial}
            </span>
          </button>

          <div
            className={
              isUserMenuOpen
                ? 'toolbar-user__menu toolbar-user__menu--open glass-popover-surface'
                : 'toolbar-user__menu glass-popover-surface'
            }
            role="menu"
          >
            {canManageServer ? (
              <NavLink
                className={({ isActive }) =>
                  isActive
                    ? 'toolbar-user__menu-item toolbar-user__menu-item--active'
                    : 'toolbar-user__menu-item'
                }
                onClick={() => setIsUserMenuOpen(false)}
                role="menuitem"
                to="/settings"
              >
                <SettingsGearIcon className="toolbar-user__menu-icon" />
                <span>{l('Server Settings')}</span>
              </NavLink>
            ) : null}

            <Link
              className="toolbar-user__menu-item"
              onClick={() => setIsUserMenuOpen(false)}
              role="menuitem"
              to="/profile"
            >
              <span className="toolbar-user__menu-icon toolbar-user__menu-icon--text">P</span>
              <span>{l('Personal Settings')}</span>
            </Link>

            <button
              className="toolbar-user__menu-item toolbar-user__menu-item--danger"
              disabled={isSigningOut}
              onClick={() => {
                setIsUserMenuOpen(false)
                onSignOut()
              }}
              role="menuitem"
              type="button"
            >
              <span className="toolbar-user__menu-icon toolbar-user__menu-icon--text">⏻</span>
              <span>{isSigningOut ? l('Signing out…') : l('Sign Out')}</span>
            </button>
          </div>
        </fieldset>
      </div>
    </header>
  )
}
