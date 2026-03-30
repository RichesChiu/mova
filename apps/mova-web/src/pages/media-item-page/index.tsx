import { useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { Link, Navigate, useOutletContext, useParams, useSearchParams } from 'react-router-dom'
import {
  getMediaItem,
  getMediaItemEpisodeOutline,
  getMediaItemPlaybackHeader,
  getMediaItemPlaybackProgress,
} from '../../api/client'
import type { EpisodeOutlineSeason, MediaCastMember } from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import { EpisodeCard } from '../../components/episode-card'
import { MetadataMatchPanel } from '../../components/metadata-match-panel'
import { ScrollableRail } from '../../components/scrollable-rail'
import { mediaItemDetailPath, mediaItemPlayPath } from '../../lib/media-routes'
import {
  MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  MEDIA_QUERY_GC_TIME_MS,
  SERIES_OUTLINE_QUERY_STALE_TIME_MS,
} from '../../lib/query-options'
import { canManageLibraries } from '../../lib/viewer'

const playbackPercent = (
  progress:
    | {
        position_seconds: number
        duration_seconds: number | null
        is_finished: boolean
      }
    | null
    | undefined,
) => {
  if (!progress) {
    return null
  }

  if (progress.is_finished) {
    return 100
  }

  if (!progress.duration_seconds || progress.duration_seconds <= 0) {
    return null
  }

  return Math.max(
    0,
    Math.min(100, Math.round((progress.position_seconds / progress.duration_seconds) * 100)),
  )
}

const playbackStatus = (
  progress:
    | {
        position_seconds: number
        duration_seconds: number | null
        is_finished: boolean
      }
    | null
    | undefined,
) => {
  if (progress?.is_finished) {
    return 'complete' as const
  }

  const percent = playbackPercent(progress)
  if (typeof percent === 'number' && percent > 0) {
    return 'progress' as const
  }

  return 'idle' as const
}

const isResumableProgress = (
  progress:
    | {
        position_seconds: number
        duration_seconds: number | null
        is_finished: boolean
      }
    | null
    | undefined,
) => Boolean(progress && !progress.is_finished && progress.position_seconds > 0)

const SeasonBlock = ({ season }: { season: EpisodeOutlineSeason }) => {
  return (
    <article className="season-card">
      <ScrollableRail hint="Drag or click arrows to scroll episodes horizontally.">
        {season.episodes.map((episode) => {
          const key = `${season.season_number}-${episode.episode_number}`
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
                key={key}
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
              key={key}
              metaLabel={index}
              placeholderLabel={index}
              progressPercent={progress}
              status={status}
              title={title}
            />
          )
        })}
      </ScrollableRail>
    </article>
  )
}

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
  const { currentUser } = useOutletContext<AppShellOutletContext>()
  const params = useParams()
  const [searchParams] = useSearchParams()
  const [selectedSeasonNumber, setSelectedSeasonNumber] = useState<number | null>(null)
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
  const castMembers = mediaItemQuery.data?.cast ?? []
  const selectedSeason = availableSeasons.find(
    (season) => season.season_number === selectedSeasonNumber,
  )
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
  // 剧集详情页优先把按钮落到当前季里已有断点的那一集；如果当前季还没播过，再回退到第一集。
  const selectedSeasonPlayableEpisode =
    selectedSeason?.episodes.find(
      (episode) =>
        episode.is_available &&
        episode.media_item_id &&
        isResumableProgress(episode.playback_progress),
    ) ??
    selectedSeason?.episodes.find((episode) => episode.is_available && episode.media_item_id) ??
    null
  const playbackTargetMediaItemId = isSeriesView
    ? (selectedSeasonPlayableEpisode?.media_item_id ?? null)
    : (mediaItemQuery.data?.id ?? null)
  const shouldResumePlayback = isSeriesView
    ? isResumableProgress(selectedSeasonPlayableEpisode?.playback_progress)
    : isResumableProgress(moviePlaybackProgressQuery.data)
  const playbackActionLabel = shouldResumePlayback ? 'Resume Playback' : 'Play'
  const heroPosterPath =
    isSeriesView && selectedSeason
      ? (selectedSeason.poster_path ?? mediaItemQuery.data?.poster_path ?? null)
      : (mediaItemQuery.data?.poster_path ?? null)
  const heroBackdropPath =
    isSeriesView && selectedSeason
      ? (selectedSeason.backdrop_path ?? mediaItemQuery.data?.backdrop_path ?? null)
      : (mediaItemQuery.data?.backdrop_path ?? null)
  const heroEyebrow = isSeriesView ? 'series' : (mediaItemQuery.data?.media_type ?? '')
  const heroTitle = mediaItemQuery.data?.title ?? ''
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

      <section
        className="detail-hero"
        style={
          heroBackdropPath
            ? {
                backgroundImage: `linear-gradient(135deg, rgba(6, 10, 18, 0.86), rgba(16, 12, 20, 0.68)), url(${heroBackdropPath})`,
              }
            : undefined
        }
      >
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
          <h2>{heroTitle}</h2>
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
          {playbackTargetMediaItemId || canMatchMetadata ? (
            <div className="detail-hero__actions">
              {playbackTargetMediaItemId ? (
                <Link
                  className="button button--primary"
                  to={mediaItemPlayPath(playbackTargetMediaItemId)}
                >
                  <span>{playbackActionLabel}</span>
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

          {episodeOutlineQuery.isLoading ? <p className="muted">Loading episodes…</p> : null}

          {episodeOutlineQuery.isError ? (
            <p className="callout callout--danger">
              {episodeOutlineQuery.error instanceof Error
                ? episodeOutlineQuery.error.message
                : 'Failed to load episodes'}
            </p>
          ) : null}

          {availableSeasons.length > 0 ? (
            selectedSeason ? (
              <SeasonBlock key={selectedSeason.season_number} season={selectedSeason} />
            ) : null
          ) : (
            <p className="muted">No local episodes available in this series yet.</p>
          )}
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

          <div className="cast-grid">
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
                    {member.character_name ? `饰 ${member.character_name}` : 'Actor'}
                  </p>
                </div>
              </article>
            ))}
          </div>
        </section>
      ) : null}
    </div>
  )
}
