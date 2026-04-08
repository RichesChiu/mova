const asNamedError = (error: unknown) =>
  error && typeof error === 'object'
    ? {
        message:
          typeof (error as { message?: unknown }).message === 'string'
            ? (error as { message: string }).message
            : null,
        name:
          typeof (error as { name?: unknown }).name === 'string'
            ? (error as { name: string }).name
            : null,
      }
    : {
        message: null,
        name: null,
      }

export const buildPlaybackInteractionWarningMessage = (error: unknown) => {
  const namedError = asNamedError(error)

  switch (namedError.name) {
    case 'AbortError':
      return 'Playback was interrupted before it could start. Click play again to continue.'
    case 'NotAllowedError':
      return 'Autoplay was blocked by the browser. Click play again to continue.'
    case 'NotSupportedError':
      return 'This browser could not start playback for the selected file.'
    default:
      return 'Playback could not start automatically. Click play again to continue.'
  }
}

export const buildFullscreenWarningMessage = (error?: unknown) => {
  if (!error) {
    return 'Fullscreen is not available in this browser or app environment.'
  }

  const namedError = asNamedError(error)

  switch (namedError.name) {
    case 'NotAllowedError':
      return 'Fullscreen was blocked by the browser or app window. Try the browser fullscreen control.'
    case 'NotSupportedError':
      return 'Fullscreen is not available in this browser or app environment.'
    default:
      return 'Fullscreen could not be enabled right now. Try again or use the browser fullscreen control.'
  }
}

export const shouldShowImmersiveOverlay = ({
  hasInteractionWarning,
  hasMultipleSources,
  hasPlaybackSyncError,
  hasPlayerError,
  hasSubtitleWarning,
  isBuffering,
}: {
  hasInteractionWarning: boolean
  hasMultipleSources: boolean
  hasPlaybackSyncError: boolean
  hasPlayerError: boolean
  hasSubtitleWarning: boolean
  isBuffering: boolean
}) =>
  hasMultipleSources ||
  isBuffering ||
  hasPlayerError ||
  hasInteractionWarning ||
  hasPlaybackSyncError ||
  hasSubtitleWarning
