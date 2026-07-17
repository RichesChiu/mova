import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { UserAccount } from '../../api/types'
import { I18nProvider } from '../../i18n'
import { UserActionsMenu } from '.'

const user: UserAccount = {
  id: 7,
  username: 'viewer01',
  nickname: 'Viewer',
  role: 'viewer',
  is_primary_admin: false,
  is_enabled: true,
  library_ids: [],
  created_at: '2026-07-17T00:00:00Z',
  updated_at: '2026-07-17T00:00:00Z',
}

describe('UserActionsMenu', () => {
  beforeEach(() => {
    window.localStorage.setItem('mova.interfaceLanguage', 'en-US')
  })

  it('groups edit and delete actions behind the three-dot trigger', () => {
    const onDeleteUser = vi.fn()
    const onEditUser = vi.fn()
    render(
      <I18nProvider>
        <UserActionsMenu onDeleteUser={onDeleteUser} onEditUser={onEditUser} user={user} />
      </I18nProvider>,
    )

    expect(screen.queryByRole('menuitem')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Open user actions menu' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'Edit User' }))

    expect(onEditUser).toHaveBeenCalledWith(user)
    expect(onDeleteUser).not.toHaveBeenCalled()
    expect(screen.queryByRole('menuitem')).not.toBeInTheDocument()
  })

  it('forwards delete from the menu', () => {
    const onDeleteUser = vi.fn()
    render(
      <I18nProvider>
        <UserActionsMenu onDeleteUser={onDeleteUser} onEditUser={vi.fn()} user={user} />
      </I18nProvider>,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Open user actions menu' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'Delete User' }))

    expect(onDeleteUser).toHaveBeenCalledWith(user)
  })
})
