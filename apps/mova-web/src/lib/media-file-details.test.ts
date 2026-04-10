import { describe, expect, it } from 'vitest'
import {
  buildAudioTrackFacts,
  buildAudioTrackOptions,
  buildAudioTrackTitle,
  buildMediaFileFeatureBadges,
  buildMediaSourceFacts,
  buildSubtitleTrackFacts,
  buildSubtitleTrackOptions,
  buildSubtitleTrackTitle,
  buildVideoCardFacts,
  buildVideoTrackOptions,
  formatMediaFileBitrate,
  formatMediaFileDolby,
  formatMediaFileResolution,
  getMediaFileDisplayName,
} from './media-file-details'

const sampleFile = {
  id: 11,
  media_item_id: 21,
  file_path: '/media/movies/Dune (2021)/Dune (2021) [DDP Atmos][DoVi].mkv',
  container: 'mkv',
  file_size: 16_987_654_321,
  duration_seconds: 9300,
  video_title: 'Main Video',
  video_codec: 'hevc',
  video_profile: 'Main 10',
  video_level: '5.1',
  audio_codec: 'eac3',
  width: 3840,
  height: 2160,
  bitrate: 18_500_000,
  video_bitrate: 17_900_000,
  video_frame_rate: 23.976,
  video_aspect_ratio: '16:9',
  video_scan_type: 'Progressive',
  video_color_primaries: 'bt2020',
  video_color_space: 'bt2020nc',
  video_color_transfer: 'smpte2084',
  video_bit_depth: 10,
  video_pixel_format: 'yuv420p10le',
  video_reference_frames: 4,
  scan_hash: null,
  created_at: '',
  updated_at: '',
}

describe('media file detail helpers', () => {
  it('extracts a display name from the file path', () => {
    expect(getMediaFileDisplayName(sampleFile.file_path)).toBe('Dune (2021) [DDP Atmos][DoVi].mkv')
  })

  it('formats resolution and bitrate for human-friendly display', () => {
    expect(formatMediaFileResolution(sampleFile)).toBe('3840 × 2160')
    expect(formatMediaFileBitrate(sampleFile.bitrate)).toBe('19 Mb/s')
    expect(formatMediaFileBitrate(820_000)).toBe('820 kb/s')
  })

  it('detects Dolby markers conservatively from codec and path', () => {
    expect(formatMediaFileDolby(sampleFile)).toBe('Dolby Audio · Dolby Vision')
    expect(
      formatMediaFileDolby({
        audio_codec: 'aac',
        file_path: '/media/movies/Plain Title/plain-title.mkv',
      }),
    ).toBe('No Dolby markers found')
  })

  it('builds source badges and video facts for the details view', () => {
    expect(buildMediaFileFeatureBadges(sampleFile)).toEqual(['MKV', 'Dolby Audio', 'Dolby Vision'])

    expect(buildMediaSourceFacts(sampleFile)).toContainEqual({
      label: 'File Size',
      value: '16 GB',
    })
    expect(buildVideoCardFacts(sampleFile)).toContainEqual({
      label: 'Codec',
      value: 'HEVC',
    })
    expect(buildVideoCardFacts(sampleFile)).toContainEqual({
      label: 'Bit Depth',
      value: '10-bit',
    })
  })

  it('builds audio track summaries and facts', () => {
    const audioTrack = {
      id: 1,
      media_file_id: 11,
      stream_index: 2,
      language: 'en',
      audio_codec: 'eac3',
      label: 'English EAC3 5.1',
      channel_layout: '5.1(side)',
      channels: 6,
      bitrate: 768_000,
      sample_rate: 48_000,
      is_default: true,
      created_at: '',
      updated_at: '',
    }

    expect(buildAudioTrackTitle(audioTrack)).toBe('English EAC3 5.1 (Default)')
    expect(buildAudioTrackOptions([audioTrack])).toEqual([
      {
        label: 'English EAC3 5.1 (Default)',
        value: '1',
      },
    ])
    expect(buildAudioTrackFacts(audioTrack)).toContainEqual({
      label: 'Title',
      value: 'English EAC3 5.1 (Default)',
    })
    expect(buildAudioTrackFacts(audioTrack)).toContainEqual({
      label: 'Channels',
      value: '6 ch',
    })
    expect(buildAudioTrackFacts(audioTrack)).toContainEqual({
      label: 'Sample Rate',
      value: '48,000 Hz',
    })
  })

  it('builds subtitle summaries and facts', () => {
    const subtitle = {
      id: 2,
      media_file_id: 11,
      source_kind: 'embedded',
      file_path: null,
      stream_index: 6,
      language: 'en',
      subtitle_format: 'pgs',
      label: 'SDH',
      is_default: false,
      is_forced: true,
      is_hearing_impaired: true,
      created_at: '',
      updated_at: '',
    }

    expect(buildSubtitleTrackTitle(subtitle, 0)).toBe('English (PGS)')
    expect(buildSubtitleTrackOptions([subtitle])).toEqual([
      {
        label: 'English (PGS)',
        value: '2',
      },
    ])
    expect(buildSubtitleTrackFacts(subtitle, 0)).toContainEqual({
      label: 'Track Label',
      value: 'SDH',
    })
    expect(buildSubtitleTrackFacts(subtitle, 0)).toContainEqual({
      label: 'External',
      value: 'No',
    })
    expect(buildSubtitleTrackFacts(subtitle, 0)).toContainEqual({
      label: 'Hearing Impaired',
      value: 'Yes',
    })
  })

  it('builds a single video option for the source selector', () => {
    expect(buildVideoTrackOptions(sampleFile)).toEqual([
      {
        label: 'Dune (2021) [DDP Atmos][DoVi].mkv',
        value: '11',
      },
    ])
  })
})
