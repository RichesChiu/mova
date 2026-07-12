import { useQuery } from '@tanstack/react-query'
import { useOutletContext } from 'react-router-dom'
import { listContinueWatching } from '../../api/client'
import type { ContinueWatchingItem, MediaItem } from '../../api/types'
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

const CONTINUE_PAGE_LIMIT = 20
const CONTINUE_PAGE_SKELETON_KEYS = [
  'continue-a',
  'continue-b',
  'continue-c',
  'continue-d',
  'continue-e',
  'continue-f',
] as const

const progressPercent = (position: number, duration: number | null) => {
  if (!duration || duration <= 0) {
    return 0
  }

  return Math.max(0, Math.min(100, Math.round((position / duration) * 100)))
}

const titleForMediaItem = (item: MediaItem, l: Translate) =>
  item.title.trim() || item.source_title.trim() || l('Untitled')

const ContinuePageCard = ({ item }: { item: ContinueWatchingItem }) => {
  const { l } = useI18n()
  const mediaItem = item.media_item
  const progress = item.playback_progress
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
        id: progress.id,
        metaLabel,
        placeholderLabel: hasEpisodeContext ? `${seasonNumber}-${episodeNumber}` : mediaTypeLabel,
        progressPercent: progressPercent(progress.position_seconds, progress.duration_seconds),
        title,
      }}
    />
  )
}

export const ContinuePage = () => {
  const { formatNumber, l } = useI18n()
  const { currentUser } = useOutletContext<AppShellOutletContext>()
  const continueWatchingQuery = useQuery({
    queryKey: ['continue-watching', CONTINUE_PAGE_LIMIT],
    queryFn: () => listContinueWatching(CONTINUE_PAGE_LIMIT),
  })
  const items = continueWatchingQuery.data ?? []
  const shouldShowSkeleton = continueWatchingQuery.isLoading && items.length === 0

  return (
    <HomeDashboardShell ariaLabel={l('Continue')} currentUser={currentUser}>
      <div className="home-dashboard__content home-dashboard__content--continue">
        <DashboardPageHeader>
          <h2>{l('Continue')}</h2>
          {!continueWatchingQuery.isLoading && !continueWatchingQuery.isError ? (
            <span className="home-dashboard-page-header__meta">
              {l('{{count}} items', { count: formatNumber(items.length) })}
            </span>
          ) : null}
        </DashboardPageHeader>

        <section className="catalog-block continue-page">
          {continueWatchingQuery.isError ? (
            <p className="callout callout--danger">
              {continueWatchingQuery.error instanceof Error
                ? continueWatchingQuery.error.message
                : l('Failed to load continue watching list')}
            </p>
          ) : null}

          {shouldShowSkeleton ? (
            <div className="continue-page__grid">
              {CONTINUE_PAGE_SKELETON_KEYS.map((key) => (
                <ContinueWatchingCardSkeleton key={key} label={l('Movies')} />
              ))}
            </div>
          ) : null}

          {!continueWatchingQuery.isLoading &&
          !continueWatchingQuery.isError &&
          items.length === 0 ? (
            <section className="empty-panel continue-page__empty">
              <h3>{l('Nothing to continue yet.')}</h3>
              <p className="muted">{l('Start watching something and it will appear here.')}</p>
            </section>
          ) : null}

          {items.length > 0 ? (
            <div className="continue-page__grid">
              {items.map((item) => (
                <ContinuePageCard item={item} key={item.playback_progress.id} />
              ))}
            </div>
          ) : null}
        </section>
      </div>
    </HomeDashboardShell>
  )
}
