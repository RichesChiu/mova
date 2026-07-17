import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { listNotifications, markAllNotificationsRead, markNotificationRead } from '../../api/client'
import type {
  NotificationItem,
  ScanNotificationIssue,
  ScanNotificationPayload,
} from '../../api/types'
import { type Translate, useI18n } from '../../i18n'
import { HomeIcon } from './home-icons'

const MAX_VISIBLE_ISSUES = 5
const categories = ['all', 'scan', 'system', 'library', 'account'] as const
type NotificationCategoryFilter = (typeof categories)[number]

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

const isScanPayload = (value: unknown): value is ScanNotificationPayload =>
  isRecord(value) &&
  typeof value.scan_job_id === 'number' &&
  typeof value.library_name === 'string' &&
  typeof value.total_files === 'number' &&
  typeof value.issue_count === 'number' &&
  Array.isArray(value.issues)

const getCategoryLabel = (category: string, l: Translate) => {
  switch (category) {
    case 'all':
      return l('All')
    case 'scan':
      return l('Scans')
    case 'system':
      return l('System')
    case 'library':
      return l('Libraries')
    case 'account':
      return l('Account')
    default:
      return category
  }
}

const getNotificationTitle = (notification: NotificationItem, l: Translate) => {
  switch (notification.notification_type) {
    case 'scan.completed':
      return l('Library scan completed')
    case 'scan.completed_with_issues':
      return l('Library scan completed with issues')
    case 'scan.failed':
      return l('Library scan failed')
    default:
      return l('New notification')
  }
}

const getMetadataIssueLabel = (item: ScanNotificationIssue, l: Translate) => {
  if (item.metadata_failure_reason === 'metadata_provider_error') {
    return l('Metadata provider request failed')
  }
  if (item.metadata_failure_reason === 'no_remote_match') {
    return l('No exact metadata match')
  }
  if (item.metadata_failure_reason === 'metadata_provider_disabled') {
    return l('Metadata provider is disabled')
  }
  return l('Metadata processing failed')
}

const NotificationIssue = ({ item }: { item: ScanNotificationIssue }) => {
  const { l } = useI18n()
  const displayTitle = item.year ? `${item.title} (${item.year})` : item.title
  const hasMetadataIssue = item.metadata_status === 'failed' || item.metadata_status === 'unmatched'

  return (
    <li className="notification-center__issue">
      <strong title={displayTitle}>{displayTitle}</strong>
      {hasMetadataIssue ? (
        <span>
          {getMetadataIssueLabel(item, l)}
          {item.failure_detail ? ` · ${item.failure_detail}` : ''}
        </span>
      ) : null}
      {item.probe_warning_count > 0 ? (
        <span>
          {l('Media inspection warning')}
          {item.probe_warning_detail ? ` · ${item.probe_warning_detail}` : ''}
        </span>
      ) : null}
      {item.probe_warning_file_path ? (
        <code title={item.probe_warning_file_path}>{item.probe_warning_file_path}</code>
      ) : null}
    </li>
  )
}

const ScanNotificationContent = ({ payload }: { payload: ScanNotificationPayload }) => {
  const { formatNumber, l } = useI18n()
  const visibleIssues = payload.issues.slice(0, MAX_VISIBLE_ISSUES)
  const hiddenIssueCount = Math.max(0, payload.issue_count - visibleIssues.length)

  return (
    <>
      <strong className="notification-center__subject">{payload.library_name}</strong>
      <div className="notification-center__summary">
        <span>{l('{{count}} files', { count: formatNumber(payload.total_files) })}</span>
        <span>{l('{{count}} matched', { count: formatNumber(payload.matched_files) })}</span>
        {payload.reused_files > 0 ? (
          <span>{l('{{count}} unchanged', { count: formatNumber(payload.reused_files) })}</span>
        ) : null}
        {payload.unmatched_files > 0 ? (
          <span className="notification-center__summary--warning">
            {l('{{count}} unmatched', { count: formatNumber(payload.unmatched_files) })}
          </span>
        ) : null}
        {payload.failed_files > 0 ? (
          <span className="notification-center__summary--error">
            {l('{{count}} failed', { count: formatNumber(payload.failed_files) })}
          </span>
        ) : null}
        {payload.probe_warning_count > 0 ? (
          <span className="notification-center__summary--warning">
            {l('{{count}} local warnings', { count: formatNumber(payload.probe_warning_count) })}
          </span>
        ) : null}
      </div>
      {payload.error_message ? (
        <p className="notification-center__job-error">{payload.error_message}</p>
      ) : null}
      {visibleIssues.length > 0 ? (
        <ul className="notification-center__issues">
          {visibleIssues.map((item) => (
            <NotificationIssue item={item} key={item.item_key} />
          ))}
        </ul>
      ) : (
        <p className="notification-center__success">{l('Scan completed without issues.')}</p>
      )}
      {hiddenIssueCount > 0 ? (
        <p className="notification-center__more">
          {l('{{count}} more issues', { count: formatNumber(hiddenIssueCount) })}
        </p>
      ) : null}
    </>
  )
}

