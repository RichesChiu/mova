import { useQuery } from '@tanstack/react-query'
import { useOutletContext } from 'react-router-dom'
import { listWatchHistory } from '../../api/client'
import type { MediaItem, WatchHistoryItem } from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import {
  ContinueWatchingCard,
  ContinueWatchingCardSkeleton,
} from '../../components/continue-watching-card'
import { type Translate, useI18n } from '../../i18n'
import { mediaItemDetailPath, mediaItemPrimaryPath } from '../../lib/media-routes'
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
  const seasonNumber = typeof item.season_number === 'number' ? item.season_number : null
  const episodeNumber = typeof item.episode_number === 'number' ? item.episode_number : null
  const hasEpisodeContext = seasonNumber !== null && episodeNumber !== null
  const title = item.episode_title?.trim() || titleForMediaItem(mediaItem, l)
  const mediaTypeLabel = formatLibraryMediaTypeLabel(mediaItem.media_type, l)
  const metaLabel = hasEpisodeContext
    ? `S${String(seasonNumber).padStart(2, '0')} · E${String(episodeNumber).padStart(2, '0')}`
    : mediaItem.year
      ? `${mediaTypeLabel} · ${mediaItem.year}`
      : mediaTypeLabel
  const percent = history.is_finished
    ? 100
    : progressPercent(history.position_seconds, history.duration_seconds)
  const href = hasEpisodeContext
    ? `${mediaItemDetailPath(mediaItem.id)}?season=${seasonNumber}`
    : mediaItemPrimaryPath(mediaItem)

  return (
    <ContinueWatchingCard
      item={{
        artworkAlt: l('{{title}} poster', { title }),
        artworkSrc: hasEpisodeContext
          ? (item.episode_poster_path ?? mediaItem.poster_path)
          : mediaItem.poster_path,
        href,
        id: history.id,
        metaLabel,
        placeholderLabel: hasEpisodeContext ? `${seasonNumber}-${episodeNumber}` : mediaTypeLabel,
        progressPercent: percent,
        title,
      }}
    />
  )
}

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
                <ContinueWatchingCardSkeleton key={key} label={l('Movies')} />
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
