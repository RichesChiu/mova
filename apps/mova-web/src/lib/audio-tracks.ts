import type { AudioTrack } from '../api/types'
import { translateCurrent } from '../i18n'

const normalizeTrackLanguageToken = (language: string | null | undefined) =>
  language?.split(/[-_]/)[0]?.toLowerCase() ?? 'und'

const resolveTrackLanguageLabel = (language: string | null | undefined) => {
  switch (normalizeTrackLanguageToken(language)) {
    case 'zh':
      return translateCurrent('Chinese')
    case 'en':
      return translateCurrent('English')
    case 'ja':
      return translateCurrent('Japanese')
    case 'ko':
      return translateCurrent('Korean')
    case 'fr':
      return translateCurrent('French')
    case 'de':
      return translateCurrent('German')
    case 'es':
      return translateCurrent('Spanish')
    case 'und':
      return translateCurrent('Unknown language')
    default:
      return language?.toUpperCase() ?? translateCurrent('Unknown language')
  }
}

export const formatAudioTrackLabel = (audioTrack: AudioTrack) =>
  audioTrack.label?.trim() ||
  resolveTrackLanguageLabel(audioTrack.language) ||
  `Track ${audioTrack.stream_index}`

export const formatAudioTrackMeta = (audioTrack: AudioTrack) =>
  [
    audioTrack.audio_codec?.toUpperCase() ?? null,
    audioTrack.is_default
      ? translateCurrent('Default in source')
      : translateCurrent('Embedded alternate'),
  ]
    .filter(Boolean)
    .join(' · ')

export const describeAudioTrackSelection = (audioTrack: AudioTrack | null) =>
  audioTrack ? formatAudioTrackLabel(audioTrack) : translateCurrent('Original default track')

export const buildAudioTrackSwitchingMessage = (audioTrack: AudioTrack | null) =>
  audioTrack
    ? translateCurrent('Switching audio to {{name}}…', { name: formatAudioTrackLabel(audioTrack) })
    : translateCurrent('Switching audio back to the original default track…')

export const buildAudioTrackReadyMessage = (audioTrack: AudioTrack | null) =>
  audioTrack
    ? translateCurrent('Audio switched to {{name}}.', { name: formatAudioTrackLabel(audioTrack) })
    : translateCurrent('Audio switched back to the original default track.')

export const buildAudioTrackLoadErrorMessage = () =>
  translateCurrent('Audio tracks could not be loaded. Playback will stay on the current audio.')
