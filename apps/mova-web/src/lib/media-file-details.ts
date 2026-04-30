import type { AudioTrack, MediaFile, SubtitleFile } from '../api/types'
import { getCurrentInterfaceLanguage, translateCurrent } from '../i18n'
import { formatBytes, formatDuration } from './format'

type MediaFileFact = {
  label: string
  value: string
}

export type MediaFileTrackOption = {
  label: string
  value: string
}

const CODEC_LABELS: Record<string, string> = {
  av1: 'AV1',
  avc: 'AVC',
  hevc: 'HEVC',
  h264: 'H.264',
  'h.264': 'H.264',
  x264: 'x264',
  h265: 'H.265',
  'h.265': 'H.265',
  x265: 'x265',
  vp9: 'VP9',
  aac: 'AAC',
  ac3: 'AC3',
  'ac-3': 'AC3',
  eac3: 'EAC3',
  'e-ac-3': 'EAC3',
  truehd: 'TrueHD',
  dts: 'DTS',
  flac: 'FLAC',
  opus: 'Opus',
  mp3: 'MP3',
  pcm: 'PCM',
  pgs: 'PGS',
  srt: 'SUBRIP',
  subrip: 'SUBRIP',
  ass: 'ASS',
  ssa: 'SSA',
  mov_text: 'MOV_TEXT',
  vtt: 'VTT',
}

const LANGUAGE_LABELS: Record<string, string> = {
  zh: 'Chinese',
  en: 'English',
  ja: 'Japanese',
  ko: 'Korean',
  fr: 'French',
  de: 'German',
  es: 'Spanish',
}

const normalizeCodecToken = (codec: string | null | undefined) => codec?.trim().toLowerCase() ?? ''

const normalizeLanguageToken = (language: string | null | undefined) =>
  language?.split(/[-_]/)[0]?.trim().toLowerCase() ?? ''

const formatCodecLabel = (codec: string | null | undefined) => {
  const normalized = normalizeCodecToken(codec)

  if (!normalized) {
    return '—'
  }

  return CODEC_LABELS[normalized] ?? codec?.trim().toUpperCase() ?? '—'
}

const formatLanguageLabel = (language: string | null | undefined) => {
  const normalized = normalizeLanguageToken(language)

  if (!normalized) {
    return translateCurrent('Unknown')
  }

  return (
    (LANGUAGE_LABELS[normalized] ? translateCurrent(LANGUAGE_LABELS[normalized]) : null) ??
    language?.trim().toUpperCase() ??
    translateCurrent('Unknown')
  )
}

const extractFileName = (filePath: string) => {
  const tokens = filePath.split(/[\\/]/).filter(Boolean)
  return tokens[tokens.length - 1] ?? filePath
}

const detectDolbyFlags = (file: Pick<MediaFile, 'audio_codec' | 'file_path'>) => {
  const normalizedCodec = normalizeCodecToken(file.audio_codec)
  const normalizedPath = file.file_path.toLowerCase()
  const hasDolbyAudio =
    ['ac3', 'ac-3', 'eac3', 'e-ac-3', 'truehd'].includes(normalizedCodec) ||
    /\b(dolby|ddp|dd\+|atmos|truehd|eac3|ac3)\b/i.test(normalizedPath)
  const hasDolbyVision = /\b(dolby[ ._-]?vision|dovi)\b/i.test(normalizedPath)

  return {
    hasDolbyAudio,
    hasDolbyVision,
  }
}

const normalizeChannelLayout = (layout: string | null | undefined) => {
  const trimmed = layout?.trim()

  if (!trimmed) {
    return '—'
  }

  return trimmed
    .replace('(side)', '')
    .replace('(back)', '')
    .replace(/^stereo$/i, '2.0')
    .replace(/^mono$/i, '1.0')
}

const formatFrameRate = (value: number | null | undefined) => {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return '—'
  }

  return `${Number(value.toFixed(value >= 100 ? 0 : value >= 10 ? 2 : 3))} fps`
}

const formatSampleRate = (value: number | null | undefined) => {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return '—'
  }

  return `${new Intl.NumberFormat(getCurrentInterfaceLanguage()).format(value)} Hz`
}

const formatChannelCount = (value: number | null | undefined) => {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return '—'
  }

  return `${value} ch`
}

const formatBitDepth = (value: number | null | undefined) => {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return '—'
  }

  return `${value}-bit`
}

const formatBooleanLabel = (value: boolean) =>
  value ? translateCurrent('Yes') : translateCurrent('No')

const formatVideoStreamTitle = (file: MediaFile) =>
  file.video_title?.trim() || translateCurrent('Video Stream')

