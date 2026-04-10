import { useQueries, useQuery } from '@tanstack/react-query'
import { type CSSProperties, useEffect, useState } from 'react'
import { Link, Navigate, useOutletContext, useParams, useSearchParams } from 'react-router-dom'
import {
  getMediaItem,
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
import { MetadataMatchPanel } from '../../components/metadata-match-panel'
import { ScrollableRail } from '../../components/scrollable-rail'
import {
  buildAudioTrackFacts,
  buildAudioTrackOptions,
  buildMediaFileFeatureBadges,
  buildMediaSourceFacts,
  buildSubtitleTrackFacts,
  buildSubtitleTrackOptions,
  buildVideoCardFacts,
  buildVideoTrackOptions,
  getMediaFileDisplayName,
} from '../../lib/media-file-details'
import { mediaItemDetailPath, mediaItemPlayPath } from '../../lib/media-routes'
import {
  buildPlaybackActionLinks,
  isResumablePlayback,
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
  const [selectedSeasonNumber, setSelectedSeasonNumber] = useState<number | null>(null)
  const [selectedAudioTrackByFile, setSelectedAudioTrackByFile] = useState<Record<number, string>>(
    {},
  )
  const [selectedSubtitleByFile, setSelectedSubtitleByFile] = useState<Record<number, string>>({})
  const mediaItemId = Number(params.mediaItemId)
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
  const audioTrackQueries = useQueries({
    queries: mediaFiles.map((file) => ({
      gcTime: MEDIA_QUERY_GC_TIME_MS,
      enabled: shouldShowMediaFilesSection,
      queryKey: ['media-file-audio-tracks', file.id],
      queryFn: () => listMediaFileAudioTracks(file.id),
      staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
    })),
  })
  const subtitleTrackQueries = useQueries({
    queries: mediaFiles.map((file) => ({
      gcTime: MEDIA_QUERY_GC_TIME_MS,
      enabled: shouldShowMediaFilesSection,
      queryKey: ['media-file-subtitles', file.id],
      queryFn: () => listMediaFileSubtitles(file.id),
      staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
    })),
  })
  const availableSeasons = seasons.filter((season) =>
    season.episodes.some((episode) => episode.is_available),
  )
  const castMembers = mediaItemQuery.data?.cast ?? []
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
  const shouldResumePlayback = isSeriesView
    ? isResumablePlayback(selectedSeasonPlayableEpisode?.playback_progress)
    : isResumablePlayback(moviePlaybackProgressQuery.data)
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
  const heroMeta = isSeriesView
    ? [
        selectedSeasonLabel,
        selectedSeasonYear ? String(selectedSeasonYear) : null,
        mediaItemQuery.data?.original_title &&
        mediaItemQuery.data.original_title !== mediaItemQuery.data.title
          ? mediaItemQuery.data.original_title
          : null,
      ]
        .filter(Boolean)
        .join(' · ')
    : `${mediaItemQuery.data?.original_title ?? 'No original title'}${mediaItemQuery.data?.year ? ` · ${mediaItemQuery.data.year}` : ''}`
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
        selectedSeason
          ? {
              label: 'Available episodes',
              value: String(selectedSeasonEpisodeCount),
            }
          : {
              label: 'Available seasons',
              value: String(availableSeasons.length),
            },
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
          className="text-link media-item-back-link"
          to={`/libraries/${mediaItemQuery.data.library_id}`}
        >
          ← Back to Library
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
          <p className="eyebrow">{heroEyebrow}</p>
          <div className="detail-hero__title-row">
            <h2>{heroTitle}</h2>
            {heroImdbRating ? (
              <span className="detail-hero__rating-badge" title={`IMDb rating ${heroImdbRating}`}>
                <span className="detail-hero__rating-label">IMDb</span>
                <strong>{heroImdbRating}</strong>
              </span>
            ) : null}
          </div>
          <p className="muted">{heroMeta}</p>
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
          {playbackTargetMediaItemId || canMatchMetadata ? (
            <div className="detail-hero__actions">
              {playbackTargetMediaItemId ? (
                <>
                  <Link
                    className="button button--primary"
                    to={
                      playbackActionLinks?.primaryPath ??
                      mediaItemPlayPath(playbackTargetMediaItemId)
                    }
                  >
                    <span>{playbackActionLinks?.primaryLabel ?? 'Play'}</span>
                  </Link>

                  {shouldResumePlayback && playbackActionLinks?.secondaryPath ? (
                    <Link className="button" to={playbackActionLinks.secondaryPath}>
                      <span>Play from Beginning</span>
                    </Link>
                  ) : null}
                </>
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
              {selectedSeasonScanItems.length === 1 ? 'episode is' : 'episodes are'} still
              syncing in this season. Placeholder cards stay visible until the library write
              completes.
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

      {castMembers.length > 0 ? (
        <section className="season-card cast-panel">
          <div className="cast-panel__header">
            <div>
              <p className="eyebrow">Cast</p>
              <h3>Main Cast</h3>
            </div>
            <span className="counter-badge">{castMembers.length}</span>
          </div>

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
            <ScrollableRail
              hint="Scroll, drag, or click arrows to compare different source files."
              viewportClassName="media-file-panel__viewport"
            >
              {mediaFiles.map((file, index) => {
                const badges = buildMediaFileFeatureBadges(file)
                const sourceFacts = buildMediaSourceFacts(file)
                const videoFacts = buildVideoCardFacts(file)
                const audioTrackQuery = audioTrackQueries[index]
                const subtitleTrackQuery = subtitleTrackQueries[index]
                const audioTracks = audioTrackQuery?.data ?? []
                const subtitles = subtitleTrackQuery?.data ?? []
                const videoOptions = buildVideoTrackOptions(file)
                const selectedVideoOptionValue = videoOptions[0]?.value ?? String(file.id)
                const audioOptions = buildAudioTrackOptions(audioTracks)
                const selectedAudioTrackValue =
                  selectedAudioTrackByFile[file.id] ?? audioOptions[0]?.value ?? ''
                const selectedAudioTrack =
                  audioTracks.find(
                    (audioTrack) => String(audioTrack.id) === selectedAudioTrackValue,
                  ) ??
                  audioTracks[0] ??
                  null
                const subtitleOptions = buildSubtitleTrackOptions(subtitles)
                const selectedSubtitleValue =
                  selectedSubtitleByFile[file.id] ?? subtitleOptions[0]?.value ?? ''
                const selectedSubtitle =
                  subtitles.find((subtitle) => String(subtitle.id) === selectedSubtitleValue) ??
                  subtitles[0] ??
                  null
                const selectedSubtitleIndex = selectedSubtitle
                  ? subtitles.findIndex((subtitle) => subtitle.id === selectedSubtitle.id)
                  : -1

                return (
                  <article className="media-file-card" key={file.id}>
                    <div className="media-file-card__header">
                      <div className="media-file-card__title-block">
                        <p className="media-file-card__eyebrow">Source {index + 1}</p>
                        <h4>{getMediaFileDisplayName(file.file_path)}</h4>
                      </div>

                      {badges.length > 0 ? (
                        <div className="media-file-card__badges">
                          {badges.map((badge) => {
                            const isFeatureBadge = badge.startsWith('Dolby')

                            return (
                              <span
                                className={
                                  isFeatureBadge
                                    ? 'media-file-card__badge media-file-card__badge--feature'
                                    : 'media-file-card__badge'
                                }
                                key={`${file.id}-${badge}`}
                              >
                                {badge}
                              </span>
                            )
                          })}
                        </div>
                      ) : null}
                    </div>

                    <div className="media-file-card__path-block">
                      <p className="media-file-card__label">Path</p>
                      <p className="media-file-card__path">{file.file_path}</p>
                    </div>

                    <dl className="media-file-card__facts">
                      {sourceFacts.map((fact) => (
                        <div className="media-file-card__fact" key={`${file.id}-${fact.label}`}>
                          <dt>{fact.label}</dt>
                          <dd>{fact.value}</dd>
                        </div>
                      ))}
                    </dl>

                    <div className="media-tech-stack">
                      <article className="media-tech-card media-tech-card--video">
                        <div className="media-tech-card__header media-tech-card__header--with-select">
                          <div className="media-tech-card__title-block">
                            <p className="media-tech-card__eyebrow">Video</p>
                            <h5>Video Details</h5>
                          </div>

                          <div className="media-tech-card__selector">
                            <GlassSelect
                              ariaLabel={`Select source file for ${getMediaFileDisplayName(file.file_path)}`}
                              compact
                              onChange={() => {}}
                              options={videoOptions}
                              value={selectedVideoOptionValue}
                            />
                          </div>
                        </div>

                        <dl className="media-tech-card__facts">
                          {videoFacts.map((fact) => (
                            <div
                              className="media-tech-card__fact"
                              key={`${file.id}-video-${fact.label}`}
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
                              ariaLabel={`Select audio track for ${getMediaFileDisplayName(file.file_path)}`}
                              compact
                              disabled={audioOptions.length === 0}
                              onChange={(value) =>
                                setSelectedAudioTrackByFile((current) => ({
                                  ...current,
                                  [file.id]: value,
                                }))
                              }
                              options={
                                audioOptions.length > 0
                                  ? audioOptions
                                  : [
                                      {
                                        label: 'No audio tracks detected',
                                        value: `empty-audio-${file.id}`,
                                      },
                                    ]
                              }
                              value={
                                audioOptions.length > 0
                                  ? selectedAudioTrackValue
                                  : `empty-audio-${file.id}`
                              }
                            />
                          </div>
                        </div>

                        {audioTrackQuery?.isLoading ? (
                          <p className="muted">Loading audio tracks…</p>
                        ) : null}
                        {audioTrackQuery?.isError ? (
                          <p className="callout callout--danger">
                            {audioTrackQuery.error instanceof Error
                              ? audioTrackQuery.error.message
                              : 'Failed to load audio tracks'}
                          </p>
                        ) : null}
                        {!audioTrackQuery?.isLoading &&
                        !audioTrackQuery?.isError &&
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
                        {!audioTrackQuery?.isLoading &&
                        !audioTrackQuery?.isError &&
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
                              ariaLabel={`Select subtitle track for ${getMediaFileDisplayName(file.file_path)}`}
                              compact
                              disabled={subtitleOptions.length === 0}
                              onChange={(value) =>
                                setSelectedSubtitleByFile((current) => ({
                                  ...current,
                                  [file.id]: value,
                                }))
                              }
                              options={
                                subtitleOptions.length > 0
                                  ? subtitleOptions
                                  : [
                                      {
                                        label: 'No subtitles detected',
                                        value: `empty-subtitle-${file.id}`,
                                      },
                                    ]
                              }
                              value={
                                subtitleOptions.length > 0
                                  ? selectedSubtitleValue
                                  : `empty-subtitle-${file.id}`
                              }
                            />
                          </div>
                        </div>

                        {subtitleTrackQuery?.isLoading ? (
                          <p className="muted">Loading subtitles…</p>
                        ) : null}
                        {subtitleTrackQuery?.isError ? (
                          <p className="callout callout--danger">
                            {subtitleTrackQuery.error instanceof Error
                              ? subtitleTrackQuery.error.message
                              : 'Failed to load subtitles'}
                          </p>
                        ) : null}
                        {!subtitleTrackQuery?.isLoading &&
                        !subtitleTrackQuery?.isError &&
                        selectedSubtitle ? (
                          <dl className="media-tech-card__facts">
                            {buildSubtitleTrackFacts(
                              selectedSubtitle,
                              selectedSubtitleIndex >= 0 ? selectedSubtitleIndex : 0,
                            ).map((fact) => (
                              <div
                                className="media-tech-card__fact"
                                key={`${selectedSubtitle.id}-${fact.label}`}
                              >
                                <dt>{fact.label}</dt>
                                <dd>{fact.value}</dd>
                              </div>
                            ))}
                          </dl>
                        ) : null}
                        {!subtitleTrackQuery?.isLoading &&
                        !subtitleTrackQuery?.isError &&
                        !selectedSubtitle ? (
                          <p className="muted">No subtitle tracks were detected.</p>
                        ) : null}
                      </article>
                    </div>
                  </article>
                )
              })}
            </ScrollableRail>
          ) : null}

          {!mediaFilesQuery.isLoading && !mediaFilesQuery.isError && mediaFiles.length === 0 ? (
            <p className="muted">No source files are linked to this title yet.</p>
          ) : null}
        </section>
      ) : null}
    </div>
  )
}
