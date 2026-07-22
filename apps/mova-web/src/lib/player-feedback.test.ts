import { describe, expect, it } from 'vitest'
import {
  buildFullscreenWarningMessage,
  buildPlaybackInteractionWarningMessage,
  isAutoplayBlockedError,
  shouldShowImmersiveOverlay,
} from './player-feedback'

describe('player-feedback helpers', () => {
  it('maps autoplay permission failures to a clear playback warning', () => {
    expect(
      buildPlaybackInteractionWarningMessage({
        name: 'NotAllowedError',
      }),
    ).toBe('Autoplay was blocked by the browser. Click play again to continue.')
  })

  it('identifies browser autoplay permission failures', () => {
    expect(isAutoplayBlockedError({ name: 'NotAllowedError' })).toBe(true)
    expect(isAutoplayBlockedError({ name: 'AbortError' })).toBe(false)
  })

  it('maps playback aborts to a retry-oriented warning', () => {
    expect(
      buildPlaybackInteractionWarningMessage({
        name: 'AbortError',
      }),
    ).toBe('Playback was interrupted before it could start. Click play again to continue.')
  })

  it('returns a stable fullscreen warning when the API is unavailable', () => {
    expect(buildFullscreenWarningMessage()).toBe(
      'Fullscreen is not available in this browser or app environment.',
    )
  })

  it('maps fullscreen permission failures to a clearer warning', () => {
    expect(
      buildFullscreenWarningMessage({
        name: 'NotAllowedError',
      }),
    ).toBe(
      'Fullscreen was blocked by the browser or app window. Try the browser fullscreen control.',
    )
  })

  it('keeps the immersive overlay visible for compatibility warnings and buffering states', () => {
    expect(
      shouldShowImmersiveOverlay({
        hasInteractionWarning: true,
        hasMultipleSources: false,
        hasPlaybackSyncError: false,
        hasPlayerError: false,
        hasSubtitleWarning: false,
        isBuffering: false,
      }),
    ).toBe(true)

    expect(
      shouldShowImmersiveOverlay({
        hasInteractionWarning: false,
        hasMultipleSources: false,
        hasPlaybackSyncError: false,
        hasPlayerError: false,
        hasSubtitleWarning: false,
        isBuffering: false,
      }),
    ).toBe(false)
  })
})