const NotificationCard = ({
  notification,
  onRead,
}: {
  notification: NotificationItem
  onRead: (notificationId: number) => void
}) => {
  const { formatDateTime, l } = useI18n()
  const scanPayload = isScanPayload(notification.payload) ? notification.payload : null

  return (
    <article
      className="notification-center__report"
      data-read={notification.is_read}
      data-severity={notification.severity}
    >
      <div className="notification-center__report-heading">
        <span className="notification-center__title-row">
          {!notification.is_read ? <i className="notification-center__unread-dot" /> : null}
          <small>{getCategoryLabel(notification.category, l)}</small>
          <strong>{getNotificationTitle(notification, l)}</strong>
        </span>
        <span className="notification-center__meta">
          <time dateTime={notification.created_at}>{formatDateTime(notification.created_at)}</time>
          {!notification.is_read ? (
            <button onClick={() => onRead(notification.id)} type="button">
              {l('Mark as read')}
            </button>
          ) : null}
        </span>
      </div>
      {notification.category === 'scan' && scanPayload ? (
        <ScanNotificationContent payload={scanPayload} />
      ) : (
        <p className="notification-center__generic-message">{l('Open for details.')}</p>
      )}
    </article>
  )
}

export const NotificationCenter = () => {
  const { formatNumber, l } = useI18n()
  const queryClient = useQueryClient()
  const [isOpen, setIsOpen] = useState(false)
  const [category, setCategory] = useState<NotificationCategoryFilter>('all')
  const containerRef = useRef<HTMLDivElement | null>(null)
  const categoryQuery = category === 'all' ? undefined : category
  const notificationsQuery = useQuery({
    queryKey: ['notifications', category],
    queryFn: () => listNotifications({ category: categoryQuery, limit: 20 }),
  })
  const feed = notificationsQuery.data
  const markReadMutation = useMutation({
    mutationFn: markNotificationRead,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['notifications'] }),
  })
  const markAllMutation = useMutation({
    mutationFn: () => markAllNotificationsRead(categoryQuery),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['notifications'] }),
  })

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const handlePointerDown = (event: MouseEvent) => {
      if (event.target instanceof Node && containerRef.current?.contains(event.target)) {
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
  }, [isOpen])

  const currentUnread =
    category === 'all' ? (feed?.total_unread ?? 0) : (feed?.unread_by_category[category] ?? 0)

  return (
    <div className="notification-center" ref={containerRef}>
      <button
        aria-controls="notification-center-panel"
        aria-expanded={isOpen}
        aria-label={l('Notifications')}
        className="home-icon-button home-dashboard-page-header__notification"
        onClick={() => setIsOpen((current) => !current)}
        type="button"
      >
        <HomeIcon name="bell" />
        {(feed?.total_unread ?? 0) > 0 ? <span className="notification-center__badge" /> : null}
      </button>
      {isOpen ? (
        <section
          aria-label={l('Notifications')}
          className="notification-center__panel glass-popover-surface floating-transition"
          data-state="open"
          id="notification-center-panel"
        >
          <div className="notification-center__panel-heading">
            <span className="notification-center__heading-copy">
              <strong>{l('Notifications')}</strong>
              <small>{l('Updates from Mova')}</small>
            </span>
            <button
              disabled={currentUnread === 0 || markAllMutation.isPending}
              onClick={() => markAllMutation.mutate()}
              type="button"
            >
              {l('Mark all as read')}
            </button>
          </div>
          <nav
            aria-label={l('Notification categories')}
            className="notification-center__categories"
          >
            {categories.map((item) => {
              const unreadCount =
                item === 'all' ? (feed?.total_unread ?? 0) : (feed?.unread_by_category[item] ?? 0)
              return (
                <button
                  aria-pressed={category === item}
                  key={item}
                  onClick={() => setCategory(item)}
                  type="button"
                >
                  {getCategoryLabel(item, l)}
                  {unreadCount > 0 ? <span>{formatNumber(unreadCount)}</span> : null}
                </button>
              )
            })}
          </nav>
          <div className="notification-center__body scrollbar-thin">
            {notificationsQuery.isPending ? <p>{l('Loading notifications…')}</p> : null}
            {notificationsQuery.isError ? <p>{l('Failed to load notifications')}</p> : null}
            {!notificationsQuery.isPending && !notificationsQuery.isError && !feed?.items.length ? (
              <p>{l('No notifications in this category.')}</p>
            ) : null}
            {feed?.items.map((notification) => (
              <NotificationCard
                key={notification.id}
                notification={notification}
                onRead={(notificationId) => markReadMutation.mutate(notificationId)}
              />
            ))}
          </div>
        </section>
      ) : null}
    </div>
  )
}