export const getMediaFileDisplayName = (filePath: string) => extractFileName(filePath)

export const buildMediaVersionOptions = (mediaFiles: MediaFile[]): MediaFileTrackOption[] =>
  mediaFiles.map((file, index) => {
    const displayName = getMediaFileDisplayName(file.file_path)
    const meta = [
      file.container?.trim() ? file.container.trim().toUpperCase() : null,
      file.width && file.height ? `${file.width} × ${file.height}` : null,
      formatMediaFileBitrate(file.bitrate),
    ]
      .filter((value) => value && value !== '—')
      .join(' · ')

    return {
      label: [
        displayName || translateCurrent('Version {{index}}', { index: index + 1 }),
        meta,
      ]
        .filter(Boolean)
        .join(' · '),
      value: String(file.id),
    }
  })

export const formatMediaFileResolution = (file: Pick<MediaFile, 'width' | 'height'>) => {
  if (!file.width || !file.height) {
    return '—'
  }

  return `${file.width} × ${file.height}`
}

export const formatMediaFileBitrate = (bitrate: number | null | undefined) => {
  if (typeof bitrate !== 'number' || !Number.isFinite(bitrate) || bitrate <= 0) {
    return '—'
  }

  if (bitrate >= 1_000_000) {
    const megabits = bitrate / 1_000_000
    return `${megabits.toFixed(megabits >= 10 ? 0 : 1)} Mb/s`
  }

  return `${Math.round(bitrate / 1_000)} kb/s`
}

export const formatMediaFileDolby = (file: Pick<MediaFile, 'audio_codec' | 'file_path'>) => {
  const { hasDolbyAudio, hasDolbyVision } = detectDolbyFlags(file)

  if (hasDolbyAudio && hasDolbyVision) {
    return 'Dolby Audio · Dolby Vision'
  }

  if (hasDolbyAudio) {
    return 'Dolby Audio'
  }

  if (hasDolbyVision) {
    return 'Dolby Vision'
  }

  return translateCurrent('No Dolby markers found')
}

export const buildMediaFileFeatureBadges = (file: Pick<MediaFile, 'audio_codec' | 'file_path'>) => {
  const { hasDolbyAudio, hasDolbyVision } = detectDolbyFlags(file)

  return [
    hasDolbyAudio ? 'Dolby Audio' : null,
    hasDolbyVision ? 'Dolby Vision' : null,
  ].filter((badge): badge is string => Boolean(badge))
}

export const buildMediaSourceFacts = (file: MediaFile): MediaFileFact[] => [
  {
    label: translateCurrent('File Size'),
    value: formatBytes(file.file_size, getCurrentInterfaceLanguage()),
  },
  {
    label: translateCurrent('Duration'),
    value: formatDuration(file.duration_seconds, getCurrentInterfaceLanguage()),
  },
  {
    label: translateCurrent('Overall Bitrate'),
    value: formatMediaFileBitrate(file.bitrate),
  },
  {
    label: translateCurrent('Dolby'),
    value: formatMediaFileDolby(file),
  },
]

export const buildVideoCardFacts = (file: MediaFile): MediaFileFact[] => [
  {
    label: translateCurrent('Title'),
    value: formatVideoStreamTitle(file),
  },
  {
    label: translateCurrent('Container'),
    value: file.container?.trim() ? file.container.trim().toUpperCase() : '—',
  },
  {
    label: translateCurrent('Codec'),
    value: formatCodecLabel(file.video_codec),
  },
  {
    label: translateCurrent('Profile'),
    value: file.video_profile?.trim() || '—',
  },
  {
    label: translateCurrent('Level'),
    value: file.video_level?.trim() || '—',
  },
  {
    label: translateCurrent('Resolution'),
    value: formatMediaFileResolution(file),
  },
  {
    label: translateCurrent('Aspect Ratio'),
    value: file.video_aspect_ratio?.trim() || '—',
  },
  {
    label: translateCurrent('Scan Type'),
    value: file.video_scan_type?.trim() || '—',
  },
  {
    label: translateCurrent('Frame Rate'),
    value: formatFrameRate(file.video_frame_rate),
  },
  {
    label: translateCurrent('Bitrate'),
    value: formatMediaFileBitrate(file.video_bitrate ?? file.bitrate),
  },
  {
    label: translateCurrent('Color Primaries'),
    value: file.video_color_primaries?.trim() || '—',
  },
  {
    label: translateCurrent('Color Space'),
    value: file.video_color_space?.trim() || '—',
  },
  {
    label: translateCurrent('Color Transfer'),
    value: file.video_color_transfer?.trim() || '—',
  },
  {
    label: translateCurrent('Bit Depth'),
    value: formatBitDepth(file.video_bit_depth),
  },
  {
    label: translateCurrent('Pixel Format'),
    value: file.video_pixel_format?.trim() || '—',
  },
  {
    label: translateCurrent('Reference Frames'),
    value:
      typeof file.video_reference_frames === 'number' &&
      Number.isFinite(file.video_reference_frames)
        ? String(file.video_reference_frames)
        : '—',
  },
]

