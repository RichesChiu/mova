import type { AudioTrack } from '../api/types'

const normalizeTrackLanguageToken = (language: string | null | undefined) =>
  language?.split(/[-_]/)[0]?.toLowerCase() ?? 'und'

const resolveTrackLanguageLabel = (language: string | null | undefined) => {
  switch (normalizeTrackLanguageToken(language)) {
    case 'zh':
      return 'Chinese'
    case 'en':
      return 'English'
    case 'ja':
      return 'Japanese'
    case 'ko':
      return 'Korean'
    case 'fr':
      return 'French'
    case 'de':
      return 'German'
    case 'es':
      return 'Spanish'
    case 'und':
      return 'Unknown language'
    default:
      return language?.toUpperCase() ?? 'Unknown language'
  }
}

export const formatAudioTrackLabel = (audioTrack: AudioTrack) =>
  audioTrack.label?.trim() ||
  resolveTrackLanguageLabel(audioTrack.language) ||
  `Track ${audioTrack.stream_index}`

export const formatAudioTrackMeta = (audioTrack: AudioTrack) =>
  [
    audioTrack.audio_codec?.toUpperCase() ?? null,
    audioTrack.is_default ? 'Default in source' : 'Embedded alternate',
  ]
    .filter(Boolean)
    .join(' · ')

export const describeAudioTrackSelection = (audioTrack: AudioTrack | null) =>
  audioTrack ? formatAudioTrackLabel(audioTrack) : 'Original default track'

export const buildAudioTrackSwitchingMessage = (audioTrack: AudioTrack | null) =>
  audioTrack
    ? `Switching audio to ${formatAudioTrackLabel(audioTrack)}…`
    : 'Switching audio back to the original default track…'

export const buildAudioTrackReadyMessage = (audioTrack: AudioTrack | null) =>
  audioTrack
    ? `Audio switched to ${formatAudioTrackLabel(audioTrack)}.`
    : 'Audio switched back to the original default track.'

export const buildAudioTrackLoadErrorMessage = () =>
  'Audio tracks could not be loaded. Playback will stay on the current audio.'
