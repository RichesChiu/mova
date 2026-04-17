import { mediaItemPlayPath } from './media-routes'

export type PlaybackProgressLike =
  | {
      position_seconds: number
      duration_seconds: number | null
      last_watched_at?: string
      is_finished: boolean
    }
  | null
  | undefined

export type PlaybackStatus = 'complete' | 'progress' | 'idle'

export interface PlaybackActionLinks {
  primaryLabel: 'Resume Playback' | 'Play'
  primaryPath: string
  secondaryPath: string | null
}

interface PlaybackCompletionInput {
  durationSeconds: number | null | undefined
  positionSeconds: number
}

type PlayableEpisode = {
  is_available: boolean
  media_item_id: number | null
  playback_progress: PlaybackProgressLike
}

const parsePlaybackTimestamp = (progress: PlaybackProgressLike) => {
  const rawTimestamp = progress?.last_watched_at
  if (!rawTimestamp) {
    return Number.NEGATIVE_INFINITY
  }

  const parsed = Date.parse(rawTimestamp)
  return Number.isFinite(parsed) ? parsed : Number.NEGATIVE_INFINITY
}

export const playbackPercent = (progress: PlaybackProgressLike) => {
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

export const shouldMarkPlaybackFinished = ({
  durationSeconds,
  positionSeconds,
}: PlaybackCompletionInput) => {
  if (!durationSeconds || durationSeconds <= 0 || positionSeconds <= 0) {
    return false
  }

  const boundedPosition = Math.max(0, Math.min(positionSeconds, durationSeconds))
  const remainingSeconds = durationSeconds - boundedPosition
  const completionWindowSeconds = Math.min(30, Math.max(5, Math.round(durationSeconds * 0.05)))

  return remainingSeconds <= completionWindowSeconds
}

export const playbackStatus = (progress: PlaybackProgressLike): PlaybackStatus => {
  if (progress?.is_finished) {
    return 'complete'
  }

  const percent = playbackPercent(progress)
  if (typeof percent === 'number' && percent > 0) {
    return 'progress'
  }

  return 'idle'
}

export const isResumablePlayback = (progress: PlaybackProgressLike) =>
  Boolean(progress && !progress.is_finished && progress.position_seconds > 0)

export const buildPlaybackActionLinks = (
  mediaItemId: number,
  progress: PlaybackProgressLike,
): PlaybackActionLinks => {
  const shouldResumePlayback = isResumablePlayback(progress)

  return {
    primaryLabel: shouldResumePlayback ? 'Resume Playback' : 'Play',
    primaryPath: mediaItemPlayPath(mediaItemId),
    secondaryPath: shouldResumePlayback
      ? mediaItemPlayPath(mediaItemId, { fromStart: true })
      : null,
  }
}

export const pickPreferredPlaybackEpisode = <TEpisode extends PlayableEpisode>(
  episodes: TEpisode[] | null | undefined,
): TEpisode | null => {
  if (!episodes || episodes.length === 0) {
    return null
  }

  return (
    episodes.find(
      (episode) =>
        episode.is_available &&
        episode.media_item_id &&
        isResumablePlayback(episode.playback_progress),
    ) ??
    episodes.find((episode) => episode.is_available && episode.media_item_id) ??
    null
  )
}

export const pickSeriesPlaybackTargetEpisode = <TEpisode extends PlayableEpisode>(
  orderedEpisodes: TEpisode[] | null | undefined,
  fallbackEpisodes?: TEpisode[] | null | undefined,
): TEpisode | null => {
  const playableEpisodes =
    orderedEpisodes?.filter((episode) => episode.is_available && episode.media_item_id) ?? []

  let latestWatchedIndex = -1
  let latestWatchedTimestamp = Number.NEGATIVE_INFINITY

  playableEpisodes.forEach((episode, index) => {
    if (!episode.playback_progress) {
      return
    }

    const playbackTimestamp = parsePlaybackTimestamp(episode.playback_progress)
    if (
      latestWatchedIndex < 0 ||
      playbackTimestamp > latestWatchedTimestamp ||
      (playbackTimestamp === latestWatchedTimestamp && index > latestWatchedIndex)
    ) {
      latestWatchedIndex = index
      latestWatchedTimestamp = playbackTimestamp
    }
  })

  if (latestWatchedIndex >= 0) {
    const watchedEpisode = playableEpisodes[latestWatchedIndex]

    if (isResumablePlayback(watchedEpisode.playback_progress)) {
      return watchedEpisode
    }

    return playableEpisodes[latestWatchedIndex + 1] ?? watchedEpisode
  }

  return pickPreferredPlaybackEpisode(fallbackEpisodes ?? playableEpisodes)
}
