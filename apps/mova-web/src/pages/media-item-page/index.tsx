import { useQuery } from '@tanstack/react-query'
import { type CSSProperties, useEffect, useState } from 'react'
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
  formatMediaItemScanStatusCopy,
  formatScanItemMeta,
  formatScanItemProgressCopy,
  getLibraryScanRuntime,
  getMediaItemScanRuntimeItems,
  getScanJobProgressPercent,
  isLibraryScanActive,
  type ScanRuntimeItem,
} from '../../components/app-shell/scan-runtime'
import { EpisodeCard, EpisodeCardSkeleton } from '../../components/episode-card'
import { GlassSelect } from '../../components/glass-select'
import { MediaTypeTag } from '../../components/media-type-tag'
import { MetadataMatchPanel } from '../../components/metadata-match-panel'
import { ScrollableRail } from '../../components/scrollable-rail'
import { formatMediaCountry } from '../../lib/media-country'
import {
  buildAudioTrackFacts,
  buildAudioTrackOptions,
  buildMediaFileFeatureBadges,
  buildMediaSourceFacts,
  buildMediaVersionOptions,
  buildSubtitleTrackFacts,
  buildSubtitleTrackOptions,
  buildVideoCardFacts,
  getMediaFileDisplayName,
} from '../../lib/media-file-details'
import { mediaItemDetailPath, mediaItemPlayPath } from '../../lib/media-routes'
import {
  buildPlaybackActionLinks,
  pickPreferredPlaybackEpisode,
  playbackPercent,
  playbackStatus,
} from '../../lib/playback'
import {
  MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  MEDIA_QUERY_GC_TIME_MS,
  SERIES_OUTLINE_QUERY_STALE_TIME_MS,
} from '../../lib/query-options'
import { canManageLibraries } from '../../lib/viewer'

const EPISODE_SKELETONS = [
  { metaLabel: 'S01 · E01', placeholderLabel: '1-1' },
  { metaLabel: 'S01 · E02', placeholderLabel: '1-2' },
  { metaLabel: 'S01 · E03', placeholderLabel: '1-3' },
  { metaLabel: 'S01 · E04', placeholderLabel: '1-4' },
] as const

