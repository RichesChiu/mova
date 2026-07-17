import { useEffect, useState } from 'react'
import type { UserAccount } from '../../api/types'
import { useI18n } from '../../i18n'
import './user-actions-menu.scss'

type UserActionIconName = 'edit' | 'trash'

interface UserActionsMenuProps {
  isDeleteDisabled?: boolean
  isDeletePending?: boolean
  onDeleteUser: (user: UserAccount) => void
  onEditUser: (user: UserAccount) => void
  user: UserAccount
}

const UserActionIcon = ({ name }: { name: UserActionIconName }) => (
  <svg
    aria-hidden="true"
    fill="none"
    focusable="false"
    stroke="currentColor"
    strokeLinecap="round"
    strokeLinejoin="round"
    strokeWidth="1.8"
    viewBox="0 0 24 24"
  >
    {name === 'edit' ? (
      <>
        <path d="M12 20h8" />
        <path d="m16.5 3.5 4 4L8 20l-4.5.5L4 16 16.5 3.5Z" />
      </>
    ) : (
      <>
        <path d="M4 6h16" />
        <path d="M9 6V4h6v2" />
        <path d="m6.5 6 .8 14h9.4l.8-14" />
        <path d="M10 10v6" />
        <path d="M14 10v6" />
      </>
    )}
  </svg>
)

export const UserActionsMenu = ({
  isDeleteDisabled = false,
  isDeletePending = false,
  onDeleteUser,
  onEditUser,
  user,
}: UserActionsMenuProps) => {
  const { l } = useI18n()
  const [isOpen, setIsOpen] = useState(false)

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const handlePointerDown = (event: MouseEvent) => {
      if (
        event.target instanceof Element &&
        event.target.closest(`[data-user-actions-menu="${user.id}"]`)
      ) {
        return
      }

      setIsOpen(false)
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setIsOpen(false)
      }
    }

    document.addEventListener('mousedown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)

    return () => {
      document.removeEventListener('mousedown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [isOpen, user.id])

  return (
    <div
      className="user-actions-menu"
      data-state={isOpen ? 'open' : 'closed'}
      data-user-actions-menu={user.id}
    >
      <button
        aria-expanded={isOpen}
        aria-haspopup="menu"
        aria-label={l('Open user actions menu')}
        className="user-actions-menu__trigger"
        onClick={() => setIsOpen((current) => !current)}
        type="button"
      >
        <span />
        <span />
        <span />
      </button>

      {isOpen ? (
        <div
          aria-label={l('User actions')}
          className="user-actions-menu__popover glass-popover-surface floating-transition"
          data-state="open"
          role="menu"
        >
          <button
            className="user-actions-menu__item"
            onClick={() => {
              setIsOpen(false)
              onEditUser(user)
            }}
            role="menuitem"
            type="button"
          >
            <UserActionIcon name="edit" />
            <span>{l('Edit User')}</span>
          </button>
          <button
            className="user-actions-menu__item user-actions-menu__item--danger"
            disabled={isDeleteDisabled}
            onClick={() => {
              setIsOpen(false)
              onDeleteUser(user)
            }}
            role="menuitem"
            type="button"
          >
            <UserActionIcon name="trash" />
            <span>{isDeletePending ? l('Deleting…') : l('Delete User')}</span>
          </button>
        </div>
      ) : null}
    </div>
  )
}
