import { useQuery } from '@tanstack/react-query'
import { type CSSProperties, type ReactNode, useEffect, useState } from 'react'
import { Link, Navigate, useOutletContext, useParams, useSearchParams } from 'react-router-dom'
import {
  getMediaItem,
  getMediaItemCast,
  getMediaItemEpisodeOutline,
  getMediaItemPlaybackHeader,
  getMediaItemPlaybackProgress,
  listMediaFileAudioTracks,
  listMediaFileSubtitles,
  listMediaItemFiles,
} from '../../api/client'
import type { EpisodeOutlineSeason, MediaCastMember } from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import {
  formatScanItemCardSummary,
  formatScanItemMeta,
  getLibraryScanRuntime,
  getMediaItemScanRuntimeItems,
  getScanItemCardProgressPercent,
  type ScanRuntimeItem,
} from '../../components/app-shell/scan-runtime'
import { EpisodeCard, EpisodeCardSkeleton } from '../../components/episode-card'
import { GlassSelect } from '../../components/glass-select'
import { MetadataMatchPanel } from '../../components/metadata-match-panel'
import { ScrollableRail } from '../../components/scrollable-rail'
import { SectionHelp } from '../../components/section-help'
import { translateCurrent, useI18n } from '../../i18n'
import { formatMediaCountry } from '../../lib/media-country'
import {
  buildAudioTrackFacts,
  buildAudioTrackOptions,
  buildMediaFileTechnicalBadges,
  buildMediaVersionOptions,
  buildSubtitleTrackFacts,
  buildSubtitleTrackOptions,
  buildVideoCardFacts,
  getMediaFileDisplayName,
  type MediaFileTechnicalBadge,
} from '../../lib/media-file-details'
import { mediaItemDetailPath, mediaItemPlayPath } from '../../lib/media-routes'
import {
  buildPlaybackActionLinks,
  pickSeriesPlaybackTargetEpisode,
  playbackPercent,
  playbackStatus,
} from '../../lib/playback'
import {
  MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  MEDIA_QUERY_GC_TIME_MS,
  SERIES_OUTLINE_QUERY_STALE_TIME_MS,
} from '../../lib/query-options'
import { canManageLibraries } from '../../lib/viewer'
import { DashboardPageHeader } from '../home-page/dashboard-page-header'
import { HomeDashboardShell } from '../home-page/home-dashboard-shell'
import { HomeIcon } from '../home-page/home-icons'

const EPISODE_SKELETONS = [
  { metaLabel: 'S01 · E01', placeholderLabel: '1-1' },
  { metaLabel: 'S01 · E02', placeholderLabel: '1-2' },
  { metaLabel: 'S01 · E03', placeholderLabel: '1-3' },
  { metaLabel: 'S01 · E04', placeholderLabel: '1-4' },
] as const

const GENERATED_EPISODE_STILL_SEGMENT = '/generated/episode-stills/'

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