const SeasonBlock = ({
  scanItems,
  season,
}: {
  scanItems: ScanRuntimeItem[]
  season: EpisodeOutlineSeason
}) => {
  const entries = [
    ...scanItems.map((item) => ({
      key: `scan-${item.item_key}`,
      order: item.episode_number ?? Number.MAX_SAFE_INTEGER,
      render: () => {
        const metaLabel = formatScanItemMeta(item)

        return (
          <EpisodeCard
            artworkAlt={`${metaLabel} artwork`}
            description={formatScanItemProgressCopy(item)}
            key={`scan-${item.item_key}`}
            metaLabel={metaLabel}
            placeholderLabel={metaLabel}
            progressPercent={item.progress_percent}
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
        const artwork = episode.poster_path ?? episode.backdrop_path
        const title = episode.title.trim() || `Episode ${episode.episode_number}`
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
      <ScrollableRail hint="Scroll, drag, or click arrows to move through episodes.">
        {entries.map((entry) => entry.render())}
      </ScrollableRail>
    </article>
  )
}

const SeasonBlockSkeleton = () => (
  <article aria-hidden="true" className="season-card">
    <ScrollableRail hint="Scroll, drag, or click arrows to move through episodes.">
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

const isHeroFact = (item: HeroFact | null): item is HeroFact => item !== null

export const MediaItemPage = () => {
  const { currentUser, scanRuntimeByLibrary } = useOutletContext<AppShellOutletContext>()
  const params = useParams()
  const [searchParams] = useSearchParams()
  const mediaItemId = Number(params.mediaItemId)
  const [selectedSeasonNumber, setSelectedSeasonNumber] = useState<number | null>(null)
  const [selectedMediaVersionId, setSelectedMediaVersionId] = useState<number | null>(null)
  const [selectedAudioTrackId, setSelectedAudioTrackId] = useState<string>('')
  const [selectedSubtitleTrackId, setSelectedSubtitleTrackId] = useState<string>('')
  const [castRefreshState, setCastRefreshState] = useState<{
    attempts: number
    mediaItemId: number
  }>({
    attempts: 0,
    mediaItemId,
  })
  const requestedSeasonParam = searchParams.get('season')
  const requestedSeasonNumber = requestedSeasonParam ? Number(requestedSeasonParam) : Number.NaN

  const mediaItemQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled: Number.isFinite(mediaItemId),
    queryKey: ['media-item', mediaItemId],
    queryFn: () => getMediaItem(mediaItemId),
    staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  })
  const mediaFilesQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled: mediaItemQuery.data?.media_type === 'movie',
    queryKey: ['media-item-files', mediaItemId],
    queryFn: () => listMediaItemFiles(mediaItemId),
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
  const mediaFiles = mediaFilesQuery.data ?? []
  const shouldShowMediaFilesSection = mediaItemQuery.data?.media_type === 'movie'
  const selectedMediaFile =
    mediaFiles.find((file) => file.id === selectedMediaVersionId) ?? mediaFiles[0] ?? null
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
  const castRefreshAttempts =
    castRefreshState.mediaItemId === mediaItemId ? castRefreshState.attempts : 0
  const shouldRetryCastFetch =
    !!mediaItemQuery.data &&
    mediaItemQuery.data.media_type !== 'episode' &&
    !castQuery.isError &&
    castMembers.length === 0 &&
    castRefreshAttempts < 6
  const selectedSeason = availableSeasons.find(
    (season) => season.season_number === selectedSeasonNumber,
  )
  const shouldShowEpisodeSkeleton =
    episodeOutlineQuery.isLoading && !episodeOutlineQuery.isError && availableSeasons.length === 0
  const selectedSeasonLabel = selectedSeason
    ? (selectedSeason.title ?? `Season ${selectedSeason.season_number}`)
    : null
  const selectedSeasonYear = selectedSeason?.year ?? null
  const selectedSeasonEpisodeCount =
    selectedSeason?.episodes.filter((episode) => episode.is_available).length ?? 0

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
    if (!shouldRetryCastFetch || castQuery.isFetching) {
      return
    }

    const timeoutId = window.setTimeout(() => {
      setCastRefreshState((current) => ({
        attempts: current.mediaItemId === mediaItemId ? current.attempts + 1 : 1,
        mediaItemId,
      }))
      void castQuery.refetch()
    }, 1200)

    return () => window.clearTimeout(timeoutId)
  }, [castQuery.isFetching, castQuery.refetch, mediaItemId, shouldRetryCastFetch])

  useEffect(() => {
    if (!shouldShowMediaFilesSection || mediaFiles.length === 0) {
      setSelectedMediaVersionId(null)
      return
    }

    const preferredMediaFile =
      (moviePlaybackProgressQuery.data &&
        mediaFiles.find((file) => file.id === moviePlaybackProgressQuery.data?.media_file_id)) ??
      mediaFiles[0]

    setSelectedMediaVersionId((current) =>
      current && mediaFiles.some((file) => file.id === current) ? current : preferredMediaFile.id,
    )
  }, [mediaFiles, moviePlaybackProgressQuery.data, shouldShowMediaFilesSection])

  useEffect(() => {
    if (!shouldShowMediaFilesSection && selectedMediaVersionId === null) {
      return
    }

    setSelectedAudioTrackId('')
    setSelectedSubtitleTrackId('')
  }, [selectedMediaVersionId, shouldShowMediaFilesSection])

  const isSeriesView = mediaItemQuery.data?.media_type === 'series'
  const canMatchMetadata =
    canManageLibraries(currentUser) && mediaItemQuery.data?.media_type !== 'episode'
  const currentScanRuntime = mediaItemQuery.data
    ? getLibraryScanRuntime(scanRuntimeByLibrary, mediaItemQuery.data.library_id)
    : null
  const detailScanItems = getMediaItemScanRuntimeItems(mediaItemQuery.data, currentScanRuntime, {
    seasonNumber: isSeriesView ? selectedSeasonNumber : null,
  })
  const primaryDetailScanItem = detailScanItems[0] ?? null
  const detailScanCopy = isLibraryScanActive(null, currentScanRuntime)
    ? formatMediaItemScanStatusCopy(mediaItemQuery.data, currentScanRuntime, {
        seasonNumber: isSeriesView ? selectedSeasonNumber : null,
      })
    : null
  const detailScanProgressPercent = primaryDetailScanItem
    ? primaryDetailScanItem.progress_percent
    : getScanJobProgressPercent(null, currentScanRuntime)
  const detailScanSubtitle = primaryDetailScanItem
    ? [
        primaryDetailScanItem.title,
        primaryDetailScanItem.media_type === 'movie'
          ? null
          : formatScanItemMeta(primaryDetailScanItem),
      ]
        .filter(Boolean)
        .join(' · ')
    : 'Details and artwork may continue updating until the sync finishes.'
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
  // 剧集详情页优先把按钮落到当前季里已有断点的那一集；如果当前季还没播过，再回退到第一集。
  const selectedSeasonPlayableEpisode = pickPreferredPlaybackEpisode(selectedSeason?.episodes)
  const playbackTargetMediaItemId = isSeriesView
    ? (selectedSeasonPlayableEpisode?.media_item_id ?? null)
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
          ? selectedSeasonPlayableEpisode?.playback_progress
          : moviePlaybackProgressQuery.data,
      )
    : null
  const heroPosterPath =
    isSeriesView && selectedSeason
      ? (selectedSeason.poster_path ?? mediaItemQuery.data?.poster_path ?? null)
      : (mediaItemQuery.data?.poster_path ?? null)
  const heroBackdropPath =
    isSeriesView && selectedSeason
      ? (selectedSeason.backdrop_path ?? mediaItemQuery.data?.backdrop_path ?? null)
      : (mediaItemQuery.data?.backdrop_path ?? null)
  const heroBackgroundImage = heroBackdropPath ?? heroPosterPath
  const heroAccentImage = heroPosterPath ?? heroBackdropPath
  const heroStyle = heroBackgroundImage
    ? ({
        ['--detail-hero-image' as string]: `url(${heroBackgroundImage})`,
        ['--detail-hero-accent-image' as string]: heroAccentImage
          ? `url(${heroAccentImage})`
          : 'none',
      } as CSSProperties)
    : undefined
  const heroEyebrow = isSeriesView ? 'series' : (mediaItemQuery.data?.media_type ?? '')
  const heroTitle = mediaItemQuery.data?.title ?? ''
  const heroImdbRating = mediaItemQuery.data?.imdb_rating?.trim() || null
  const heroCountry = formatMediaCountry(mediaItemQuery.data?.country)
  const heroGenres = mediaItemQuery.data?.genres?.trim() || null
  const heroStudio = mediaItemQuery.data?.studio?.trim() || null
  const heroOverview =
    isSeriesView && selectedSeason
      ? (selectedSeason.overview ??
        `Currently showing ${selectedSeasonLabel ?? `Season ${selectedSeason.season_number}`}.`)
      : (mediaItemQuery.data?.overview ?? 'No overview available yet.')
  const heroFacts = isSeriesView
    ? [
        mediaItemQuery.data?.original_title &&
        mediaItemQuery.data.original_title !== mediaItemQuery.data.title
          ? {
              label: 'Original title',
              value: mediaItemQuery.data.original_title,
            }
          : null,
        selectedSeasonYear
          ? {
              label: 'Season air year',
              value: String(selectedSeasonYear),
            }
          : mediaItemQuery.data?.year
            ? {
                label: 'Series first air year',
                value: String(mediaItemQuery.data.year),
              }
            : null,
        heroGenres
          ? {
              label: 'Genres',
              value: heroGenres,
            }
          : null,
        heroStudio
          ? {
              label: 'Studio',
              value: heroStudio,
            }
          : null,
        selectedSeason
          ? {
              label: 'Available episodes',
              value: String(selectedSeasonEpisodeCount),
            }
          : {
              label: 'Available seasons',
              value: String(availableSeasons.length),
            },
        heroCountry
          ? {
              label: 'Country',
              value: heroCountry,
            }
          : null,
      ].filter(isHeroFact)
    : [
        mediaItemQuery.data?.original_title &&
        mediaItemQuery.data.original_title !== mediaItemQuery.data.title
          ? {
              label: 'Original title',
              value: mediaItemQuery.data.original_title,
            }
          : null,
        {
          label: 'Release year',
          value: mediaItemQuery.data?.year ? String(mediaItemQuery.data.year) : 'Unknown',
        },
        heroGenres
          ? {
              label: 'Genres',
              value: heroGenres,
            }
          : null,
        heroStudio
          ? {
              label: 'Studio',
              value: heroStudio,
            }
          : null,
        heroCountry
          ? {
              label: 'Country',
              value: heroCountry,
            }
          : null,
      ].filter(isHeroFact)

  if (!Number.isFinite(mediaItemId)) {
    return <p className="callout callout--danger">Invalid media item id.</p>
  }

  if (mediaItemQuery.isLoading) {
    return (
      <div className="page-stack">
        <p className="muted">Loading media item…</p>
      </div>
    )
  }

  if (mediaItemQuery.isError) {
    return (
      <div className="page-stack">
        <p className="callout callout--danger">
          {mediaItemQuery.error instanceof Error
            ? mediaItemQuery.error.message
            : 'Failed to load media item'}
        </p>
      </div>
    )
  }

  if (!mediaItemQuery.data) {
    return null
  }

  if (mediaItemQuery.data.media_type === 'episode') {
    if (playbackHeaderQuery.isLoading) {
      return (
        <div className="page-stack">
          <p className="muted">Loading media item…</p>
        </div>
      )
    }

    if (playbackHeaderQuery.data?.series_media_item_id) {
      return (
        <Navigate replace to={mediaItemDetailPath(playbackHeaderQuery.data.series_media_item_id)} />
      )
    }

    return <Navigate replace to={mediaItemPlayPath(mediaItemQuery.data.id)} />
  }

  return (
    <div className="page-stack">
      <div className="media-item-toolbar">
        <Link
          className="back-link media-item-back-link"
          to={`/libraries/${mediaItemQuery.data.library_id}`}
        >
          <svg aria-hidden="true" className="back-link__icon" fill="none" viewBox="0 0 16 16">
            <path
              d="M9.5 3.5L5.5 8L9.5 12.5"
              stroke="currentColor"
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth="1.8"
            />
          </svg>
          <span>Back to Library</span>
        </Link>
      </div>

      <section className="detail-hero" style={heroStyle}>
        <div className="detail-hero__poster">
          {heroPosterPath ? (
            <img alt={`${heroTitle} poster`} src={heroPosterPath} />
          ) : (
            <div className="media-card__placeholder">
              <span>{mediaItemQuery.data.media_type}</span>
            </div>
          )}
        </div>

        <div className="detail-hero__body">
          <div className="detail-hero__eyebrow-row">
            <p className="eyebrow">{heroEyebrow}</p>
            <MediaTypeTag mediaType={mediaItemQuery.data.media_type} />
          </div>
          <div className="detail-hero__title-row">
            <h2>{heroTitle}</h2>
            {heroImdbRating ? (
              <span className="detail-hero__rating-badge" title={`IMDb rating ${heroImdbRating}`}>
                <span className="detail-hero__rating-label">IMDb</span>
                <strong>{heroImdbRating}</strong>
              </span>
            ) : null}
          </div>
          {isSeriesView && availableSeasons.length > 0 ? (
            <div className="detail-hero__season-picker">
              <div className="detail-hero__season-heading">
                <p className="detail-hero__season-label">Season</p>
                <span className="muted">
                  {selectedSeasonLabel ?? `Season ${selectedSeasonNumber ?? 1}`}
                </span>
              </div>
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
          {heroFacts.length > 0 ? (
            <div className="detail-hero__facts">
              {heroFacts.map((item) => (
                <article className="detail-hero__fact" key={item.label}>
                  <p className="detail-hero__fact-label">{item.label}</p>
                  <p className="detail-hero__fact-value">{item.value}</p>
                </article>
              ))}
            </div>
          ) : null}
          <p className="detail-hero__overview">{heroOverview}</p>
          {detailScanCopy ? (
            <div className="detail-hero__sync-note" role="status">
              <div className="detail-hero__sync-copy">
                <p className="detail-hero__sync-label">
                  {primaryDetailScanItem ? 'This item is syncing' : 'This library is syncing'}
                </p>
                <strong>{detailScanCopy}</strong>
                <span className="muted">{detailScanSubtitle}</span>
              </div>

              <div aria-hidden="true" className="detail-hero__sync-progress">
                <span style={{ width: `${detailScanProgressPercent}%` }} />
              </div>
            </div>
          ) : null}
          {!isSeriesView && mediaVersionOptions.length > 1 ? (
            <div className="detail-hero__version-picker">
              <p className="detail-hero__version-label">Version</p>
              <GlassSelect
                ariaLabel={`Select playback version for ${heroTitle}`}
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
                  className="button button--primary"
                  to={
                    playbackPrimaryPath ??
                    playbackActionLinks?.primaryPath ??
                    mediaItemPlayPath(playbackTargetMediaItemId)
                  }
                >
                  <span>{playbackActionLinks?.primaryLabel ?? 'Play'}</span>
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
      </section>

      {isSeriesView ? (
        <section className="page-stack">
          <div className="section-heading">
            <h3>Episodes</h3>
          </div>

          {selectedSeasonScanItems.length > 0 ? (
            <p className="muted">
              {selectedSeasonScanItems.length}{' '}
              {selectedSeasonScanItems.length === 1 ? 'episode is' : 'episodes are'} still syncing
              in this season. Placeholder cards stay visible until the library write completes.
            </p>
          ) : null}

          {shouldShowEpisodeSkeleton ? (
            <>
              <p className="muted">Loading episodes…</p>
              <SeasonBlockSkeleton />
            </>
          ) : null}

          {episodeOutlineQuery.isError ? (
            <p className="callout callout--danger">
              {episodeOutlineQuery.error instanceof Error
                ? episodeOutlineQuery.error.message
                : 'Failed to load episodes'}
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
            <p className="muted">No local episodes available in this series yet.</p>
          ) : null}
        </section>
      ) : null}

      {castQuery.isLoading ||
      castQuery.isFetching ||
      castMembers.length > 0 ||
      shouldRetryCastFetch ? (
        <section className="season-card cast-panel">
          <div className="cast-panel__header">
            <div>
              <p className="eyebrow">Cast</p>
              <h3>Main Cast</h3>
            </div>
            {!castQuery.isLoading ? (
              <span className="counter-badge">{castMembers.length}</span>
            ) : null}
          </div>

          {castQuery.isLoading || castQuery.isFetching || shouldRetryCastFetch ? (
            <p className="muted">Loading cast…</p>
          ) : castQuery.isError ? (
            <p className="muted">Cast details are taking longer than usual.</p>
          ) : castMembers.length > 0 ? (
            <ScrollableRail
              hint="Scroll, drag, or click arrows to move through the cast list."
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
                      {member.character_name ? `as ${member.character_name}` : 'Actor'}
                    </p>
                  </div>
                </article>
              ))}
            </ScrollableRail>
          ) : null}
        </section>
      ) : null}

      {shouldShowMediaFilesSection ? (
        <section className="season-card media-file-panel">
          <div className="media-file-panel__header">
            <div>
              <p className="eyebrow">Source Files</p>
              <h3>Technical Details</h3>
            </div>
            {!mediaFilesQuery.isLoading && !mediaFilesQuery.isError ? (
              <span className="counter-badge">{mediaFiles.length}</span>
            ) : null}
          </div>

          {mediaFilesQuery.isLoading ? <p className="muted">Loading source details…</p> : null}

          {mediaFilesQuery.isError ? (
            <p className="callout callout--danger">
              {mediaFilesQuery.error instanceof Error
                ? mediaFilesQuery.error.message
                : 'Failed to load source details'}
            </p>
          ) : null}

          {!mediaFilesQuery.isLoading && !mediaFilesQuery.isError && mediaFiles.length > 0 ? (
            selectedMediaFile ? (
              <div className="media-file-panel__list">
                <article className="media-file-card" key={selectedMediaFile.id}>
                  <div className="media-file-card__header">
                    <div className="media-file-card__title-block">
                      <p className="media-file-card__eyebrow">Source Details</p>
                      <h4>{getMediaFileDisplayName(selectedMediaFile.file_path)}</h4>
                    </div>

                    {buildMediaFileFeatureBadges(selectedMediaFile).length > 0 ? (
                      <div className="media-file-card__badges">
                        {buildMediaFileFeatureBadges(selectedMediaFile).map((badge) => {
                          const isFeatureBadge = badge.startsWith('Dolby')

                          return (
                            <span
                              className={
                                isFeatureBadge
                                  ? 'media-file-card__badge media-file-card__badge--feature'
                                  : 'media-file-card__badge'
                              }
                              key={`${selectedMediaFile.id}-${badge}`}
                            >
                              {badge}
                            </span>
                          )
                        })}
                      </div>
                    ) : null}
                  </div>

                  <div className="media-file-card__details">
                    <div className="media-file-card__path-block">
                      <p className="media-file-card__label">Path</p>
                      <p className="media-file-card__path">{selectedMediaFile.file_path}</p>
                    </div>

                    <dl className="media-file-card__facts">
                      {buildMediaSourceFacts(selectedMediaFile).map((fact) => (
                        <div
                          className="media-file-card__fact"
                          key={`${selectedMediaFile.id}-${fact.label}`}
                        >
                          <dt>{fact.label}</dt>
                          <dd>{fact.value}</dd>
                        </div>
                      ))}
                    </dl>
                  </div>

                  <div className="media-tech-stack">
                    <article className="media-tech-card media-tech-card--video">
                      <div className="media-tech-card__header">
                        <div className="media-tech-card__title-block">
                          <p className="media-tech-card__eyebrow">Video</p>
                          <h5>Video Details</h5>
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
                          <p className="media-tech-card__eyebrow">Audio</p>
                          <h5>Audio Details</h5>
                        </div>

                        <div className="media-tech-card__selector">
                          <GlassSelect
                            ariaLabel={`Select audio track for ${getMediaFileDisplayName(selectedMediaFile.file_path)}`}
                            compact
                            disabled={audioTrackOptions.length === 0}
                            onChange={(value) => setSelectedAudioTrackId(value)}
                            options={
                              audioTrackOptions.length > 0
                                ? audioTrackOptions
                                : [
                                    {
                                      label: 'No audio tracks detected',
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
                        <p className="muted">Loading audio tracks…</p>
                      ) : null}
                      {audioTracksQuery.isError ? (
                        <p className="callout callout--danger">
                          {audioTracksQuery.error instanceof Error
                            ? audioTracksQuery.error.message
                            : 'Failed to load audio tracks'}
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
                        <p className="muted">No embedded audio tracks were detected.</p>
                      ) : null}
                    </article>

                    <article className="media-tech-card">
                      <div className="media-tech-card__header media-tech-card__header--with-select">
                        <div className="media-tech-card__title-block">
                          <p className="media-tech-card__eyebrow">Subtitle</p>
                          <h5>Subtitle Details</h5>
                        </div>

                        <div className="media-tech-card__selector">
                          <GlassSelect
                            ariaLabel={`Select subtitle track for ${getMediaFileDisplayName(selectedMediaFile.file_path)}`}
                            compact
                            disabled={subtitleTrackOptions.length === 0}
                            onChange={(value) => setSelectedSubtitleTrackId(value)}
                            options={
                              subtitleTrackOptions.length > 0
                                ? subtitleTrackOptions
                                : [
                                    {
                                      label: 'No subtitles detected',
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
                        <p className="muted">Loading subtitles…</p>
                      ) : null}
                      {subtitleTracksQuery.isError ? (
                        <p className="callout callout--danger">
                          {subtitleTracksQuery.error instanceof Error
                            ? subtitleTracksQuery.error.message
                            : 'Failed to load subtitles'}
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
                        <p className="muted">No subtitle tracks were detected.</p>
                      ) : null}
                    </article>
                  </div>
                </article>
              </div>
            ) : null
          ) : null}

          {!mediaFilesQuery.isLoading && !mediaFilesQuery.isError && mediaFiles.length === 0 ? (
            <p className="muted">No source files are linked to this title yet.</p>
          ) : null}
        </section>
      ) : null}
    </div>
  )
}
