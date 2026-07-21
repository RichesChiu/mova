import { useQuery } from '@tanstack/react-query'
import { type CSSProperties, type ReactNode, useEffect, useState } from 'react'
import { Link, Navigate, useOutletContext, useParams, useSearchParams } from 'react-router-dom'
import {
  getMediaItem,
  getMediaItemCast,
  getMediaItemEpisodeOutline,
  getMediaItemPlaybackHeader,
  getMediaItemPlaybackProgress,
  listMediaItemFiles,
} from '../../api/client'
import type { AppShellOutletContext } from '../../components/app-shell'
import {
  getLibraryScanRuntime,
  getMediaItemScanRuntimeItems,
} from '../../components/app-shell/scan-runtime'
import { GlassSelect } from '../../components/glass-select'
import { MetadataMatchPanel } from '../../components/metadata-match-panel'
import { useI18n } from '../../i18n'
import { formatMediaCountry } from '../../lib/media-country'
import {
  buildMediaFileTechnicalBadges,
  buildMediaVersionOptions,
  type MediaFileTechnicalBadge,
} from '../../lib/media-file-details'
import { mediaItemDetailPath, mediaItemPlayPath } from '../../lib/media-routes'
import { buildPlaybackActionLinks, pickSeriesPlaybackTargetEpisode } from '../../lib/playback'
import {
  MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  MEDIA_QUERY_GC_TIME_MS,
  SERIES_OUTLINE_QUERY_STALE_TIME_MS,
} from '../../lib/query-options'
import { canManageLibraries } from '../../lib/viewer'
import { DashboardPageHeader } from '../home-page/dashboard-page-header'
import { HomeDashboardShell } from '../home-page/home-dashboard-shell'
import { HomeIcon } from '../home-page/home-icons'
import { MediaItemCastSection, MediaItemEpisodesSection } from './media-item-sections'
import { MediaItemSourceFilesSection } from './source-files-section'

const GENERATED_EPISODE_STILL_SEGMENT = '/generated/episode-stills/'

const RATING_SOURCE_LABELS: Record<string, string> = {
  imdb: 'IMDb',
  rotten_tomatoes: 'Rotten Tomatoes',
  tmdb: 'TMDB',
}

const formatRatingSource = (source: string) => {
  const normalizedSource = source.trim().toLowerCase()
  return (
    RATING_SOURCE_LABELS[normalizedSource] ??
    normalizedSource
      .split('_')
      .filter(Boolean)
      .map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
      .join(' ')
  )
}

function preferHeroArtwork(path: string | null | undefined): string | null {
  if (!path) {
    return null
  }

  return path.includes(GENERATED_EPISODE_STILL_SEGMENT) ? null : path
}

const renderMediaTechnicalBadge = (badge: MediaFileTechnicalBadge, key: string) => (
  <li
    className={
      badge.iconSrc
        ? 'media-technical-badge media-technical-badge--icon'
        : 'media-technical-badge media-technical-badge--text'
    }
    key={key}
    title={badge.label}
  >
    {badge.iconSrc ? (
      <img alt={badge.label} loading="lazy" src={badge.iconSrc} />
    ) : (
      <span>{badge.label}</span>
    )}
  </li>
)

type HeroFact = {
  label: string
  value: string
}

type PageArtworkState = {
  mediaItemId: number
  image: string
}

const isHeroFact = (item: HeroFact | null): item is HeroFact => item !== null

