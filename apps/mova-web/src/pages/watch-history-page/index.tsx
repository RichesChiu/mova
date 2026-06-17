import { useQuery } from '@tanstack/react-query'
import { Link, useOutletContext } from 'react-router-dom'
import { listWatchHistory } from '../../api/client'
import type { MediaItem, WatchHistoryItem } from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import { useI18n, type Translate } from '../../i18n'
import { formatDateTime } from '../../lib/format'
import { mediaItemPrimaryPath } from '../../lib/media-routes'
import { formatLibraryMediaTypeLabel } from '../../lib/media-type-label'
import { DashboardPageHeader } from '../home-page/dashboard-page-header'
import { HomeDashboardShell } from '../home-page/home-dashboard-shell'

const WATCH_HISTORY_LIMIT = 120
const WATCH_HISTORY_SKELETON_KEYS = [
  'history-a',
  'history-b',
  'history-c',
  'history-d',
  'history-e',
  'history-f',
] as const

const progressPercent = (position: number, duration: number | null) => {
  if (!duration || duration <= 0) {
    return 0
  }

  return Math.max(0, Math.min(100, Math.round((position / duration) * 100)))
}

const titleForMediaItem = (item: MediaItem, l: Translate) =>
  item.title.trim() || item.source_title.trim() || l('Untitled')

const dedupeWatchHistoryItems = (items: WatchHistoryItem[]) => {
  const seenMediaItemIds = new Set<number>()

  return items.filter((item) => {
    if (seenMediaItemIds.has(item.media_item.id)) {
      return false
    }

    seenMediaItemIds.add(item.media_item.id)
    return true
  })
}

const WatchHistoryCard = ({ item }: { item: WatchHistoryItem }) => {
  const { l } = useI18n()
  const mediaItem = item.media_item
  const history = item.watch_history
  const title = titleForMediaItem(mediaItem, l)
  const mediaTypeLabel = formatLibraryMediaTypeLabel(mediaItem.media_type, l)
  const metaLabel = mediaItem.year ? `${mediaTypeLabel} · ${mediaItem.year}` : mediaTypeLabel
  const watchedAt = formatDateTime(history.last_watched_at)
  const percent = progressPercent(history.position_seconds, history.duration_seconds)
  const progressLabel = history.is_finished
    ? l('Watched')
    : l('{{count}}% watched', { count: percent })

  return (
    <Link className="watch-history-card" to={mediaItemPrimaryPath(mediaItem)}>
      <span className="watch-history-card__poster">
        {mediaItem.poster_path ? (
          <img alt={`${title} poster`} loading="lazy" src={mediaItem.poster_path} />
        ) : (
          <span className="watch-history-card__placeholder">{mediaTypeLabel}</span>
        )}
        <span className="watch-history-card__badge">{progressLabel}</span>
      </span>

      <span className="watch-history-card__body">
        <strong title={title}>{title}</strong>
        <em>{metaLabel}</em>
        <small title={watchedAt}>{l('Last watched {{value}}', { value: watchedAt })}</small>
      </span>

      {!history.is_finished ? (
        <span aria-hidden="true" className="watch-history-card__progress">
          <span style={{ width: `${percent}%` }} />
        </span>
      ) : null}
    </Link>
  )
}

const WatchHistorySkeleton = () => (
  <div aria-hidden="true" className="watch-history-card watch-history-card--loading">
    <span className="watch-history-card__poster skeleton-shimmer" />
    <span className="watch-history-card__body">
      <span className="watch-history-card__line watch-history-card__line--title skeleton-shimmer" />
      <span className="watch-history-card__line watch-history-card__line--meta skeleton-shimmer" />
      <span className="watch-history-card__line watch-history-card__line--time skeleton-shimmer" />
    </span>
  </div>
)

export const WatchHistoryPage = () => {
  const { formatNumber, l } = useI18n()
  const { currentUser } = useOutletContext<AppShellOutletContext>()
  const watchHistoryQuery = useQuery({
    queryKey: ['watch-history', WATCH_HISTORY_LIMIT],
    queryFn: () => listWatchHistory(WATCH_HISTORY_LIMIT),
  })
  const rawItems = watchHistoryQuery.data ?? []
  const items = dedupeWatchHistoryItems(rawItems)
  const shouldShowSkeleton = watchHistoryQuery.isLoading && items.length === 0

  return (
    <HomeDashboardShell ariaLabel={l('Recently Watched')} currentUser={currentUser}>
      <div className="home-dashboard__content home-dashboard__content--watch-history">
        <DashboardPageHeader>
          <h2>{l('Recently Watched')}</h2>
          {!watchHistoryQuery.isLoading && !watchHistoryQuery.isError ? (
            <span className="home-dashboard-page-header__meta">
              {l('{{count}} items', { count: formatNumber(items.length) })}
            </span>
          ) : null}
        </DashboardPageHeader>

        <section className="catalog-block watch-history-page">
          {watchHistoryQuery.isError ? (
            <p className="callout callout--danger">
              {watchHistoryQuery.error instanceof Error
                ? watchHistoryQuery.error.message
                : l('Failed to load watch history')}
            </p>
          ) : null}

          {shouldShowSkeleton ? (
            <div className="watch-history-page__grid">
              {WATCH_HISTORY_SKELETON_KEYS.map((key) => (
                <WatchHistorySkeleton key={key} />
              ))}
            </div>
          ) : null}

          {!watchHistoryQuery.isLoading && !watchHistoryQuery.isError && items.length === 0 ? (
            <section className="empty-panel watch-history-page__empty">
              <h3>{l('No recently watched media yet.')}</h3>
              <p className="muted">{l('Start watching something and it will appear here.')}</p>
            </section>
          ) : null}

          {items.length > 0 ? (
            <div className="watch-history-page__grid">
              {items.map((item) => (
                <WatchHistoryCard item={item} key={item.watch_history.id} />
              ))}
            </div>
          ) : null}
        </section>
      </div>
    </HomeDashboardShell>
  )
}