const SeasonBlock = ({
  scanItems,
  season,
}: {
  scanItems: ScanRuntimeItem[]
  season: EpisodeOutlineSeason
}) => {
  const { l } = useI18n()
  const entries = [
    ...scanItems.map((item) => ({
      key: `scan-${item.item_key}`,
      order: item.episode_number ?? Number.MAX_SAFE_INTEGER,
      render: () => {
        const metaLabel = formatScanItemMeta(item)

        return (
          <EpisodeCard
            artworkAlt={`${metaLabel} artwork`}
            artworkSrc={item.poster_path}
            description={formatScanItemCardSummary(item)}
            key={`scan-${item.item_key}`}
            metaLabel={metaLabel}
            placeholderLabel={metaLabel}
            progressPercent={getScanItemCardProgressPercent(item)}
            status="progress"
            title={item.title}
          />
        )
      },
    })),
    ...season.episodes.map((episode) => ({
      key: `${season.season_number}-${episode.episode_number}`,
      order: episode.episode_number,
      render: () => {
        const index = `S${String(season.season_number).padStart(2, '0')} · E${String(episode.episode_number).padStart(2, '0')}`
        const artwork = episode.poster_path
        const title =
          episode.title.trim() || l('Episode {{episode}}', { episode: episode.episode_number })
        const progress = playbackPercent(episode.playback_progress)
        const status = playbackStatus(episode.playback_progress)

        if (episode.is_available && episode.media_item_id) {
          return (
            <EpisodeCard
              artworkAlt={`${index} artwork`}
              artworkSrc={artwork}
              description={episode.overview}
              key={`${season.season_number}-${episode.episode_number}`}
              href={mediaItemPlayPath(episode.media_item_id)}
              metaLabel={index}
              placeholderLabel={index}
              progressPercent={progress}
              status={status}
              title={title}
            />
          )
        }

        return (
          <EpisodeCard
            artworkAlt={`${index} artwork`}
            artworkSrc={artwork}
            description={episode.overview}
            key={`${season.season_number}-${episode.episode_number}`}
            metaLabel={index}
            placeholderLabel={index}
            progressPercent={progress}
            status={status}
            title={title}
          />
        )
      },
    })),
  ].sort((left, right) => left.order - right.order)

  return (
    <article className="season-card">
      <ScrollableRail
        hint={l('Use horizontal scrolling or click arrows to move through episodes.')}
        resetKey={season.season_number}
      >
        {entries.map((entry) => entry.render())}
      </ScrollableRail>
    </article>
  )
}

const SeasonBlockSkeleton = () => (
  <article aria-hidden="true" className="season-card">
    <ScrollableRail
      hint={translateCurrent('Use horizontal scrolling or click arrows to move through episodes.')}
      resetKey="loading"
    >
      {EPISODE_SKELETONS.map((episode) => (
        <EpisodeCardSkeleton
          key={episode.metaLabel}
          metaLabel={episode.metaLabel}
          placeholderLabel={episode.placeholderLabel}
        />
      ))}
    </ScrollableRail>
  </article>
)

