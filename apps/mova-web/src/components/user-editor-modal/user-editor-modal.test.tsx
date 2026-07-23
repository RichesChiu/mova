import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Library, UserAccount } from '../../api/types'
import { I18nProvider } from '../../i18n'
import { UserEditorModal } from '.'

const library: Library = {
  id: 3,
  name: 'Movies',
  description: null,
  metadata_language: 'zh-CN',
  root_path: '/media/movies',
  created_at: '2026-07-17T00:00:00Z',
  updated_at: '2026-07-17T00:00:00Z',
}

const user: UserAccount = {
  id: 7,
  username: 'viewer01',
  nickname: 'Viewer',
  role: 'viewer',
  is_primary_admin: false,
  is_enabled: true,
  library_ids: [library.id],
  created_at: '2026-07-17T00:00:00Z',
  updated_at: '2026-07-17T00:00:00Z',
}

const baseProps = {
  currentUserId: 1,
  currentUserIsPrimaryAdmin: true,
  error: null,
  isOpen: true,
  isSubmitting: false,
  libraries: [library],
  onClose: vi.fn(),
  onCreate: vi.fn().mockResolvedValue(undefined),
  onUpdate: vi.fn().mockResolvedValue(undefined),
}

describe('UserEditorModal', () => {
  beforeEach(() => {
    window.localStorage.setItem('mova.interfaceLanguage', 'en-US')
    baseProps.onClose.mockReset()
    baseProps.onCreate.mockReset().mockResolvedValue(undefined)
    baseProps.onUpdate.mockReset().mockResolvedValue(undefined)
  })

  it('keeps the account immutable while editing nickname, role, and library access', async () => {
    render(
      <I18nProvider>
        <UserEditorModal {...baseProps} mode="edit" user={user} />
      </I18nProvider>,
    )

    expect(screen.queryByRole('textbox', { name: 'Account' })).not.toBeInTheDocument()
    expect(screen.getByText(user.username)).toBeInTheDocument()
    expect(screen.getByLabelText('Nickname')).toHaveValue(user.nickname)
    expect(screen.queryByText('Account enabled')).not.toBeInTheDocument()
    expect(screen.getByText('Role')).toBeInTheDocument()
    expect(screen.getByText('Library Access')).toBeInTheDocument()
    expect(screen.getByText('Movies')).toBeInTheDocument()
    expect(screen.queryByText('No description')).not.toBeInTheDocument()

    fireEvent.change(screen.getByLabelText('Nickname'), {
      target: { value: 'Cinema Fan' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save Changes' }))

    await waitFor(() => {
      expect(baseProps.onUpdate).toHaveBeenCalledWith(user.id, {
        nickname: 'Cinema Fan',
        role: 'viewer',
        library_ids: [library.id],
      })
    })
  })

  it('creates an enabled account without nickname or enabled controls', async () => {
    render(
      <I18nProvider>
        <UserEditorModal {...baseProps} mode="create" />
      </I18nProvider>,
    )

    expect(screen.getByLabelText('Account')).toBeInTheDocument()
    expect(screen.getByLabelText('Password')).toBeInTheDocument()
    expect(screen.queryByLabelText('Nickname')).not.toBeInTheDocument()
    expect(screen.queryByText('Account enabled')).not.toBeInTheDocument()
    expect(screen.getByText('Movies')).toBeInTheDocument()
    expect(screen.queryByText('No description')).not.toBeInTheDocument()

    fireEvent.change(screen.getByLabelText('Account'), {
      target: { value: 'new-user' },
    })
    fireEvent.change(screen.getByLabelText('Password'), {
      target: { value: 'strong-password' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Create User' }))

    await waitFor(() => {
      expect(baseProps.onCreate).toHaveBeenCalledWith({
        username: 'new-user',
        password: 'strong-password',
        role: 'viewer',
        is_enabled: true,
        library_ids: [],
      })
    })
  })
})
