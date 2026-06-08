import { describe, expect, it } from 'vitest'
import {
  buildAudioTrackLoadErrorMessage,
  buildAudioTrackReadyMessage,
  buildAudioTrackSwitchingMessage,
  describeAudioTrackSelection,
  formatAudioTrackLabel,
  formatAudioTrackMeta,
} from './audio-tracks'

const englishTrack = {
  id: 2,
  media_file_id: 10,
  stream_index: 3,
  language: 'en',
  audio_codec: 'ac3',
  label: null,
  is_default: false,
  created_at: '',
  updated_at: '',
}

describe('audio track helpers', () => {
  it('prefers explicit labels when available', () => {
    expect(
      formatAudioTrackLabel({
        id: 1,
        media_file_id: 10,
        stream_index: 2,
        language: 'zh-CN',
        audio_codec: 'aac',
        label: 'Mandarin Stereo',
        is_default: true,
        created_at: '',
        updated_at: '',
      }),
    ).toBe('Mandarin Stereo')
  })

  it('falls back to human readable language labels', () => {
    expect(formatAudioTrackLabel(englishTrack)).toBe('English')
  })

  it('includes codec and default marker in track meta', () => {
    expect(
      formatAudioTrackMeta({
        id: 3,
        media_file_id: 10,
        stream_index: 4,
        language: null,
        audio_codec: 'aac',
        label: null,
        is_default: true,
        created_at: '',
        updated_at: '',
      }),
    ).toBe('AAC · Default in source')
  })

  it('describes the current selection and switch feedback copy', () => {
    expect(describeAudioTrackSelection(englishTrack)).toBe('English')
    expect(describeAudioTrackSelection(null)).toBe('Original default track')
    expect(buildAudioTrackSwitchingMessage(englishTrack)).toBe('Switching audio to English…')
    expect(buildAudioTrackReadyMessage(englishTrack)).toBe('Audio switched to English.')
    expect(buildAudioTrackSwitchingMessage(null)).toBe(
      'Switching audio back to the original default track…',
    )
    expect(buildAudioTrackReadyMessage(null)).toBe(
      'Audio switched back to the original default track.',
    )
  })

  it('returns a user-facing fallback for audio track query errors', () => {
    expect(buildAudioTrackLoadErrorMessage()).toBe(
      'Audio tracks could not be loaded. Playback will stay on the current audio.',
    )
  })
})
