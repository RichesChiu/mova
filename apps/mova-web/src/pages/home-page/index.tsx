import { useQueries, useQuery } from '@tanstack/react-query'
import { useOutletContext } from 'react-router-dom'
import {
  getLibrary,
  getMediaItemEpisodeOutline,
  listContinueWatching,
  listLibraryMediaItems,
} from '../../api/client'
import type { AppShellOutletContext } from '../../components/app-shell'
import { mediaItemDetailPath, mediaItemPrimaryPath } from '../../lib/media-routes'
import { MEDIA_QUERY_GC_TIME_MS, SERIES_OUTLINE_QUERY_STALE_TIME_MS } from '../../lib/query-options'
import { ContinueWatchingSection } from './continue-watching-section'
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

export const HomePage = () => {
  const { libraries, librariesLoading } = useOutletContext<AppShellOutletContext>()

  const continueWatchingQuery = useQuery({
    queryKey: ['continue-watching', 20],
    queryFn: () => listContinueWatching(20),
  })
  const continueWatchingItems = continueWatchingQuery.data ?? []
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

  const shelfQueries = useQueries({
    queries: libraries.map((library) => ({
      queryKey: ['home-library-shelf', library.id],
      queryFn: () =>
        listLibraryMediaItems(library.id, {
          page: 1,
          pageSize: 20,
        }),
    })),
  })
  const libraryDetailQueries = useQueries({
    queries: libraries.map((library) => ({
      queryKey: ['home-library-detail', library.id],
      queryFn: () => getLibrary(library.id),
    })),
  })
  // Build a page-level view model once and keep the three home modules purely presentational.
  const libraryModules: HomeLibraryModuleData[] = libraries.map((library, index) => ({
    detail: libraryDetailQueries[index]?.data ?? null,
    library,
    shelfError: shelfQueries[index]?.error instanceof Error ? shelfQueries[index].error : null,
    shelfItems: shelfQueries[index]?.data?.items ?? [],
    shelfLoading: shelfQueries[index]?.isLoading ?? false,
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
    const artwork =
      localizedEpisode?.poster_path ??
      localizedEpisode?.backdrop_path ??
      entry.episode_poster_path ??
      entry.episode_backdrop_path ??
      entry.media_item.poster_path ??
      entry.media_item.backdrop_path
    const title = hasEpisodeContext
      ? (localizedEpisode?.title ?? entry.episode_title ?? entry.media_item.title)
      : entry.media_item.title
    const description = hasEpisodeContext
      ? (localizedEpisode?.overview ?? entry.episode_overview ?? null)
      : (entry.media_item.overview ?? null)
    const placeholderLabel = hasEpisodeContext ? `${seasonNumber}-${episodeNumber}` : 'MOVIE'

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
      : 'Failed to load continue watching list'
    : null

  return (
    <div className="home-shell">
      <LibrariesSection isLoading={librariesLoading} libraryModules={libraryModules} />

      <ContinueWatchingSection
        errorMessage={continueWatchingErrorMessage}
        isLoading={continueWatchingQuery.isLoading}
        items={continueWatchingCards}
      />

      <LibraryContentSections isLoading={librariesLoading} libraryModules={libraryModules} />
    </div>
  )
}
