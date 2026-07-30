import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { type ReactNode, useEffect, useRef, useState } from 'react'
import { listNotifications, markAllNotificationsRead, markNotificationRead } from '../../api/client'
import type {
  CacheCleanupFailureNotificationPayload,
  NotificationItem,
  ScanNotificationIssue,
  ScanNotificationPayload,
} from '../../api/types'
import { type Translate, useI18n } from '../../i18n'
import { localizeApiError } from '../../lib/api-error'
import {
  isScanSummaryAvailable,
  type NotificationResult,
  resolveNotificationResult,
} from '../../lib/notification-presentation'
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
  typeof value.status === 'string' &&
  (value.summary_available === undefined || typeof value.summary_available === 'boolean') &&
  typeof value.total_files === 'number' &&
  typeof value.issue_count === 'number' &&
  Array.isArray(value.issues)

const isCacheCleanupFailurePayload = (
  value: unknown,
): value is CacheCleanupFailureNotificationPayload =>
  isRecord(value) &&
  typeof value.background_job_id === 'number' &&
  typeof value.library_id === 'number' &&
  typeof value.library_name === 'string' &&
  typeof value.attempt_count === 'number' &&
  typeof value.max_attempts === 'number' &&
  typeof value.reason_code === 'string'

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
    case 'scan.cancelled':
      return l('Library scan cancelled')
    case 'cache.cleanup.failed':
      return l('Library cache cleanup failed')
    default:
      return l('New notification')
  }
}

const getNotificationTypeLabel = (category: string, l: Translate) => {
  switch (category) {
    case 'scan':
      return l('Scan')
    case 'system':
      return l('System')
    case 'library':
      return l('Library')
    case 'account':
      return l('Account')
    default:
      return category
  }
}

const getNotificationResultLabel = (result: NotificationResult, l: Translate) => {
  switch (result) {
    case 'completed':
      return l('Completed')
    case 'completed_with_issues':
      return l('Completed with issues')
    case 'failed':
      return l('Failed')
    case 'cancelled':
      return l('Cancelled')
    case 'warning':
      return l('Warning')
    case 'information':
      return l('Information')
  }
}

interface NotificationLogRow {
  label: string
  value: ReactNode
  tone?: 'error' | 'warning'
  mono?: boolean
}

const NotificationLog = ({ rows }: { rows: NotificationLogRow[] }) => (
  <dl className="notification-center__log">
    {rows.map((row) => (
      <div className="notification-center__log-row" key={row.label}>
        <dt>{row.label}</dt>
        <dd
          className={row.mono ? 'notification-center__log-value--mono' : undefined}
          data-tone={row.tone}
        >
          {row.value}
        </dd>
      </div>
    ))}
  </dl>
)

const NotificationIssue = ({ item }: { item: ScanNotificationIssue }) => {
  const displayTitle = item.year ? `${item.title} (${item.year})` : item.title
  const hasMetadataIssue = item.metadata_status === 'failed' || item.metadata_status === 'unmatched'

  return (
    <li className="notification-center__issue">
      <strong title={displayTitle}>{displayTitle}</strong>
      {hasMetadataIssue ? (
        <span>{localizeApiError(item.reason_code, item.reason_params)}</span>
      ) : null}
      {hasMetadataIssue && item.diagnostic_message ? <code>{item.diagnostic_message}</code> : null}
      {item.probe_warning_count > 0 ? (
        <span>
          {localizeApiError(
            item.probe_warning_code ?? 'media_probe_warning',
            item.probe_warning_params,
          )}
        </span>
      ) : null}
      {item.probe_warning_diagnostic ? <code>{item.probe_warning_diagnostic}</code> : null}
      {item.probe_warning_file_path ? (
        <code title={item.probe_warning_file_path}>{item.probe_warning_file_path}</code>
      ) : null}
    </li>
  )
}

