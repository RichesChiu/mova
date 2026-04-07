import { mediaItemPlayPath } from './media-routes'

export type PlaybackProgressLike =
  | {
      position_seconds: number
      duration_seconds: number | null
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

type PlayableEpisode = {
  is_available: boolean
  media_item_id: number | null
  playback_progress: PlaybackProgressLike
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