const castInitials = (member: MediaCastMember) =>
  member.name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? '')
    .join('')

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
  const [selectedAudioTrackId, setSelectedAudioTrackId] = useState<string>('')
  const [selectedSubtitleTrackId, setSelectedSubtitleTrackId] = useState<string>('')
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
  const shouldShowEpisodeSkeleton =
    episodeOutlineQuery.isLoading && !episodeOutlineQuery.isError && availableSeasons.length === 0
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
  const audioTracksQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled: shouldShowMediaFilesSection && selectedMediaFile !== null,
    queryKey: ['media-file-audio-tracks', selectedMediaFile?.id],
    queryFn: () => listMediaFileAudioTracks(selectedMediaFile?.id ?? 0),
    staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  })
  const subtitleTracksQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled: shouldShowMediaFilesSection && selectedMediaFile !== null,
    queryKey: ['media-file-subtitles', selectedMediaFile?.id],
    queryFn: () => listMediaFileSubtitles(selectedMediaFile?.id ?? 0),
    staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  })

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

  useEffect(() => {
    if (!shouldShowMediaFilesSection && selectedMediaVersionId === null) {
      return
    }

    setSelectedAudioTrackId('')
    setSelectedSubtitleTrackId('')
  }, [selectedMediaVersionId, shouldShowMediaFilesSection])

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
  const audioTracks = audioTracksQuery.data ?? []
  const subtitleTracks = subtitleTracksQuery.data ?? []
  const audioTrackOptions = buildAudioTrackOptions(audioTracks)
  const selectedAudioTrackValue = selectedAudioTrackId || audioTrackOptions[0]?.value || ''
  const selectedAudioTrack =
    audioTracks.find((audioTrack) => String(audioTrack.id) === selectedAudioTrackValue) ??
    audioTracks[0] ??
    null
  const subtitleTrackOptions = buildSubtitleTrackOptions(subtitleTracks)
  const selectedSubtitleTrackValue = selectedSubtitleTrackId || subtitleTrackOptions[0]?.value || ''
  const selectedSubtitleTrack =
    subtitleTracks.find((subtitle) => String(subtitle.id) === selectedSubtitleTrackValue) ??
    subtitleTracks[0] ??
    null
  const selectedSubtitleTrackIndex = selectedSubtitleTrack
    ? subtitleTracks.findIndex((subtitle) => subtitle.id === selectedSubtitleTrack.id)
    : -1
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
  const heroImdbRating = mediaItemQuery.data?.imdb_rating?.trim() || null
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
              <img alt={`${heroTitle} poster`} src={heroPosterPath} />
            ) : (
              <div className="media-card__placeholder">
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
              {heroImdbRating ? (
                <span className="detail-hero__rating-badge" title={`IMDb rating ${heroImdbRating}`}>
                  <span className="detail-hero__rating-label">IMDb</span>
                  <strong>{heroImdbRating}</strong>
                </span>
              ) : null}
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
            {heroAvailabilityText || selectedTechnicalBadges.length > 0 ? (
              <div className="detail-hero__resource-row">
                {heroAvailabilityText ? (
                  <p className="detail-hero__availability">{heroAvailabilityText}</p>
                ) : null}
                {selectedTechnicalBadges.length > 0 ? (
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
                ) : null}
              </div>
            ) : null}
            {heroSecondaryFacts.length > 0 ? (
              <div className="detail-hero__facts detail-hero__facts--secondary">
                {heroSecondaryFacts.map((item) => (
                  <article className="detail-hero__fact" key={item.label}>
                    <p className="detail-hero__fact-label">{item.label}</p>
                    <p className="detail-hero__fact-value" title={item.value}>
                      {item.value}
                    </p>
                  </article>
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
        <section className="page-stack">
          <div className="section-heading">
            <h3>{l('Episodes')}</h3>
          </div>

          {selectedSeasonScanItems.length > 0 ? (
            <p className="muted">
              {selectedSeasonScanItems.length === 1
                ? l(
                    '{{count}} episode is still syncing in this season. Placeholder cards stay visible until the library write completes.',
                    { count: selectedSeasonScanItems.length },
                  )
                : l(
                    '{{count}} episodes are still syncing in this season. Placeholder cards stay visible until the library write completes.',
                    { count: selectedSeasonScanItems.length },
                  )}
            </p>
          ) : null}

          {shouldShowEpisodeSkeleton ? (
            <>
              <p className="muted">{l('Loading episodes…')}</p>
              <SeasonBlockSkeleton />
            </>
          ) : null}

          {episodeOutlineQuery.isError ? (
            <p className="callout callout--danger">
              {episodeOutlineQuery.error instanceof Error
                ? episodeOutlineQuery.error.message
                : l('Failed to load episodes')}
            </p>
          ) : null}

          {!shouldShowEpisodeSkeleton && availableSeasons.length > 0 ? (
            selectedSeason ? (
              <SeasonBlock
                key={selectedSeason.season_number}
                scanItems={selectedSeasonScanItems}
                season={selectedSeason}
              />
            ) : null
          ) : !shouldShowEpisodeSkeleton ? (
            <p className="muted">{l('No local episodes available in this series yet.')}</p>
          ) : null}
        </section>
      ) : null}

      {castQuery.isLoading ||
      castQuery.isFetching ||
      castMembers.length > 0 ||
      castQuery.isError ? (
        <section className="page-stack">
          <div className="section-heading">
            <div className="media-item-section-title-row">
              <h3>{l('Cast')}</h3>
            </div>
            {!castQuery.isLoading ? (
              <span className="counter-badge">{castMembers.length}</span>
            ) : null}
          </div>

          <div className="season-card cast-panel">
            {castQuery.isLoading || castQuery.isFetching ? (
              <p className="muted">{l('Loading cast…')}</p>
            ) : castQuery.isError ? (
              <p className="muted">{l('Cast details are unavailable right now.')}</p>
            ) : castMembers.length > 0 ? (
              <ScrollableRail
                hint={l('Use horizontal scrolling or click arrows to move through the cast list.')}
                viewportClassName="cast-panel__viewport"
              >
                {castMembers.map((member) => (
                  <article
                    className="cast-card"
                    key={`${member.person_id ?? member.name}-${member.sort_order}`}
                  >
                    <div className="cast-card__portrait">
                      {member.profile_path ? (
                        <img alt={member.name} loading="lazy" src={member.profile_path} />
                      ) : (
                        <div className="cast-card__placeholder">
                          <span>{castInitials(member) || '??'}</span>
                        </div>
                      )}
                    </div>

                    <div className="cast-card__body">
                      <p className="cast-card__name">{member.name}</p>
                      <p className="cast-card__role">
                        {member.character_name
                          ? l('as {{character}}', { character: member.character_name })
                          : l('Actor')}
                      </p>
                    </div>
                  </article>
                ))}
              </ScrollableRail>
            ) : null}
          </div>
        </section>
      ) : null}

      {shouldShowMediaFilesSection ? (
        <section className="page-stack">
          <div className="section-heading">
            <div className="media-item-section-title-row">
              <h3>{l('Source Files')}</h3>
              {isSeriesView && sourceContextDescription ? (
                <SectionHelp
                  detail={sourceContextDescription}
                  placement="bottom"
                  title={l('Source file selection help')}
                  variant="notice"
                />
              ) : null}
            </div>
            {!mediaFilesQuery.isLoading && !mediaFilesQuery.isError ? (
              <span className="counter-badge">{mediaFiles.length}</span>
            ) : null}
          </div>

          <div className="season-card media-file-panel">
            {mediaFilesQuery.isLoading ? (
              <p className="muted">{l('Loading source details…')}</p>
            ) : null}

            {mediaFilesQuery.isError ? (
              <p className="callout callout--danger">
                {mediaFilesQuery.error instanceof Error
                  ? mediaFilesQuery.error.message
                  : l('Failed to load source details')}
              </p>
            ) : null}

            {!mediaFilesQuery.isLoading && !mediaFilesQuery.isError && mediaFiles.length > 0 ? (
              selectedMediaFile ? (
                <div className="media-file-panel__list">
                  <div className="media-file-detail">
                    {originalTitle ? (
                      <div className="media-file-detail__meta-block">
                        <p className="media-file-detail__label">{l('Original title')}</p>
                        <p className="media-file-detail__meta-value">{originalTitle}</p>
                      </div>
                    ) : null}
                    <div className="media-file-detail__path-block">
                      <p className="media-file-detail__label">{l('Path')}</p>
                      <p className="media-file-detail__path">{selectedMediaFile.file_path}</p>
                    </div>
                  </div>

                  <div className="media-tech-stack">
                    <article className="media-tech-card media-tech-card--video">
                      <div className="media-tech-card__header">
                        <div className="media-tech-card__title-block">
                          <p className="media-tech-card__eyebrow">{l('Video')}</p>
                          <h5>{l('Video Details')}</h5>
                        </div>
                      </div>

                      <dl className="media-tech-card__facts">
                        {buildVideoCardFacts(selectedMediaFile).map((fact) => (
                          <div
                            className="media-tech-card__fact"
                            key={`${selectedMediaFile.id}-video-${fact.label}`}
                          >
                            <dt>{fact.label}</dt>
                            <dd>{fact.value}</dd>
                          </div>
                        ))}
                      </dl>
                    </article>

                    <article className="media-tech-card">
                      <div className="media-tech-card__header media-tech-card__header--with-select">
                        <div className="media-tech-card__title-block">
                          <p className="media-tech-card__eyebrow">{l('Audio')}</p>
                          <h5>{l('Audio Details')}</h5>
                        </div>

                        <div className="media-tech-card__selector">
                          <GlassSelect
                            ariaLabel={l('Select audio track for {{name}}', {
                              name: getMediaFileDisplayName(selectedMediaFile.file_path),
                            })}
                            compact
                            disabled={audioTrackOptions.length === 0}
                            onChange={(value) => setSelectedAudioTrackId(value)}
                            options={
                              audioTrackOptions.length > 0
                                ? audioTrackOptions
                                : [
                                    {
                                      label: l('No audio tracks detected'),
                                      value: `empty-audio-${selectedMediaFile.id}`,
                                    },
                                  ]
                            }
                            value={
                              audioTrackOptions.length > 0
                                ? selectedAudioTrackValue
                                : `empty-audio-${selectedMediaFile.id}`
                            }
                          />
                        </div>
                      </div>

                      {audioTracksQuery.isLoading ? (
                        <p className="muted">{l('Loading audio tracks…')}</p>
                      ) : null}
                      {audioTracksQuery.isError ? (
                        <p className="callout callout--danger">
                          {audioTracksQuery.error instanceof Error
                            ? audioTracksQuery.error.message
                            : l('Failed to load audio tracks')}
                        </p>
                      ) : null}
                      {!audioTracksQuery.isLoading &&
                      !audioTracksQuery.isError &&
                      selectedAudioTrack ? (
                        <dl className="media-tech-card__facts">
                          {buildAudioTrackFacts(selectedAudioTrack).map((fact) => (
                            <div
                              className="media-tech-card__fact"
                              key={`${selectedAudioTrack.id}-${fact.label}`}
                            >
                              <dt>{fact.label}</dt>
                              <dd>{fact.value}</dd>
                            </div>
                          ))}
                        </dl>
                      ) : null}
                      {!audioTracksQuery.isLoading &&
                      !audioTracksQuery.isError &&
                      !selectedAudioTrack ? (
                        <p className="muted">{l('No embedded audio tracks were detected.')}</p>
                      ) : null}
                    </article>

                    <article className="media-tech-card">
                      <div className="media-tech-card__header media-tech-card__header--with-select">
                        <div className="media-tech-card__title-block">
                          <p className="media-tech-card__eyebrow">{l('Subtitle')}</p>
                          <h5>{l('Subtitle Details')}</h5>
                        </div>

                        <div className="media-tech-card__selector">
                          <GlassSelect
                            ariaLabel={l('Select subtitle track for {{name}}', {
                              name: getMediaFileDisplayName(selectedMediaFile.file_path),
                            })}
                            compact
                            disabled={subtitleTrackOptions.length === 0}
                            onChange={(value) => setSelectedSubtitleTrackId(value)}
                            options={
                              subtitleTrackOptions.length > 0
                                ? subtitleTrackOptions
                                : [
                                    {
                                      label: l('No subtitles detected'),
                                      value: `empty-subtitle-${selectedMediaFile.id}`,
                                    },
                                  ]
                            }
                            value={
                              subtitleTrackOptions.length > 0
                                ? selectedSubtitleTrackValue
                                : `empty-subtitle-${selectedMediaFile.id}`
                            }
                          />
                        </div>
                      </div>

                      {subtitleTracksQuery.isLoading ? (
                        <p className="muted">{l('Loading subtitles…')}</p>
                      ) : null}
                      {subtitleTracksQuery.isError ? (
                        <p className="callout callout--danger">
                          {subtitleTracksQuery.error instanceof Error
                            ? subtitleTracksQuery.error.message
                            : l('Failed to load subtitles')}
                        </p>
                      ) : null}
                      {!subtitleTracksQuery.isLoading &&
                      !subtitleTracksQuery.isError &&
                      selectedSubtitleTrack ? (
                        <dl className="media-tech-card__facts">
                          {buildSubtitleTrackFacts(
                            selectedSubtitleTrack,
                            selectedSubtitleTrackIndex >= 0 ? selectedSubtitleTrackIndex : 0,
                          ).map((fact) => (
                            <div
                              className="media-tech-card__fact"
                              key={`${selectedSubtitleTrack.id}-${fact.label}`}
                            >
                              <dt>{fact.label}</dt>
                              <dd>{fact.value}</dd>
                            </div>
                          ))}
                        </dl>
                      ) : null}
                      {!subtitleTracksQuery.isLoading &&
                      !subtitleTracksQuery.isError &&
                      !selectedSubtitleTrack ? (
                        <p className="muted">{l('No subtitle tracks were detected.')}</p>
                      ) : null}
                    </article>
                  </div>
                </div>
              ) : null
            ) : null}

            {!mediaFilesQuery.isLoading && !mediaFilesQuery.isError && mediaFiles.length === 0 ? (
              <p className="muted">
                {isSeriesView
                  ? l('No source files are linked to the selected season episode yet.')
                  : l('No source files are linked to this title yet.')}
              </p>
            ) : null}
          </div>
        </section>
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
