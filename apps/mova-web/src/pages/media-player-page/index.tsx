import { useQuery } from '@tanstack/react-query'
import { Navigate, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { ApiError, getMediaItemEpisodeOutline, getMediaItemPlaybackHeader } from '../../api/client'
import { MediaPlayerPanel } from '../../components/media-player-panel'
import { mediaItemDetailPath, mediaItemPlayPath } from '../../lib/media-routes'
import {
  MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  MEDIA_QUERY_GC_TIME_MS,
  SERIES_OUTLINE_QUERY_STALE_TIME_MS,
} from '../../lib/query-options'

export const MediaPlayerPage = () => {
  const navigate = useNavigate()
  const params = useParams()
  const [searchParams] = useSearchParams()
  const mediaItemId = Number(params.mediaItemId)
  const startMode = searchParams.get('fromStart') === '1' ? 'from-start' : 'resume'

  const playbackHeaderQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled: Number.isFinite(mediaItemId),
    queryKey: ['media-item-playback-header', mediaItemId],
    queryFn: () => getMediaItemPlaybackHeader(mediaItemId),
    retry: false,
    staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  })
  const episodeOutlineQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled:
      playbackHeaderQuery.data?.media_type === 'episode' &&
      Number.isFinite(playbackHeaderQuery.data.series_media_item_id ?? Number.NaN),
    queryKey: ['media-episode-outline', playbackHeaderQuery.data?.series_media_item_id],
    queryFn: () => getMediaItemEpisodeOutline(playbackHeaderQuery.data?.series_media_item_id ?? 0),
    staleTime: SERIES_OUTLINE_QUERY_STALE_TIME_MS,
  })

  if (!Number.isFinite(mediaItemId)) {
    return (
      <div className="player-screen player-screen--state">
        <p className="callout callout--danger">Invalid media item id.</p>
      </div>
    )
  }

  if (playbackHeaderQuery.isLoading) {
    return (
      <div className="player-screen player-screen--state">
        <p className="muted">Loading player…</p>
      </div>
    )
  }

  if (playbackHeaderQuery.isError) {
    if (playbackHeaderQuery.error instanceof ApiError && playbackHeaderQuery.error.status === 401) {
      return <Navigate replace to="/login" />
    }

    return (
      <div className="player-screen player-screen--state">
        <p className="callout callout--danger">
          {playbackHeaderQuery.error instanceof Error
            ? playbackHeaderQuery.error.message
            : 'Failed to load media item'}
        </p>
      </div>
    )
  }

  if (!playbackHeaderQuery.data) {
    return null
  }

  if (playbackHeaderQuery.data.media_type === 'series') {
    return <Navigate replace to={mediaItemDetailPath(playbackHeaderQuery.data.media_item_id)} />
  }

  const subtitle =
    playbackHeaderQuery.data.media_type === 'episode'
      ? [
          playbackHeaderQuery.data.season_number
            ? `第${playbackHeaderQuery.data.season_number}季`
            : null,
          playbackHeaderQuery.data.episode_number
            ? `第${playbackHeaderQuery.data.episode_number}集`
            : null,
          playbackHeaderQuery.data.episode_title,
        ]
          .filter(Boolean)
          .join(' · ')
      : [
          playbackHeaderQuery.data.original_title,
          playbackHeaderQuery.data.year ? String(playbackHeaderQuery.data.year) : null,
        ]
          .filter(Boolean)
          .join(' · ')

  const outlineSeasons =
    episodeOutlineQuery.data?.seasons
      .map((season) => ({
        ...season,
        episodes: season.episodes.filter(
          (episode) => episode.is_available && episode.media_item_id,
        ),
      }))
      .filter((season) => season.episodes.length > 0) ?? []
  const currentSeason = outlineSeasons.find(
    (season) => season.season_number === playbackHeaderQuery.data.season_number,
  )
  const episodeSwitchOptions =
    currentSeason?.episodes
      .filter(
        (episode) =>
          episode.media_item_id !== null &&
          episode.media_item_id !== playbackHeaderQuery.data.media_item_id,
      )
      .map((episode) => ({
        label: `E${String(episode.episode_number).padStart(2, '0')} · ${episode.title}`,
        mediaItemId: episode.media_item_id as number,
      })) ?? []

  return (
    <div className="player-screen">
      {/* 用顶部/底部热区触发工具栏，避免画面较小时必须先 hover 到视频本体。 */}
      <div aria-hidden="true" className="player-screen__hotspot player-screen__hotspot--top" />
      <div aria-hidden="true" className="player-screen__hotspot player-screen__hotspot--bottom" />

      <header className="player-screen__chrome">
        <button
          aria-label="Go back"
          className="player-screen__back"
          onClick={() => {
            if (window.history.length > 1) {
              navigate(-1)
              return
            }

            navigate(`/libraries/${playbackHeaderQuery.data.library_id}`)
          }}
          type="button"
        >
          <svg
            aria-hidden="true"
            className="player-screen__back-icon"
            fill="none"
            viewBox="0 0 16 16"
          >
            <path
              d="M9.5 3.5L5.5 8L9.5 12.5"
              stroke="currentColor"
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth="1.8"
            />
          </svg>
        </button>

        <div className="player-screen__title-lockup">
          <div className="player-screen__title">
            <strong>{playbackHeaderQuery.data.title}</strong>
            {subtitle ? <span>{subtitle}</span> : null}
          </div>
        </div>
      </header>

      <main className="player-screen__viewport">
        <MediaPlayerPanel
          episodeSwitchOptions={episodeSwitchOptions}
          mediaItemId={playbackHeaderQuery.data.media_item_id}
          onSelectEpisode={(targetMediaItemId) => navigate(mediaItemPlayPath(targetMediaItemId))}
          startMode={startMode}
          title={playbackHeaderQuery.data.title}
          variant="immersive"
        />
      </main>
    </div>
  )
}
