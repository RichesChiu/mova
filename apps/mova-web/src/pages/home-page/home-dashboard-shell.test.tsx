import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { UserAccount } from '../../api/types'
import { I18nProvider } from '../../i18n'
import { HomeDashboardShell } from './home-dashboard-shell'

const clientMocks = vi.hoisted(() => ({
  logout: vi.fn(),
}))

vi.mock('../../api/client', () => ({
  logout: clientMocks.logout,
}))

const buildUser = (role: UserAccount['role'], isPrimaryAdmin = false): UserAccount => ({
  id: 1,
  username: 'account',
  nickname: 'Account',
  role,
  is_primary_admin: isPrimaryAdmin,
  is_enabled: true,
  library_ids: [],
  created_at: '2026-07-17T00:00:00Z',
  updated_at: '2026-07-17T00:00:00Z',
})

const renderShell = (currentUser: UserAccount) => {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })

  return render(
    <I18nProvider>
      <QueryClientProvider client={queryClient}>
        <MemoryRouter initialEntries={['/']}>
          <Routes>
            <Route path="/login" element={<p>Login destination</p>} />
            <Route
              path="*"
              element={
                <HomeDashboardShell ariaLabel="Dashboard" currentUser={currentUser}>
                  <p>Dashboard content</p>
                </HomeDashboardShell>
              }
            />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>
    </I18nProvider>,
  )
}

describe('HomeDashboardShell account menu', () => {
  beforeEach(() => {
    clientMocks.logout.mockReset().mockResolvedValue(undefined)
    window.localStorage.clear()
    window.localStorage.setItem('mova.interfaceLanguage', 'en-US')
  })

  it('moves settings out of primary navigation and shows server settings to administrators', () => {
    renderShell(buildUser('admin', true))

    expect(screen.queryByRole('link', { name: 'Settings' })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Open account menu' }))

    expect(screen.getByRole('menuitem', { name: 'Server Settings' })).toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: 'Personal Settings' })).toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: 'Log out' })).toBeInTheDocument()
  })

  it('hides server settings from standard users', () => {
    renderShell(buildUser('viewer'))

    fireEvent.click(screen.getByRole('button', { name: 'Open account menu' }))

    expect(screen.queryByRole('menuitem', { name: 'Server Settings' })).not.toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: 'Personal Settings' })).toBeInTheDocument()
  })

  it('clears the authenticated view and returns to login after logout', async () => {
    renderShell(buildUser('viewer'))

    fireEvent.click(screen.getByRole('button', { name: 'Open account menu' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'Log out' }))

    await waitFor(() => {
      expect(clientMocks.logout).toHaveBeenCalledOnce()
      expect(screen.getByText('Login destination')).toBeInTheDocument()
    })
  })
})