export const MediaItemPage = () => {
  const { l } = useI18n()
  const { currentUser, scanRuntimeByLibrary } = useOutletContext<AppShellOutletContext>()
  const params = useParams()
  const [searchParams] = useSearchParams()
  const mediaItemId = Number(params.mediaItemId)
  const renderDashboardContent = (
    children: ReactNode,
    ariaLabel = l('Media'),
    headerContent: ReactNode = <h2>{ariaLabel}</h2>,
    routeStyle?: CSSProperties,
  ) => (
    <HomeDashboardShell ariaLabel={ariaLabel} autoCollapseSidebar currentUser={currentUser}>
      <div
        className="home-dashboard__content home-dashboard__content--media-item"
        style={routeStyle}
      >
        <DashboardPageHeader>{headerContent}</DashboardPageHeader>
        {children}
      </div>
    </HomeDashboardShell>
  )
  const [selectedSeasonNumber, setSelectedSeasonNumber] = useState<number | null>(null)
  const [selectedMediaVersionId, setSelectedMediaVersionId] = useState<number | null>(null)
  const [pageArtwork, setPageArtwork] = useState<PageArtworkState | null>(null)
  const requestedSeasonParam = searchParams.get('season')
  const requestedSeasonNumber = requestedSeasonParam ? Number(requestedSeasonParam) : Number.NaN

  const mediaItemQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled: Number.isFinite(mediaItemId),
    queryKey: ['media-item', mediaItemId],
    queryFn: () => getMediaItem(mediaItemId),
    staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  })

  const episodeOutlineQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled: mediaItemQuery.data?.media_type === 'series',
    queryKey: ['media-episode-outline', mediaItemId],
    queryFn: () => getMediaItemEpisodeOutline(mediaItemId),
    staleTime: SERIES_OUTLINE_QUERY_STALE_TIME_MS,
  })
  const moviePlaybackProgressQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled: mediaItemQuery.data?.media_type === 'movie',
    queryKey: ['media-item-playback-progress', mediaItemId],
    queryFn: () => getMediaItemPlaybackProgress(mediaItemId),
    staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  })
  const playbackHeaderQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled: mediaItemQuery.data?.media_type === 'episode',
    queryKey: ['media-item-playback-header', mediaItemId],
    queryFn: () => getMediaItemPlaybackHeader(mediaItemId),
    staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  })

  const seasons = episodeOutlineQuery.data?.seasons ?? []
  const availableSeasons = seasons.filter((season) =>
    season.episodes.some((episode) => episode.is_available),
  )
  const castQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled:
      Number.isFinite(mediaItemId) &&
      !!mediaItemQuery.data &&
      mediaItemQuery.data.media_type !== 'episode',
    queryKey: ['media-item-cast', mediaItemId],
    queryFn: () => getMediaItemCast(mediaItemId),
    staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  })
  const castMembers = castQuery.data ?? []
  const selectedSeason = availableSeasons.find(
    (season) => season.season_number === selectedSeasonNumber,
  )
  const selectedSeasonYear = selectedSeason?.year ?? null
  const selectedSeasonEpisodeCount =
    selectedSeason?.episodes.filter((episode) => episode.is_available).length ?? 0
  const isSeriesView = mediaItemQuery.data?.media_type === 'series'
  const selectedSeasonPlayableEpisodes =
    selectedSeason?.episodes
      .filter((episode) => episode.is_available && episode.media_item_id !== null)
      .map((episode) => ({
        ...episode,
        season_number: selectedSeason.season_number,
      })) ?? []
  // Keep the resource panel aligned with the episode the primary Play button will open.
  const seriesPlaybackTargetEpisode = pickSeriesPlaybackTargetEpisode(
    availableSeasons.flatMap((season) =>
      season.episodes
        .filter((episode) => episode.is_available && episode.media_item_id !== null)
        .map((episode) => ({
          ...episode,
          season_number: season.season_number,
        })),
    ),
    selectedSeasonPlayableEpisodes,
  )
  const shouldShowMediaFilesSection =
    mediaItemQuery.data?.media_type === 'movie' || mediaItemQuery.data?.media_type === 'series'
  const sourceMediaItemId = shouldShowMediaFilesSection
    ? mediaItemQuery.data?.media_type === 'series'
      ? (seriesPlaybackTargetEpisode?.media_item_id ?? null)
      : mediaItemId
    : null
  const mediaFilesQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled: shouldShowMediaFilesSection && Number.isFinite(sourceMediaItemId),
    queryKey: ['media-item-files', sourceMediaItemId],
    queryFn: () => listMediaItemFiles(sourceMediaItemId ?? 0),
    staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  })
  const mediaFiles = mediaFilesQuery.data ?? []
  const selectedMediaFile =
    mediaFiles.find((file) => file.id === selectedMediaVersionId) ?? mediaFiles[0] ?? null
  const selectedTechnicalBadges = selectedMediaFile
    ? buildMediaFileTechnicalBadges(selectedMediaFile)
    : []
  useEffect(() => {
    if (mediaItemQuery.data?.media_type !== 'series' || availableSeasons.length === 0) {
      setSelectedSeasonNumber(null)
      return
    }

    const hasSelected = availableSeasons.some(
      (season) => season.season_number === selectedSeasonNumber,
    )
    if (hasSelected) {
      return
    }

    if (Number.isFinite(requestedSeasonNumber)) {
      const requestedSeason = availableSeasons.find(
        (season) => season.season_number === requestedSeasonNumber,
      )
      if (requestedSeason) {
        setSelectedSeasonNumber(requestedSeason.season_number)
        return
      }
    }

    const firstSeason =
      availableSeasons.find((season) => season.season_number === 1) ?? availableSeasons[0]
    setSelectedSeasonNumber(firstSeason.season_number)
  }, [
    mediaItemQuery.data?.media_type,
    availableSeasons,
    requestedSeasonNumber,
    selectedSeasonNumber,
  ])

  useEffect(() => {
    if (!shouldShowMediaFilesSection || mediaFiles.length === 0) {
      setSelectedMediaVersionId(null)
      return
    }

    const moviePlaybackProgress = moviePlaybackProgressQuery.data
    const preferredMediaFile =
      mediaItemQuery.data?.media_type === 'movie'
        ? ((moviePlaybackProgress &&
            mediaFiles.find((file) => file.id === moviePlaybackProgress.media_file_id)) ??
          mediaFiles[0])
        : mediaFiles[0]

    setSelectedMediaVersionId((current) =>
      current && mediaFiles.some((file) => file.id === current) ? current : preferredMediaFile.id,
    )
  }, [
    mediaFiles,
    mediaItemQuery.data?.media_type,
    moviePlaybackProgressQuery.data,
    shouldShowMediaFilesSection,
  ])

  const canMatchMetadata =
    canManageLibraries(currentUser) && mediaItemQuery.data?.media_type !== 'episode'
  const currentScanRuntime = mediaItemQuery.data
    ? getLibraryScanRuntime(scanRuntimeByLibrary, mediaItemQuery.data.library_id)
    : null
  const detailScanItems = getMediaItemScanRuntimeItems(mediaItemQuery.data, currentScanRuntime, {
    seasonNumber: isSeriesView ? selectedSeasonNumber : null,
  })
  const selectedSeasonScanItems = selectedSeason
    ? detailScanItems.filter(
        (item) =>
          !selectedSeason.episodes.some(
            (episode) =>
              episode.is_available &&
              typeof item.episode_number === 'number' &&
              episode.episode_number === item.episode_number,
          ),
      )
    : []
  const playbackTargetMediaItemId = isSeriesView
    ? (seriesPlaybackTargetEpisode?.media_item_id ?? null)
    : (mediaItemQuery.data?.id ?? null)
  const mediaVersionOptions = buildMediaVersionOptions(mediaFiles)
  const selectedMediaVersionValue =
    selectedMediaFile !== null
      ? String(selectedMediaFile.id)
      : (mediaVersionOptions[0]?.value ?? '')
  const playbackPrimaryPath = playbackTargetMediaItemId
    ? mediaItemPlayPath(playbackTargetMediaItemId, {
        fileId: !isSeriesView ? (selectedMediaFile?.id ?? null) : null,
      })
    : null
  const playbackActionLinks = playbackTargetMediaItemId
    ? buildPlaybackActionLinks(
        playbackTargetMediaItemId,
        isSeriesView
          ? seriesPlaybackTargetEpisode?.playback_progress
          : moviePlaybackProgressQuery.data,
      )
    : null
  const seasonHeroPosterPath = preferHeroArtwork(selectedSeason?.poster_path)
  const heroPosterPath =
    isSeriesView && selectedSeason
      ? seasonHeroPosterPath
      : (mediaItemQuery.data?.poster_path ?? null)
  const heroBackdropPath = preferHeroArtwork(mediaItemQuery.data?.backdrop_path)
  useEffect(() => {
    if (!Number.isFinite(mediaItemId) || !heroBackdropPath) {
      return
    }

    setPageArtwork((current) =>
      current?.mediaItemId === mediaItemId && current.image === heroBackdropPath
        ? current
        : { mediaItemId, image: heroBackdropPath },
    )
  }, [heroBackdropPath, mediaItemId])

  const cachedPageArtworkImage = pageArtwork?.mediaItemId === mediaItemId ? pageArtwork.image : null
  const pageArtworkImage = heroBackdropPath ?? cachedPageArtworkImage
  const pageArtworkStyle = pageArtworkImage
    ? ({
        ['--media-item-page-artwork' as string]: `url(${pageArtworkImage})`,
      } as CSSProperties)
    : undefined
  const heroTitle = mediaItemQuery.data?.title ?? ''
  const heroRatings = (mediaItemQuery.data?.ratings ?? []).filter(
    (rating) =>
      Number.isFinite(rating.score) &&
      Number.isFinite(rating.scale) &&
      rating.scale > 0 &&
      rating.score >= 0 &&
      rating.score <= rating.scale,
  )
  const heroCountry = formatMediaCountry(mediaItemQuery.data?.country)
  const heroGenres = mediaItemQuery.data?.genres?.trim() || null
  const heroStudio = mediaItemQuery.data?.studio?.trim() || null
  const heroOverview =
    isSeriesView && selectedSeason
      ? (selectedSeason.overview ??
        mediaItemQuery.data?.overview ??
        l('No overview available yet.'))
      : (mediaItemQuery.data?.overview ?? l('No overview available yet.'))
  const originalTitle =
    mediaItemQuery.data?.original_title &&
    mediaItemQuery.data.original_title !== mediaItemQuery.data.title
      ? mediaItemQuery.data.original_title
      : null
  const heroYearText = isSeriesView
    ? selectedSeasonYear
      ? String(selectedSeasonYear)
      : mediaItemQuery.data?.year
        ? String(mediaItemQuery.data.year)
        : null
    : mediaItemQuery.data?.year
      ? String(mediaItemQuery.data.year)
      : null
  const heroSecondaryFacts = [
    heroGenres
      ? {
          label: l('Genres'),
          value: heroGenres,
        }
      : null,
    heroStudio
      ? {
          label: l('Studio'),
          value: heroStudio,
        }
      : null,
    heroCountry
      ? {
          label: l('Country'),
          value: heroCountry,
        }
      : null,
  ].filter(isHeroFact)
  const heroAvailabilityText = isSeriesView
    ? selectedSeason
      ? selectedSeasonEpisodeCount === 1
        ? l('1 available episode')
        : l('{{count}} available episodes', { count: selectedSeasonEpisodeCount })
      : availableSeasons.length > 0
        ? availableSeasons.length === 1
          ? l('1 available season')
          : l('{{count}} available seasons', { count: availableSeasons.length })
        : null
    : null
  const sourceContextDescription =
    isSeriesView && seriesPlaybackTargetEpisode
      ? l('Showing resource details for the episode the Play button will open.')
      : null

  if (!Number.isFinite(mediaItemId)) {
    return renderDashboardContent(
      <p className="callout callout--danger">{l('Invalid media item id.')}</p>,
    )
  }

  if (mediaItemQuery.isLoading) {
    return renderDashboardContent(
      <div className="media-item-page">
        <p className="muted">{l('Loading media item…')}</p>
      </div>,
    )
  }

  if (mediaItemQuery.isError) {
    return renderDashboardContent(
      <div className="media-item-page">
        <p className="callout callout--danger">
          {mediaItemQuery.error instanceof Error
            ? mediaItemQuery.error.message
            : l('Failed to load media item')}
        </p>
      </div>,
    )
  }

  if (!mediaItemQuery.data) {
    return null
  }

  if (mediaItemQuery.data.media_type === 'episode') {
    if (playbackHeaderQuery.isLoading) {
      return renderDashboardContent(
        <div className="media-item-page">
          <p className="muted">{l('Loading media item…')}</p>
        </div>,
      )
    }

    if (playbackHeaderQuery.data?.series_media_item_id) {
      return (
        <Navigate replace to={mediaItemDetailPath(playbackHeaderQuery.data.series_media_item_id)} />
      )
    }

    return <Navigate replace to={mediaItemPlayPath(mediaItemQuery.data.id)} />
  }

  return renderDashboardContent(
    <div className="media-item-page">
      <section className="detail-hero">
        <div className="detail-hero__poster-column">
          <div className="detail-hero__poster">
            {heroPosterPath ? (
              <img alt={l('{{title}} poster', { title: heroTitle })} src={heroPosterPath} />
            ) : (
              <div className="artwork-placeholder">
                <span>{l(mediaItemQuery.data.media_type === 'series' ? 'Series' : 'Movie')}</span>
              </div>
            )}
          </div>
        </div>

        <div className="detail-hero__body">
          <div className="detail-hero__info">
            <div className="detail-hero__title-row">
              <h2>{heroTitle}</h2>
              {heroYearText ? <span className="detail-hero__year">{heroYearText}</span> : null}
              {heroAvailabilityText ? (
                <span className="detail-hero__availability detail-hero__availability--title">
                  {heroAvailabilityText}
                </span>
              ) : null}
              {heroRatings.map((rating) => {
                const sourceLabel = formatRatingSource(rating.source)
                const scoreLabel = Number.isInteger(rating.score)
                  ? String(rating.score)
                  : rating.score.toFixed(1)
                const scaleLabel = Number.isInteger(rating.scale)
                  ? String(rating.scale)
                  : rating.scale.toFixed(1)

                return (
                  <span
                    className="detail-hero__rating-badge"
                    key={`${rating.source}:${rating.kind}`}
                    title={l('{{source}} rating {{value}} out of {{scale}}', {
                      source: sourceLabel,
                      value: scoreLabel,
                      scale: scaleLabel,
                    })}
                  >
                    <span className="detail-hero__rating-label">{sourceLabel}</span>
                    <strong>{scoreLabel}</strong>
                  </span>
                )
              })}
            </div>
            {isSeriesView && availableSeasons.length > 0 ? (
              <div className="detail-hero__season-picker">
                <div className="season-picker" role="tablist">
                  {availableSeasons.map((season) => {
                    const isActive = season.season_number === selectedSeasonNumber
                    const label = `S${String(season.season_number).padStart(2, '0')}`

                    return (
                      <button
                        aria-selected={isActive}
                        className={
                          isActive
                            ? 'season-picker__button season-picker__button--active'
                            : 'season-picker__button'
                        }
                        key={season.season_number}
                        onClick={() => setSelectedSeasonNumber(season.season_number)}
                        role="tab"
                        type="button"
                      >
                        <span>{label}</span>
                      </button>
                    )
                  })}
                </div>
              </div>
            ) : null}
            {selectedTechnicalBadges.length > 0 ? (
              <div className="detail-hero__resource-row">
                <ul
                  className="detail-hero__technical-badges media-technical-badges media-technical-badges--hero"
                  aria-label={l('Resource Tags')}
                >
                  {selectedTechnicalBadges.map((badge) =>
                    renderMediaTechnicalBadge(
                      badge,
                      `hero-${selectedMediaFile?.id}-${badge.label}`,
                    ),
                  )}
                </ul>
              </div>
            ) : null}
            {heroSecondaryFacts.length > 0 ? (
              <div className="detail-hero__facts detail-hero__facts--secondary">
                {heroSecondaryFacts.map((item) => (
                  <span className="detail-hero__fact" key={item.label}>
                    <span className="detail-hero__fact-label">{item.label}</span>
                    <span className="detail-hero__fact-value" title={item.value}>
                      {item.value}
                    </span>
                  </span>
                ))}
              </div>
            ) : null}
            <p className="detail-hero__overview">{heroOverview}</p>
          </div>

          <div className="detail-hero__controls">
            {!isSeriesView && mediaVersionOptions.length > 1 ? (
              <div className="detail-hero__version-picker">
                <p className="detail-hero__version-label">{l('Version')}</p>
                <GlassSelect
                  ariaLabel={l('Select playback version for {{title}}', { title: heroTitle })}
                  compact
                  onChange={(value) => setSelectedMediaVersionId(Number(value))}
                  options={mediaVersionOptions}
                  value={selectedMediaVersionValue}
                />
              </div>
            ) : null}
            {playbackTargetMediaItemId || canMatchMetadata ? (
              <div className="detail-hero__actions">
                {playbackTargetMediaItemId ? (
                  <Link
                    className="button button--primary detail-hero__play-button"
                    to={
                      playbackPrimaryPath ??
                      playbackActionLinks?.primaryPath ??
                      mediaItemPlayPath(playbackTargetMediaItemId)
                    }
                  >
                    <HomeIcon className="button__icon" name="play" />
                    <span>{playbackActionLinks?.primaryLabel ?? l('Play')}</span>
                  </Link>
                ) : null}
                {canMatchMetadata ? (
                  <MetadataMatchPanel
                    canOpen={canMatchMetadata}
                    initialQuery={mediaItemQuery.data.source_title}
                    initialYear={mediaItemQuery.data.year}
                    mediaItemId={mediaItemQuery.data.id}
                    mediaType={mediaItemQuery.data.media_type}
                  />
                ) : null}
              </div>
            ) : null}
          </div>
        </div>
      </section>

      {isSeriesView ? (
        <MediaItemEpisodesSection
          availableSeasons={availableSeasons}
          error={episodeOutlineQuery.error}
          isLoading={episodeOutlineQuery.isLoading}
          scanItems={selectedSeasonScanItems}
          selectedSeason={selectedSeason}
        />
      ) : null}

      <MediaItemCastSection
        error={castQuery.error}
        isLoading={castQuery.isLoading || castQuery.isFetching}
        members={castMembers}
      />

      {shouldShowMediaFilesSection ? (
        <MediaItemSourceFilesSection
          error={mediaFilesQuery.error}
          isLoading={mediaFilesQuery.isLoading}
          isSeriesView={isSeriesView}
          key={selectedMediaFile?.id ?? 'no-source'}
          mediaFiles={mediaFiles}
          originalTitle={originalTitle}
          selectedMediaFile={selectedMediaFile}
          sourceContextDescription={sourceContextDescription}
        />
      ) : null}
    </div>,
    heroTitle || l('Media'),
    <Link
      aria-label={l('Back to Library')}
      className="home-dashboard-page-header__back"
      to={`/libraries/${mediaItemQuery.data.library_id}`}
    >
      <HomeIcon name="arrowLeft" />
    </Link>,
    pageArtworkStyle,
  )
}