const ScanNotificationContent = ({
  createdAt,
  notificationType,
  payload,
}: {
  createdAt: string
  notificationType: string
  payload: ScanNotificationPayload
}) => {
  const { formatDateTime, formatNumber, l } = useI18n()
  const result = resolveNotificationResult(notificationType, 'info')
  const canShowSummary =
    (result === 'completed' || result === 'completed_with_issues') &&
    isScanSummaryAvailable(payload)
  const visibleIssues = payload.issues.slice(0, MAX_VISIBLE_ISSUES)
  const hiddenIssueCount = Math.max(0, payload.issue_count - visibleIssues.length)
  const summary = canShowSummary ? (
    <span className="notification-center__summary">
      <span>{l('{{count}} files', { count: formatNumber(payload.total_files) })}</span>
      <span>{l('{{count}} matched', { count: formatNumber(payload.matched_files) })}</span>
      {payload.reused_files > 0 ? (
        <span>{l('{{count}} unchanged', { count: formatNumber(payload.reused_files) })}</span>
      ) : null}
      {payload.unmatched_files > 0 ? (
        <span data-tone="warning">
          {l('{{count}} unmatched', { count: formatNumber(payload.unmatched_files) })}
        </span>
      ) : null}
      {payload.failed_files > 0 ? (
        <span data-tone="error">
          {l('{{count}} failed', { count: formatNumber(payload.failed_files) })}
        </span>
      ) : null}
      {payload.skipped_files > 0 ? (
        <span>{l('{{count}} skipped', { count: formatNumber(payload.skipped_files) })}</span>
      ) : null}
      {payload.probe_warning_count > 0 ? (
        <span data-tone="warning">
          {l('{{count}} local warnings', { count: formatNumber(payload.probe_warning_count) })}
        </span>
      ) : null}
    </span>
  ) : null
  const issues =
    result === 'completed_with_issues' && visibleIssues.length > 0 ? (
      <>
        <ul className="notification-center__issues">
          {visibleIssues.map((item) => (
            <NotificationIssue item={item} key={item.item_key} />
          ))}
        </ul>
        {hiddenIssueCount > 0 ? (
          <p className="notification-center__more">
            {l('{{count}} more issues', { count: formatNumber(hiddenIssueCount) })}
          </p>
        ) : null}
      </>
    ) : null
  const reason = payload.reason_code
    ? localizeApiError(payload.reason_code, payload.reason_params)
    : result === 'failed'
      ? l('The library scan could not be completed.')
      : result === 'cancelled'
        ? l('Scan was cancelled.')
        : null
  const rows: NotificationLogRow[] = [
    { label: l('Type'), value: l('Scan') },
    {
      label: l('Result'),
      value: getNotificationResultLabel(result, l),
      tone:
        result === 'failed'
          ? 'error'
          : result === 'completed_with_issues' || result === 'warning'
            ? 'warning'
            : undefined,
    },
    {
      label: l('Time'),
      value: <time dateTime={createdAt}>{formatDateTime(createdAt)}</time>,
    },
    { label: l('Library'), value: payload.library_name },
  ]

  if (summary) {
    rows.push({ label: l('Summary'), value: summary })
  }
  if (reason) {
    rows.push({
      label: l('Reason'),
      value: reason,
      tone: result === 'failed' ? 'error' : undefined,
    })
  }
  if (payload.diagnostic_message && (result === 'failed' || result === 'cancelled')) {
    rows.push({
      label: l('Info'),
      value: payload.diagnostic_message,
      tone: result === 'failed' ? 'error' : undefined,
      mono: true,
    })
  }
  if (issues) {
    rows.push({ label: l('Issues'), value: issues })
  }

  return <NotificationLog rows={rows} />
}

const CacheCleanupFailureContent = ({
  createdAt,
  payload,
}: {
  createdAt: string
  payload: CacheCleanupFailureNotificationPayload
}) => {
  const { formatDateTime, l } = useI18n()
  const rows: NotificationLogRow[] = [
    { label: l('Type'), value: l('System') },
    { label: l('Result'), value: l('Failed'), tone: 'error' },
    {
      label: l('Time'),
      value: <time dateTime={createdAt}>{formatDateTime(createdAt)}</time>,
    },
    { label: l('Library'), value: payload.library_name },
    {
      label: l('Attempts'),
      value: l('{{current}} / {{maximum}}', {
        current: payload.attempt_count,
        maximum: payload.max_attempts,
      }),
    },
    {
      label: l('Reason'),
      value: localizeApiError(payload.reason_code, payload.reason_params),
      tone: 'error',
    },
  ]

  if (payload.diagnostic_message) {
    rows.push({
      label: l('Info'),
      value: payload.diagnostic_message,
      tone: 'error',
      mono: true,
    })
  }

  return <NotificationLog rows={rows} />
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
  const cacheCleanupFailurePayload = isCacheCleanupFailurePayload(notification.payload)
    ? notification.payload
    : null
  const result = resolveNotificationResult(notification.notification_type, notification.severity)

  return (
    <article
      className="notification-center__report"
      data-read={notification.is_read}
      data-severity={notification.severity}
    >
      <div className="notification-center__report-heading">
        <span className="notification-center__title-row">
          {!notification.is_read ? <i className="notification-center__unread-dot" /> : null}
          <strong>{getNotificationTitle(notification, l)}</strong>
        </span>
        <span className="notification-center__meta">
          {!notification.is_read ? (
            <button onClick={() => onRead(notification.id)} type="button">
              {l('Mark as read')}
            </button>
          ) : null}
        </span>
      </div>
      {notification.category === 'scan' && scanPayload ? (
        <ScanNotificationContent
          createdAt={notification.created_at}
          notificationType={notification.notification_type}
          payload={scanPayload}
        />
      ) : notification.notification_type === 'cache.cleanup.failed' &&
        cacheCleanupFailurePayload ? (
        <CacheCleanupFailureContent
          createdAt={notification.created_at}
          payload={cacheCleanupFailurePayload}
        />
      ) : (
        <NotificationLog
          rows={[
            { label: l('Type'), value: getNotificationTypeLabel(notification.category, l) },
            {
              label: l('Result'),
              value: getNotificationResultLabel(result, l),
              tone:
                result === 'failed'
                  ? 'error'
                  : result === 'warning' || result === 'completed_with_issues'
                    ? 'warning'
                    : undefined,
            },
            {
              label: l('Time'),
              value: (
                <time dateTime={notification.created_at}>
                  {formatDateTime(notification.created_at)}
                </time>
              ),
            },
            { label: l('Target'), value: l('Mova server') },
            { label: l('Info'), value: l('Open for details.') },
          ]}
        />
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
