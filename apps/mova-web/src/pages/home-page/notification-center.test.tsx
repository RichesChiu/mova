import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { NotificationFeed } from '../../api/types'
import { I18nProvider } from '../../i18n'
import { NotificationCenter } from './notification-center'

const clientMocks = vi.hoisted(() => ({
  listNotifications: vi.fn(),
  markAllNotificationsRead: vi.fn(),
  markNotificationRead: vi.fn(),
}))

vi.mock('../../api/client', () => ({
  listNotifications: clientMocks.listNotifications,
  markAllNotificationsRead: clientMocks.markAllNotificationsRead,
  markNotificationRead: clientMocks.markNotificationRead,
}))

const unreadFeed: NotificationFeed = {
  items: [
    {
      id: 92,
      category: 'system',
      notification_type: 'system.notice',
      severity: 'info',
      library_id: null,
      payload: {},
      is_read: false,
      read_at: null,
      created_at: '2026-08-07T12:00:00Z',
    },
  ],
  total_unread: 1,
  unread_by_category: { system: 1 },
}

const emptyFeed: NotificationFeed = {
  items: [],
  total_unread: 0,
  unread_by_category: {},
}

const renderNotificationCenter = () => {
  const queryClient = new QueryClient({
    defaultOptions: {
      mutations: { retry: false },
      queries: { retry: false },
    },
  })

  return render(
    <QueryClientProvider client={queryClient}>
      <I18nProvider>
        <NotificationCenter />
      </I18nProvider>
    </QueryClientProvider>,
  )
}

describe('NotificationCenter', () => {
  beforeEach(() => {
    window.localStorage.setItem('mova.interfaceLanguage', 'en-US')
    clientMocks.listNotifications.mockReset()
    clientMocks.markAllNotificationsRead.mockReset().mockResolvedValue(1)
    clientMocks.markNotificationRead.mockReset().mockResolvedValue(undefined)
  })

  it('refetches the unread feed after marking one notification as read', async () => {
    clientMocks.listNotifications.mockResolvedValueOnce(unreadFeed).mockResolvedValueOnce(emptyFeed)
    renderNotificationCenter()

    fireEvent.click(screen.getByRole('button', { name: 'Notifications' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Mark as read' }))

    await waitFor(() => expect(clientMocks.markNotificationRead).toHaveBeenCalledOnce())
    expect(clientMocks.markNotificationRead.mock.calls[0]?.[0]).toBe(92)
    await waitFor(() => expect(clientMocks.listNotifications).toHaveBeenCalledTimes(2))
    expect(clientMocks.listNotifications).toHaveBeenCalledWith({
      category: undefined,
      limit: 20,
      unreadOnly: true,
    })
    expect(await screen.findByText('No notifications in this category.')).toBeInTheDocument()
  })

  it('refetches the unread feed after marking the current category as read', async () => {
    clientMocks.listNotifications.mockResolvedValueOnce(unreadFeed).mockResolvedValueOnce(emptyFeed)
    renderNotificationCenter()

    fireEvent.click(screen.getByRole('button', { name: 'Notifications' }))
    const markAllButton = await screen.findByRole('button', { name: 'Mark all as read' })
    await waitFor(() => expect(markAllButton).toBeEnabled())
    fireEvent.click(markAllButton)

    await waitFor(() =>
      expect(clientMocks.markAllNotificationsRead).toHaveBeenCalledWith(undefined),
    )
    await waitFor(() => expect(clientMocks.listNotifications).toHaveBeenCalledTimes(2))
    expect(await screen.findByText('No notifications in this category.')).toBeInTheDocument()
  })
})
