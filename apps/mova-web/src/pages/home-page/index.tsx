import { useQueries, useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { Link, NavLink, useOutletContext } from 'react-router-dom'
import {
  getLibrary,
  getMediaItemEpisodeOutline,
  listContinueWatching,
  listRecentlyAddedByLibrary,
} from '../../api/client'
import type { AppShellOutletContext } from '../../components/app-shell'
import { getLibraryScanRuntime } from '../../components/app-shell/scan-runtime'
import { useI18n } from '../../i18n'
import { mediaItemDetailPath, mediaItemPrimaryPath } from '../../lib/media-routes'
import { MEDIA_QUERY_GC_TIME_MS, SERIES_OUTLINE_QUERY_STALE_TIME_MS } from '../../lib/query-options'
import { getUserDisplayName, getUserInitial } from '../../lib/user-identity'
import { canManageServer } from '../../lib/viewer'
import { ContinueWatchingSection } from './continue-watching-section'
import { HomeIcon, type HomeIconName } from './home-icons'
import { LibrariesSection } from './libraries-section'
import { LibraryContentSections } from './library-content-sections'
import type { ContinueWatchingCardData, HomeLibraryModuleData } from './types'

// Playback progress is stored in seconds, but the card UI needs a clamped percentage.
const progressPercent = (position: number, duration: number | null) => {
  if (!duration || duration <= 0) {
    return 0
  }

  return Math.max(0, Math.min(100, Math.round((position / duration) * 100)))
}

const progressStatus = (percent: number) => {
  if (percent >= 100) {
    return 'complete' as const
  }

  if (percent > 0) {
    return 'progress' as const
  }

  return 'idle' as const
}

const isEpisodeContextEntry = (entry: {
  season_number: number | null
  episode_number: number | null
}) => typeof entry.season_number === 'number' && typeof entry.episode_number === 'number'

const homeNavItems = [
  { icon: 'home', label: 'Home', to: '/' },
  { icon: 'libraries', label: 'Libraries', to: '/libraries' },
  { icon: 'clock', label: 'Continue', to: '/' },
  { icon: 'search', label: 'Search', to: '/' },
  { icon: 'settings', label: 'Settings', to: '/settings' },
] as const satisfies ReadonlyArray<{
  icon: HomeIconName
  label: string
  to: string
}>

const HOME_SIDEBAR_COLLAPSED_STORAGE_KEY = 'mova.home.sidebarCollapsed'

const readStoredSidebarCollapsed = () => {
  if (typeof window === 'undefined') {
    return false
  }

  return window.localStorage.getItem(HOME_SIDEBAR_COLLAPSED_STORAGE_KEY) === 'true'
}

export const HomePage = () => {
  const { l } = useI18n()
  const { currentUser, libraries, librariesLoading, scanRuntimeByLibrary } =
    useOutletContext<AppShellOutletContext>()
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState(readStoredSidebarCollapsed)
  const displayName = getUserDisplayName(currentUser)
  const userInitial = getUserInitial(currentUser)
  const isAdmin = canManageServer(currentUser)

  useEffect(() => {
    window.localStorage.setItem(HOME_SIDEBAR_COLLAPSED_STORAGE_KEY, String(isSidebarCollapsed))
  }, [isSidebarCollapsed])

  const continueWatchingQuery = useQuery({
    queryKey: ['continue-watching', 20],
    queryFn: () => listContinueWatching(20),
  })
  const recentlyAddedQuery = useQuery({
    queryKey: ['recently-added-by-library', 3, 8],
    queryFn: () => listRecentlyAddedByLibrary(3, 8),
  })
  const continueWatchingItems = continueWatchingQuery.data ?? []
  const recentlyAddedGroups = recentlyAddedQuery.data ?? []
  const recentlyAddedByLibraryId = new Map(
    recentlyAddedGroups.map((group) => [group.library.id, group.items]),
  )
  const continueWatchingSeriesIds = Array.from(
    new Set(
      continueWatchingItems
        .filter((entry) => entry.media_item.media_type === 'series' && isEpisodeContextEntry(entry))
        .map((entry) => entry.media_item.id),
    ),
  )
  // Continue Watching collapses series into one row, so the home page loads the outline once per
  // series and rehydrates the last watched episode from the current season/episode numbers.
  const continueWatchingOutlineQueries = useQueries({
    queries: continueWatchingSeriesIds.map((seriesId) => ({
      gcTime: MEDIA_QUERY_GC_TIME_MS,
      queryKey: ['home-continue-outline', seriesId],
      queryFn: () => getMediaItemEpisodeOutline(seriesId),
      staleTime: SERIES_OUTLINE_QUERY_STALE_TIME_MS,
    })),
  })
  const continueWatchingOutlineBySeriesId = new Map(
    continueWatchingSeriesIds.map((seriesId, index) => [
      seriesId,
      continueWatchingOutlineQueries[index]?.data ?? null,
    ]),
  )

  const libraryDetailQueries = useQueries({
    queries: libraries.map((library) => ({
      queryKey: ['home-library-detail', library.id],
      queryFn: () => getLibrary(library.id),
    })),
  })
  // Build a page-level view model once and keep the three home modules purely presentational.
  const libraryModules: HomeLibraryModuleData[] = libraries.map((library, index) => ({
    detail: libraryDetailQueries[index]?.data ?? null,
    detailError:
      libraryDetailQueries[index]?.error instanceof Error
        ? libraryDetailQueries[index].error
        : null,
    detailLoading: libraryDetailQueries[index]?.isLoading ?? false,
    library,
    recentItems: recentlyAddedByLibraryId.get(library.id) ?? [],
    scanRuntime: getLibraryScanRuntime(scanRuntimeByLibrary, library.id),
  }))
  const continueWatchingCards: ContinueWatchingCardData[] = continueWatchingItems.map((entry) => {
    const percent = progressPercent(
      entry.playback_progress.position_seconds,
      entry.playback_progress.duration_seconds,
    )
    const seasonNumber = typeof entry.season_number === 'number' ? entry.season_number : null
    const episodeNumber = typeof entry.episode_number === 'number' ? entry.episode_number : null
    const hasEpisodeContext = seasonNumber !== null && episodeNumber !== null
    const localizedEpisode =
      entry.media_item.media_type === 'series' && hasEpisodeContext
        ? continueWatchingOutlineBySeriesId
            .get(entry.media_item.id)
            ?.seasons.find((season) => season.season_number === seasonNumber)
            ?.episodes.find((episode) => episode.episode_number === episodeNumber)
        : null
    const episodeLabel = hasEpisodeContext
      ? `S${String(seasonNumber).padStart(2, '0')} · E${String(episodeNumber).padStart(2, '0')}`
      : null
    const continuePath = hasEpisodeContext
      ? `${mediaItemDetailPath(entry.media_item.id)}?season=${seasonNumber}`
      : mediaItemPrimaryPath(entry.media_item)
    const artwork = hasEpisodeContext
      ? (localizedEpisode?.poster_path ?? entry.episode_poster_path)
      : entry.media_item.poster_path
    const title = hasEpisodeContext
      ? (localizedEpisode?.title ?? entry.episode_title ?? entry.media_item.title)
      : entry.media_item.title
    const description = hasEpisodeContext
      ? (localizedEpisode?.overview ?? entry.episode_overview ?? null)
      : (entry.media_item.overview ?? null)
    const placeholderLabel = hasEpisodeContext ? `${seasonNumber}-${episodeNumber}` : l('Movies')

    return {
      artworkAlt: `${entry.media_item.title} artwork`,
      artworkSrc: artwork,
      description,
      href: continuePath,
      id: entry.playback_progress.id,
      metaLabel: episodeLabel,
      placeholderLabel,
      progressPercent: percent,
      status: progressStatus(percent),
      title,
    }
  })
  const continueWatchingErrorMessage = continueWatchingQuery.isError
    ? continueWatchingQuery.error instanceof Error
      ? continueWatchingQuery.error.message
      : l('Failed to load continue watching list')
    : null
  const recentlyAddedErrorMessage = recentlyAddedQuery.isError
    ? recentlyAddedQuery.error instanceof Error
      ? recentlyAddedQuery.error.message
      : l('Failed to load recently added media')
    : null

  return (
    <div className={isSidebarCollapsed ? 'home-shell home-shell--sidebar-collapsed' : 'home-shell'}>
      <aside className="home-sidebar" aria-label={l('Home navigation')}>
        <div className="home-sidebar__top">
          <Link className="home-sidebar__brand" to="/" aria-label={l('Mova home')}>
            <img alt="" src="/mova-logo-web-64.png" />
            <span>MOVA</span>
          </Link>
          <button
            aria-expanded={!isSidebarCollapsed}
            aria-label={isSidebarCollapsed ? l('Expand sidebar') : l('Collapse sidebar')}
            className="home-sidebar__toggle"
            onClick={() => setIsSidebarCollapsed((current) => !current)}
            type="button"
          >
            <HomeIcon name="chevronRight" />
          </button>
        </div>

        <nav className="home-sidebar__nav">
          {homeNavItems.map((item) => {
            const isDisabledLocalAction =
              (item.label === 'Continue' || item.label === 'Search') && item.to === '/'

            if (item.label === 'Settings' && !isAdmin) {
              return null
            }

            return (
              <NavLink
                aria-disabled={isDisabledLocalAction}
                className={({ isActive }) =>
                  isActive && item.label === 'Home'
                    ? 'home-sidebar__nav-item home-sidebar__nav-item--active'
                    : 'home-sidebar__nav-item'
                }
                key={item.label}
                to={item.to}
                title={isSidebarCollapsed ? l(item.label) : undefined}
              >
                <span aria-hidden="true">
                  <HomeIcon name={item.icon} />
                </span>
                <strong>{l(item.label)}</strong>
              </NavLink>
            )
          })}
        </nav>

        <Link
          className="home-sidebar__user"
          title={isSidebarCollapsed ? displayName : undefined}
          to="/profile"
        >
          <span className="home-sidebar__avatar" aria-hidden="true">
            {userInitial}
          </span>
          <span className="home-sidebar__user-copy">
            <strong>{displayName}</strong>
            <em>{currentUser.role === 'admin' ? l('Administrator') : l('Member')}</em>
          </span>
          <span aria-hidden="true" className="home-sidebar__user-arrow">
            <HomeIcon name="chevronRight" />
          </span>
        </Link>
      </aside>

      <section className="home-dashboard" aria-label={l('Home')}>
        <header className="home-dashboard__topbar">
          <label className="home-search">
            <span aria-hidden="true">
              <HomeIcon name="search" />
            </span>
            <input readOnly placeholder={l('Search media in your libraries…')} />
            <kbd>⌘K</kbd>
          </label>

          <div className="home-dashboard__actions">
            <button className="home-icon-button" type="button" aria-label={l('Notifications')}>
              <HomeIcon name="bell" />
            </button>
            <Link className="home-avatar-button" to="/profile" aria-label={l('Profile')}>
              <span>{userInitial}</span>
            </Link>
            {isAdmin ? <span className="home-admin-pill">{l('Admin')}</span> : null}
          </div>
        </header>

        <div className="home-dashboard__content">
          <ContinueWatchingSection
            errorMessage={continueWatchingErrorMessage}
            isLoading={continueWatchingQuery.isLoading}
            items={continueWatchingCards}
          />

          <LibrariesSection isLoading={librariesLoading} libraryModules={libraryModules} />

          <LibraryContentSections
            errorMessage={recentlyAddedErrorMessage}
            groups={recentlyAddedGroups}
            isLoading={recentlyAddedQuery.isLoading}
          />
        </div>
      </section>
    </div>
  )
}
