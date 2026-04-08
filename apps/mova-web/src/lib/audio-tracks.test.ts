import { describe, expect, it } from 'vitest'
import { formatAudioTrackLabel, formatAudioTrackMeta } from './audio-tracks'

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
    expect(
      formatAudioTrackLabel({
        id: 2,
        media_file_id: 10,
        stream_index: 3,
        language: 'en',
        audio_codec: 'ac3',
        label: null,
        is_default: false,
        created_at: '',
        updated_at: '',
      }),
    ).toBe('English')
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
    ).toBe('AAC · Default')
  })
})