export const buildVideoTrackOptions = (file: MediaFile): MediaFileTrackOption[] => [
  {
    label: getMediaFileDisplayName(file.file_path),
    value: String(file.id),
  },
]

export const buildAudioTrackTitle = (audioTrack: AudioTrack) => {
  const summary = [
    formatLanguageLabel(audioTrack.language),
    formatCodecLabel(audioTrack.audio_codec),
    normalizeChannelLayout(audioTrack.channel_layout),
  ]
    .filter((value) => value && value !== '—')
    .join(' ')

  const base =
    summary ||
    audioTrack.label?.trim() ||
    translateCurrent('Track {{index}}', { index: audioTrack.stream_index })
  return audioTrack.is_default ? `${base} (${translateCurrent('Default')})` : base
}

export const buildAudioTrackOptions = (audioTracks: AudioTrack[]): MediaFileTrackOption[] =>
  audioTracks.map((audioTrack) => ({
    label: buildAudioTrackTitle(audioTrack),
    value: String(audioTrack.id),
  }))

export const buildAudioTrackFacts = (audioTrack: AudioTrack): MediaFileFact[] => [
  {
    label: translateCurrent('Title'),
    value: buildAudioTrackTitle(audioTrack),
  },
  {
    label: translateCurrent('Language'),
    value: formatLanguageLabel(audioTrack.language),
  },
  {
    label: translateCurrent('Codec'),
    value: formatCodecLabel(audioTrack.audio_codec),
  },
  {
    label: translateCurrent('Layout'),
    value: normalizeChannelLayout(audioTrack.channel_layout),
  },
  {
    label: translateCurrent('Channels'),
    value: formatChannelCount(audioTrack.channels),
  },
  {
    label: translateCurrent('Bitrate'),
    value: formatMediaFileBitrate(audioTrack.bitrate),
  },
  {
    label: translateCurrent('Sample Rate'),
    value: formatSampleRate(audioTrack.sample_rate),
  },
  {
    label: translateCurrent('Default'),
    value: formatBooleanLabel(audioTrack.is_default),
  },
]

export const buildSubtitleTrackTitle = (subtitle: SubtitleFile, index: number) => {
  const language = formatLanguageLabel(subtitle.language)
  const codec = formatCodecLabel(subtitle.subtitle_format)

  if (language !== 'Unknown' && codec !== '—') {
    return `${language} (${codec})`
  }

  if (subtitle.label?.trim()) {
    return subtitle.label.trim()
  }

  if (codec !== '—') {
    return `${translateCurrent('Subtitle {{index}}', { index: index + 1 })} (${codec})`
  }

  return translateCurrent('Subtitle {{index}}', { index: index + 1 })
}

export const buildSubtitleTrackOptions = (subtitles: SubtitleFile[]): MediaFileTrackOption[] =>
  subtitles.map((subtitle, index) => ({
    label: buildSubtitleTrackTitle(subtitle, index),
    value: String(subtitle.id),
  }))

export const buildSubtitleTrackFacts = (subtitle: SubtitleFile, index: number): MediaFileFact[] => [
  {
    label: translateCurrent('Title'),
    value: buildSubtitleTrackTitle(subtitle, index),
  },
  {
    label: translateCurrent('Track Label'),
    value: subtitle.label?.trim() || '—',
  },
  {
    label: translateCurrent('Language'),
    value: formatLanguageLabel(subtitle.language),
  },
  {
    label: translateCurrent('Codec'),
    value: formatCodecLabel(subtitle.subtitle_format),
  },
  {
    label: translateCurrent('Default'),
    value: formatBooleanLabel(subtitle.is_default),
  },
  {
    label: translateCurrent('Forced'),
    value: formatBooleanLabel(subtitle.is_forced),
  },
  {
    label: translateCurrent('Hearing Impaired'),
    value: formatBooleanLabel(subtitle.is_hearing_impaired),
  },
  {
    label: translateCurrent('External'),
    value: formatBooleanLabel(subtitle.source_kind === 'external'),
  },
]
