import { describe, expect, it } from 'vitest'
import {
  buildPlaybackActionLinks,
  isResumablePlayback,
  pickPreferredPlaybackEpisode,
  playbackPercent,
  playbackStatus,
  shouldMarkPlaybackFinished,
} from './playback'

describe('playback helpers', () => {
  it('builds resume and from-start links when progress is resumable', () => {
    expect(
      buildPlaybackActionLinks(31, {
        position_seconds: 320,
        duration_seconds: 7200,
        is_finished: false,
      }),
    ).toEqual({
      primaryLabel: 'Resume Playback',
      primaryPath: '/media-items/31/play',
      secondaryPath: '/media-items/31/play?fromStart=1',
    })
  })

  it('falls back to a single play action when no resumable progress exists', () => {
    expect(buildPlaybackActionLinks(31, null)).toEqual({
      primaryLabel: 'Play',
      primaryPath: '/media-items/31/play',
      secondaryPath: null,
    })
  })

  it('prefers resumable episodes over the first available episode', () => {
    expect(
      pickPreferredPlaybackEpisode([
        {
          is_available: true,
          media_item_id: 401,
          playback_progress: null,
        },
        {
          is_available: true,
          media_item_id: 402,
          playback_progress: {
            position_seconds: 180,
            duration_seconds: 2400,
            is_finished: false,
          },
        },
      ]),
    )?.toMatchObject({
      media_item_id: 402,
    })
  })

  it('derives playback percent and status from progress snapshots', () => {
    expect(
      playbackPercent({
        position_seconds: 600,
        duration_seconds: 2400,
        is_finished: false,
      }),
    ).toBe(25)
    expect(
      playbackStatus({
        position_seconds: 600,
        duration_seconds: 2400,
        is_finished: false,
      }),
    ).toBe('progress')
    expect(isResumablePlayback(null)).toBe(false)
  })

  it('marks playback as finished when the remaining time is within the completion window', () => {
    expect(
      shouldMarkPlaybackFinished({
        durationSeconds: 7200,
        positionSeconds: 7180,
      }),
    ).toBe(true)

    expect(
      shouldMarkPlaybackFinished({
        durationSeconds: 120,
        positionSeconds: 100,
      }),
    ).toBe(false)

    expect(
      shouldMarkPlaybackFinished({
        durationSeconds: 40,
        positionSeconds: 35,
      }),
    ).toBe(true)
  })
})
