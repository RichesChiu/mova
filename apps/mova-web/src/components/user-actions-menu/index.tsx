import { useState } from 'react'
import type { UserAccount } from '../../api/types'
import { useI18n } from '../../i18n'
import { GlassMenu } from '../glass-menu'
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

  return (
    <GlassMenu
      ariaLabel={l('User actions')}
      isOpen={isOpen}
      onOpenChange={setIsOpen}
      popoverClassName="glass-menu__action-popover user-actions-menu__popover"
      rootClassName="user-actions-menu"
      trigger={(triggerProps) => (
        <button
          {...triggerProps}
          aria-label={l('Open user actions menu')}
          className="glass-menu__more-trigger user-actions-menu__trigger"
          type="button"
        >
          <span />
          <span />
          <span />
        </button>
      )}
    >
      {(closeMenu) => (
        <>
          <button
            className="glass-menu__action user-actions-menu__item"
            onClick={() => {
              closeMenu()
              onEditUser(user)
            }}
            role="menuitem"
            type="button"
          >
            <UserActionIcon name="edit" />
            <span>{l('Edit User')}</span>
          </button>
          <button
            className="glass-menu__action glass-menu__action--danger user-actions-menu__item"
            disabled={isDeleteDisabled}
            onClick={() => {
              closeMenu()
              onDeleteUser(user)
            }}
            role="menuitem"
            type="button"
          >
            <UserActionIcon name="trash" />
            <span>{isDeletePending ? l('Deleting…') : l('Delete User')}</span>
          </button>
        </>
      )}
    </GlassMenu>
  )
}
