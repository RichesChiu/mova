import type { PointerEvent as ReactPointerEvent } from 'react'
import type { MediaFile, SubtitleFile } from '../../api/types'
import { translateCurrent } from '../../i18n'
import { formatDuration } from '../../lib/format'

export const isInteractiveKeyboardTarget = (target: EventTarget | null) => {
  if (!(target instanceof HTMLElement)) {
    return false
  }

  const tagName = target.tagName.toLowerCase()
  return (
    target.isContentEditable ||
    tagName === 'button' ||
    tagName === 'input' ||
    tagName === 'select' ||
    tagName === 'textarea'
  )
}

export const releasePointerButtonFocus = (event: ReactPointerEvent<HTMLElement>) => {
  if (!(event.target instanceof Element)) {
    return
  }

  const button = event.target.closest('button')
  if (button instanceof HTMLButtonElement && event.currentTarget.contains(button)) {
    button.blur()
  }
}

export const formatVideoMeta = (file: MediaFile) => {
  const parts = [file.container?.toUpperCase()]

  if (file.width && file.height) {
    parts.push(`${file.width}×${file.height}`)
  }

  if (file.duration_seconds) {
    parts.push(formatDuration(file.duration_seconds))
  }

  return parts.filter(Boolean).join(' · ')
}

export const normalizeSubtitleTrackLanguage = (language: string | null | undefined) =>
  language?.split(/[-_]/)[0]?.toLowerCase() || 'und'

export const renderSubtitleLabel = (subtitle: SubtitleFile) => {
  const languageLabel = (() => {
    switch (subtitle.language?.toLowerCase()) {
      case 'zh':
      case 'zh-cn':
        return translateCurrent('Chinese')
      case 'zh-tw':
        return translateCurrent('Traditional Chinese')
      case 'en':
        return translateCurrent('English')
      case 'ja':
        return translateCurrent('Japanese')
      case 'ko':
        return translateCurrent('Korean')
      default:
        return subtitle.language?.toUpperCase() ?? null
    }
  })()

  return [languageLabel, subtitle.label, subtitle.is_forced ? translateCurrent('Forced') : null]
    .filter(Boolean)
    .join(' · ')
}

export const measureBufferedSeconds = (video: HTMLVideoElement) => {
  let maxBufferedEnd = 0

  for (let index = 0; index < video.buffered.length; index += 1) {
    const rangeStart = video.buffered.start(index)
    const rangeEnd = video.buffered.end(index)

    if (video.currentTime >= rangeStart && video.currentTime <= rangeEnd) {
      return Math.round(rangeEnd)
    }

    maxBufferedEnd = Math.max(maxBufferedEnd, rangeEnd)
  }

  return Math.round(maxBufferedEnd)
}

export const forceSelectedTextTrack = (video: HTMLVideoElement, shouldShowSubtitle: boolean) => {
  const tracks = Array.from(video.textTracks)
  tracks.forEach((track) => {
    track.mode = 'disabled'
  })

  if (shouldShowSubtitle && tracks[0]) {
    tracks[0].mode = 'showing'
  }
}

export const buildPlaybackSourceErrorMessage = (video: HTMLVideoElement | null) => {
  const errorCode = video?.error?.code

  switch (errorCode) {
    case 1:
      return translateCurrent(
        'Playback was interrupted before the file finished loading. Try again.',
      )
    case 2:
      return translateCurrent(
        'The selected file could not be streamed. Check the storage mount or network path.',
      )
    case 3:
      return translateCurrent(
        'This browser could not decode the selected file. Try another version or container.',
      )
    case 4:
      return translateCurrent('This browser does not support the selected video format.')
    default:
      return translateCurrent(
        'This browser could not play the selected file. Try another version or container.',
      )
  }
}
