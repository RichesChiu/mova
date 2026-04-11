import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { type CSSProperties, useEffect, useRef, useState } from 'react'
import {
  flushMediaItemPlaybackProgress,
  getMediaItemPlaybackProgress,
  listMediaFileAudioTracks,
  listMediaFileSubtitles,
  listMediaItemFiles,
  mediaFileStreamUrl,
  subtitleFileStreamUrl,
  updateMediaItemPlaybackProgress,
} from '../../api/client'
import type { MediaFile, SubtitleFile } from '../../api/types'
import {
  buildAudioTrackLoadErrorMessage,
  buildAudioTrackReadyMessage,
  buildAudioTrackSwitchingMessage,
  describeAudioTrackSelection,
  formatAudioTrackLabel,
  formatAudioTrackMeta,
} from '../../lib/audio-tracks'
import { formatDuration } from '../../lib/format'
import { shouldMarkPlaybackFinished } from '../../lib/playback'
import {
  buildFullscreenWarningMessage,
  buildPlaybackInteractionWarningMessage,
  shouldShowImmersiveOverlay,
} from '../../lib/player-feedback'

const PROGRESS_SYNC_INTERVAL_SECONDS = 5
const PLAYBACK_PROGRESS_SAVE_ERROR =
  'Playback progress could not be saved. We will retry on the next sync.'
const SUBTITLE_LOAD_ERROR =
  'The selected subtitle could not be loaded. Playback will continue without subtitles.'

interface MediaPlayerPanelProps {
  episodeSwitchOptions?: Array<{
    label: string
    mediaItemId: number
  }>
  mediaItemId: number
  onSelectEpisode?: (mediaItemId: number) => void
  preferredMediaFileId?: number | null
  title: string
  startMode?: 'resume' | 'from-start'
  variant?: 'panel' | 'immersive'
}

interface PendingPlaybackRestore {
  positionSeconds: number
  shouldAutoplay: boolean
  shouldPersistSelection: boolean
}

const formatVideoMeta = (file: MediaFile) => {
  const parts = [file.container?.toUpperCase()]

  if (file.width && file.height) {
    parts.push(`${file.width}×${file.height}`)
  }

  if (file.duration_seconds) {
    parts.push(formatDuration(file.duration_seconds))
  }

  return parts.filter(Boolean).join(' · ')
}

const SpeakerIcon = ({ muted, volume }: { muted: boolean; volume: number }) => {
  if (muted || volume === 0) {
    return (
      <svg
        aria-hidden="true"
        className="player-control-button__glyph"
        fill="none"
        viewBox="0 0 24 24"
      >
        <path
          d="M5 10H8L12 6V18L8 14H5V10Z"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="1.8"
        />
        <path
          d="M16 9L20 15"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="1.8"
        />
        <path
          d="M20 9L16 15"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="1.8"
        />
      </svg>
    )
  }

  return (
    <svg
      aria-hidden="true"
      className="player-control-button__glyph"
      fill="none"
      viewBox="0 0 24 24"
    >
      <path
        d="M5 10H8L12 6V18L8 14H5V10Z"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
      <path
        d="M15.5 9.5C16.3 10.1 16.8 11.01 16.8 12C16.8 12.99 16.3 13.9 15.5 14.5"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
      {volume >= 0.5 ? (
        <path
          d="M18.3 7C19.72 8.24 20.6 10.05 20.6 12C20.6 13.95 19.72 15.76 18.3 17"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="1.8"
        />
      ) : null}
    </svg>
  )
}

const FullscreenIcon = () => {
  return (
    <svg
      aria-hidden="true"
      className="player-control-button__glyph"
      fill="none"
      viewBox="0 0 24 24"
    >
      <path
        d="M9 4H5V8"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
      <path
        d="M15 4H19V8"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
      <path
        d="M9 20H5V16"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
      <path
        d="M15 20H19V16"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
    </svg>
  )
}

const PlayIcon = () => {
  return (
    <svg
      aria-hidden="true"
      className="player-control-button__glyph"
      fill="none"
      viewBox="0 0 24 24"
    >
      <path
        d="M8 6.5L17 12L8 17.5V6.5Z"
        fill="currentColor"
        stroke="currentColor"
        strokeLinejoin="round"
        strokeWidth="1.2"
      />
    </svg>
  )
}

const PauseIcon = () => {
  return (
    <svg
      aria-hidden="true"
      className="player-control-button__glyph"
      fill="none"
      viewBox="0 0 24 24"
    >
      <path d="M8.5 6.5V17.5" stroke="currentColor" strokeLinecap="round" strokeWidth="2.2" />
      <path d="M15.5 6.5V17.5" stroke="currentColor" strokeLinecap="round" strokeWidth="2.2" />
    </svg>
  )
}

const SeekBackIcon = () => {
  return (
    <svg
      aria-hidden="true"
      className="player-control-button__glyph"
      fill="none"
      viewBox="0 0 24 24"
    >
      <path
        d="M11 7L6 12L11 17"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.9"
      />
      <path
        d="M18 7L13 12L18 17"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.9"
      />
    </svg>
  )
}

const SeekForwardIcon = () => {
  return (
    <svg
      aria-hidden="true"
      className="player-control-button__glyph"
      fill="none"
      viewBox="0 0 24 24"
    >
      <path
        d="M13 7L18 12L13 17"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.9"
      />
      <path
        d="M6 7L11 12L6 17"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.9"
      />
    </svg>
  )
}

const SubtitleIcon = () => {
  return (
    <svg
      aria-hidden="true"
      className="player-control-button__glyph"
      fill="none"
      viewBox="0 0 24 24"
    >
      <rect height="12" rx="2.5" stroke="currentColor" strokeWidth="1.8" width="18" x="3" y="6" />
      <path d="M7 11H11" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" />
      <path d="M7 14H14" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" />
      <path
        d="M16.5 11.5L18 13L16.5 14.5"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
    </svg>
  )
}

const AudioTrackIcon = () => {
  return (
    <svg
      aria-hidden="true"
      className="player-control-button__glyph"
      fill="none"
      viewBox="0 0 24 24"
    >
      <path
        d="M5 10H8L12 6V18L8 14H5V10Z"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
      <path
        d="M15.5 9.5C16.3 10.1 16.8 11.01 16.8 12C16.8 12.99 16.3 13.9 15.5 14.5"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
      <path
        d="M18.3 7C19.72 8.24 20.6 10.05 20.6 12C20.6 13.95 19.72 15.76 18.3 17"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
    </svg>
  )
}

const EpisodeSwitchIcon = () => {
  return (
    <svg
      aria-hidden="true"
      className="player-control-button__glyph"
      fill="none"
      viewBox="0 0 24 24"
    >
      <rect height="14" rx="2.5" stroke="currentColor" strokeWidth="1.8" width="18" x="3" y="5" />
      <path d="M7 9H15" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" />
      <path d="M7 12.5H13" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" />
      <path
        d="M16.5 12L18.5 14L16.5 16"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
    </svg>
  )
}

const normalizeSubtitleTrackLanguage = (language: string | null | undefined) =>
  language?.split(/[-_]/)[0]?.toLowerCase() || 'und'

const renderSubtitleLabel = (subtitle: SubtitleFile) => {
  const languageLabel = (() => {
    switch (subtitle.language?.toLowerCase()) {
      case 'zh':
      case 'zh-cn':
        return 'Chinese'
      case 'zh-tw':
        return 'Traditional Chinese'
      case 'en':
        return 'English'
      case 'ja':
        return 'Japanese'
      case 'ko':
        return 'Korean'
      default:
        return subtitle.language?.toUpperCase() ?? null
    }
  })()

  return [languageLabel, subtitle.label, subtitle.is_forced ? 'Forced' : null]
    .filter(Boolean)
    .join(' · ')
}

const measureBufferedSeconds = (video: HTMLVideoElement) => {
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

const forceSelectedTextTrack = (video: HTMLVideoElement, shouldShowSubtitle: boolean) => {
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
      return 'Playback was interrupted before the file finished loading. Try again.'
    case 2:
      return 'The selected file could not be streamed. Check the storage mount or network path.'
    case 3:
      return 'This browser could not decode the selected file. Try another version or container.'
    case 4:
      return 'This browser does not support the selected video format.'
    default:
      return 'This browser could not play the selected file. Try another version or container.'
  }
}

export const MediaPlayerPanel = ({
  episodeSwitchOptions = [],
  mediaItemId,
  onSelectEpisode,
  preferredMediaFileId = null,
  startMode = 'resume',
  title,
  variant = 'panel',
}: MediaPlayerPanelProps) => {
  const queryClient = useQueryClient()
  const stageRef = useRef<HTMLDivElement | null>(null)
  const videoRef = useRef<HTMLVideoElement | null>(null)
  const episodeMenuRef = useRef<HTMLDivElement | null>(null)
  const audioMenuRef = useRef<HTMLDivElement | null>(null)
  const subtitleMenuRef = useRef<HTMLDivElement | null>(null)
  const selectedMediaFileRef = useRef<MediaFile | null>(null)
  const audioTrackNoticeTimeoutRef = useRef<number | null>(null)
  const pendingAudioTrackSwitchRef = useRef<{
    label: string
    target: number | null
  } | null>(null)
  const previousMediaItemIdRef = useRef(mediaItemId)
  const durationSecondsRef = useRef<number | null>(null)
  const restoredForFileRef = useRef<number | null>(null)
  const shouldHonorStartModeRef = useRef(startMode === 'from-start')
  const pendingPlaybackRestoreRef = useRef<PendingPlaybackRestore | null>(null)
  const lastReportedSecondsRef = useRef(-1)
  const hasSubmittedProgressRef = useRef(false)
  const syncPlaybackProgressRef = useRef<(force?: boolean, isFinished?: boolean) => void>(() => {})
  const flushPlaybackProgressRef = useRef<() => void>(() => {})
  const [selectedMediaFileId, setSelectedMediaFileId] = useState<number | null>(null)
  const [playerError, setPlayerError] = useState<string | null>(null)
  const [interactionWarning, setInteractionWarning] = useState<string | null>(null)
  const [playbackSyncError, setPlaybackSyncError] = useState<string | null>(null)
  const [subtitleTrackError, setSubtitleTrackError] = useState<string | null>(null)
  const [audioTrackNotice, setAudioTrackNotice] = useState<string | null>(null)
  const [isBuffering, setIsBuffering] = useState(false)
  const [bufferedSeconds, setBufferedSeconds] = useState(0)
  const [positionSeconds, setPositionSeconds] = useState(0)
  const [durationSeconds, setDurationSeconds] = useState<number | null>(null)
  const [isPlaying, setIsPlaying] = useState(false)
  const [isMuted, setIsMuted] = useState(false)
  const [volume, setVolume] = useState(1)
  const [isFullscreen, setIsFullscreen] = useState(false)
  const [isEpisodeMenuOpen, setIsEpisodeMenuOpen] = useState(false)
  const [isAudioMenuOpen, setIsAudioMenuOpen] = useState(false)
  const [isSubtitleMenuOpen, setIsSubtitleMenuOpen] = useState(false)
  const [selectedAudioTrackId, setSelectedAudioTrackId] = useState<number | null>(null)
  const [selectedSubtitleId, setSelectedSubtitleId] = useState<number | null>(null)

  const mediaFilesQuery = useQuery({
    queryKey: ['media-item-files', mediaItemId],
    queryFn: () => listMediaItemFiles(mediaItemId),
    retry: false,
  })
  const playbackProgressQuery = useQuery({
    queryKey: ['media-item-playback-progress', mediaItemId],
    queryFn: () => getMediaItemPlaybackProgress(mediaItemId),
    retry: false,
  })
  const playbackProgressMutation = useMutation({
    mutationFn: (input: {
      media_file_id: number
      position_seconds: number
      duration_seconds?: number
      is_finished?: boolean
    }) => updateMediaItemPlaybackProgress(mediaItemId, input),
    onSuccess: (progress) => {
      hasSubmittedProgressRef.current = true
      lastReportedSecondsRef.current = progress.position_seconds
      setPlaybackSyncError(null)
      queryClient.setQueryData(['media-item-playback-progress', mediaItemId], progress)
    },
    onError: () => {
      setPlaybackSyncError(PLAYBACK_PROGRESS_SAVE_ERROR)
    },
  })
  const subtitleFilesQuery = useQuery({
    enabled: selectedMediaFileId !== null,
    queryKey: ['media-file-subtitles', selectedMediaFileId],
    queryFn: () => listMediaFileSubtitles(selectedMediaFileId ?? 0),
  })
  const audioTracksQuery = useQuery({
    enabled: selectedMediaFileId !== null,
    queryKey: ['media-file-audio-tracks', selectedMediaFileId],
    queryFn: () => listMediaFileAudioTracks(selectedMediaFileId ?? 0),
  })

  const mediaFiles = mediaFilesQuery.data ?? []
  const audioTracks = audioTracksQuery.data ?? []
  const subtitleFiles = subtitleFilesQuery.data ?? []
  const selectedMediaFile =
    mediaFiles.find((file) => file.id === selectedMediaFileId) ?? mediaFiles[0] ?? null
  const selectedMediaFileDuration = selectedMediaFile?.duration_seconds ?? null
  const selectedAudioTrack =
    audioTracks.find((audioTrack) => audioTrack.id === selectedAudioTrackId) ?? null
  const selectedSubtitle =
    subtitleFiles.find((subtitle) => subtitle.id === selectedSubtitleId) ?? null
  const subtitleWarning =
    subtitleTrackError ?? (subtitleFilesQuery.isError ? SUBTITLE_LOAD_ERROR : null)
  const currentAudioSelectionLabel = describeAudioTrackSelection(selectedAudioTrack)

  const clearAudioTrackNotice = () => {
    if (audioTrackNoticeTimeoutRef.current !== null) {
      window.clearTimeout(audioTrackNoticeTimeoutRef.current)
      audioTrackNoticeTimeoutRef.current = null
    }

    setAudioTrackNotice(null)
  }

  const showAudioTrackNotice = (message: string, durationMs?: number) => {
    clearAudioTrackNotice()
    setAudioTrackNotice(message)

    if (typeof durationMs === 'number' && durationMs > 0) {
      audioTrackNoticeTimeoutRef.current = window.setTimeout(() => {
        audioTrackNoticeTimeoutRef.current = null
        setAudioTrackNotice(null)
      }, durationMs)
    }
  }

  const resetTransientPlayerFeedback = ({
    keepBuffering = false,
  }: {
    keepBuffering?: boolean
  } = {}) => {
    setPlayerError(null)
    setInteractionWarning(null)
    setPlaybackSyncError(null)
    setSubtitleTrackError(null)
    setIsEpisodeMenuOpen(false)
    setIsAudioMenuOpen(false)
    setIsSubtitleMenuOpen(false)

    if (!keepBuffering) {
      setIsBuffering(false)
    }
  }

  useEffect(() => {
    selectedMediaFileRef.current = selectedMediaFile
  }, [selectedMediaFile])

  useEffect(() => {
    durationSecondsRef.current = durationSeconds
  }, [durationSeconds])

  useEffect(() => {
    return () => {
      if (audioTrackNoticeTimeoutRef.current !== null) {
        window.clearTimeout(audioTrackNoticeTimeoutRef.current)
        audioTrackNoticeTimeoutRef.current = null
      }

      setAudioTrackNotice(null)
    }
  }, [])

  useEffect(() => {
    const mediaItemChanged = previousMediaItemIdRef.current !== mediaItemId
    previousMediaItemIdRef.current = mediaItemId

    if (mediaItemChanged) {
      pendingPlaybackRestoreRef.current = null
      pendingAudioTrackSwitchRef.current = null

      if (audioTrackNoticeTimeoutRef.current !== null) {
        window.clearTimeout(audioTrackNoticeTimeoutRef.current)
        audioTrackNoticeTimeoutRef.current = null
      }

      setAudioTrackNotice(null)
    }

    shouldHonorStartModeRef.current = startMode === 'from-start'
  }, [mediaItemId, startMode])

  useEffect(() => {
    if (mediaFiles.length === 0) {
      setSelectedMediaFileId(null)
      return
    }

    // Prefer the file tied to the saved playback progress so multi-file items reopen on the same
    // source instead of snapping back to the first file after every refresh.
    const playbackProgress = playbackProgressQuery.data
    const preferredFile =
      mediaFiles.find((file) => file.id === preferredMediaFileId) ??
      (playbackProgress && mediaFiles.find((file) => file.id === playbackProgress.media_file_id)) ??
      mediaFiles[0]

    setSelectedMediaFileId((current) =>
      current && mediaFiles.some((file) => file.id === current) ? current : preferredFile.id,
    )
  }, [mediaFiles, playbackProgressQuery.data, preferredMediaFileId])

  useEffect(() => {
    restoredForFileRef.current = null
    lastReportedSecondsRef.current = -1
    setPlayerError(null)
    setInteractionWarning(null)
    setPlaybackSyncError(null)
    setSubtitleTrackError(null)
    setIsEpisodeMenuOpen(false)
    setIsAudioMenuOpen(false)
    setIsSubtitleMenuOpen(false)
    setIsBuffering(selectedMediaFileId !== null)
    setBufferedSeconds(0)
    setPositionSeconds(0)
    setDurationSeconds(selectedMediaFileId === null ? null : selectedMediaFileDuration)
    setIsPlaying(false)
    pendingAudioTrackSwitchRef.current = null

    if (audioTrackNoticeTimeoutRef.current !== null) {
      window.clearTimeout(audioTrackNoticeTimeoutRef.current)
      audioTrackNoticeTimeoutRef.current = null
    }

    setAudioTrackNotice(null)
  }, [selectedMediaFileDuration, selectedMediaFileId])

  useEffect(() => {
    if (selectedMediaFileId === null) {
      return
    }

    setSelectedAudioTrackId(null)
    setSelectedSubtitleId(null)
  }, [selectedMediaFileId])

  useEffect(() => {
    if (
      selectedAudioTrackId !== null &&
      !audioTracks.some((audioTrack) => audioTrack.id === selectedAudioTrackId)
    ) {
      setSelectedAudioTrackId(null)
    }
  }, [audioTracks, selectedAudioTrackId])

  useEffect(() => {
    if (subtitleFiles.length === 0) {
      setSelectedSubtitleId(null)
      return
    }

    if (
      selectedSubtitleId &&
      subtitleFiles.some((subtitle) => subtitle.id === selectedSubtitleId)
    ) {
      return
    }

    const preferredSubtitle =
      subtitleFiles.find((subtitle) => subtitle.is_default) ??
      subtitleFiles.find((subtitle) => subtitle.language?.toLowerCase().startsWith('zh')) ??
      subtitleFiles[0]

    setSelectedSubtitleId(preferredSubtitle?.id ?? null)
  }, [selectedSubtitleId, subtitleFiles])

  useEffect(() => {
    if (!isSubtitleMenuOpen && !isEpisodeMenuOpen && !isAudioMenuOpen) {
      return
    }

    const handlePointerDown = (event: MouseEvent) => {
      if (!(event.target instanceof Node)) {
        return
      }

      const subtitleMenuRoot = subtitleMenuRef.current
      const audioMenuRoot = audioMenuRef.current
      const episodeMenuRoot = episodeMenuRef.current
      const clickedSubtitleMenu = subtitleMenuRoot?.contains(event.target)
      const clickedAudioMenu = audioMenuRoot?.contains(event.target)
      const clickedEpisodeMenu = episodeMenuRoot?.contains(event.target)

      if (!clickedSubtitleMenu) {
        setIsSubtitleMenuOpen(false)
      }

      if (!clickedAudioMenu) {
        setIsAudioMenuOpen(false)
      }

      if (!clickedEpisodeMenu) {
        setIsEpisodeMenuOpen(false)
      }
    }

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setIsSubtitleMenuOpen(false)
        setIsAudioMenuOpen(false)
        setIsEpisodeMenuOpen(false)
      }
    }

    window.addEventListener('mousedown', handlePointerDown)
    window.addEventListener('keydown', handleEscape)

    return () => {
      window.removeEventListener('mousedown', handlePointerDown)
      window.removeEventListener('keydown', handleEscape)
    }
  }, [isAudioMenuOpen, isEpisodeMenuOpen, isSubtitleMenuOpen])

  useEffect(() => {
    const video = videoRef.current
    if (!video || selectedMediaFileId === null) {
      return
    }

    if (!selectedSubtitle) {
      forceSelectedTextTrack(video, false)
      return
    }

    // Web 端始终只保留一条激活字幕轨道；切换时先禁用旧轨道，再等待新 track 加载并显示，
    // 这样外挂字幕和内嵌字幕不会出现同时叠加的重影。
    const applySelectedTrack = () => {
      forceSelectedTextTrack(video, true)
    }

    const trackElements = Array.from(video.querySelectorAll('track'))
    const deferHandle = window.setTimeout(applySelectedTrack, 0)
    trackElements.forEach((track) => {
      track.addEventListener('load', applySelectedTrack)
    })

    return () => {
      window.clearTimeout(deferHandle)
      trackElements.forEach((track) => {
        track.removeEventListener('load', applySelectedTrack)
      })
    }
  }, [selectedSubtitle, selectedMediaFileId])

  useEffect(() => {
    const video = videoRef.current
    if (!video) {
      return
    }

    const syncVolumeState = () => {
      setIsMuted(video.muted || video.volume === 0)
      setVolume(video.volume)
    }
    const syncBufferedState = () => {
      setBufferedSeconds(measureBufferedSeconds(video))
    }

    const handlePlay = () => {
      setIsPlaying(true)
      setInteractionWarning(null)
    }
    const handlePause = () => setIsPlaying(false)
    const handleLoadStart = () => {
      setIsBuffering(true)
      setBufferedSeconds(0)
    }
    const handleWaiting = () => {
      setIsBuffering(true)
      syncBufferedState()
    }
    const handlePlaybackReady = () => {
      syncBufferedState()
      setIsBuffering(false)
      setPlayerError(null)

      if (pendingAudioTrackSwitchRef.current) {
        const switchTargetId = pendingAudioTrackSwitchRef.current.target
        const targetAudioTrack =
          switchTargetId === null
            ? null
            : (audioTracks.find((audioTrack) => audioTrack.id === switchTargetId) ?? null)

        pendingAudioTrackSwitchRef.current = null

        if (audioTrackNoticeTimeoutRef.current !== null) {
          window.clearTimeout(audioTrackNoticeTimeoutRef.current)
          audioTrackNoticeTimeoutRef.current = null
        }

        setAudioTrackNotice(buildAudioTrackReadyMessage(targetAudioTrack))
        audioTrackNoticeTimeoutRef.current = window.setTimeout(() => {
          audioTrackNoticeTimeoutRef.current = null
          setAudioTrackNotice(null)
        }, 2400)
      }
    }
    const handleFullscreenChange = () => {
      setIsFullscreen(document.fullscreenElement === stageRef.current)
    }

    syncVolumeState()
    syncBufferedState()
    video.addEventListener('play', handlePlay)
    video.addEventListener('pause', handlePause)
    video.addEventListener('loadstart', handleLoadStart)
    video.addEventListener('waiting', handleWaiting)
    video.addEventListener('stalled', handleWaiting)
    video.addEventListener('progress', syncBufferedState)
    video.addEventListener('canplay', handlePlaybackReady)
    video.addEventListener('playing', handlePlaybackReady)
    video.addEventListener('volumechange', syncVolumeState)
    document.addEventListener('fullscreenchange', handleFullscreenChange)

    return () => {
      video.removeEventListener('play', handlePlay)
      video.removeEventListener('pause', handlePause)
      video.removeEventListener('loadstart', handleLoadStart)
      video.removeEventListener('waiting', handleWaiting)
      video.removeEventListener('stalled', handleWaiting)
      video.removeEventListener('progress', syncBufferedState)
      video.removeEventListener('canplay', handlePlaybackReady)
      video.removeEventListener('playing', handlePlaybackReady)
      video.removeEventListener('volumechange', syncVolumeState)
      document.removeEventListener('fullscreenchange', handleFullscreenChange)
    }
  }, [audioTracks])

  const measurePlaybackProgress = () => {
    const video = videoRef.current
    const mediaFile = selectedMediaFileRef.current
    if (!video || !mediaFile) {
      return null
    }

    const measuredDuration =
      Number.isFinite(video.duration) && video.duration > 0
        ? Math.round(video.duration)
        : (durationSecondsRef.current ?? mediaFile.duration_seconds ?? undefined)
    const measuredPosition = Math.max(
      0,
      Math.round(
        measuredDuration ? Math.min(video.currentTime, measuredDuration) : video.currentTime,
      ),
    )

    return {
      durationSeconds: measuredDuration,
      mediaFileId: mediaFile.id,
      positionSeconds: measuredPosition,
    }
  }

  // 播放中的持久化改成定时轮询，不再依赖浏览器 `timeupdate` 的触发频率。
  syncPlaybackProgressRef.current = (force = false, isFinished = false) => {
    const snapshot = measurePlaybackProgress()
    if (!snapshot) {
      return
    }

    if (
      !force &&
      Math.abs(snapshot.positionSeconds - lastReportedSecondsRef.current) <
        PROGRESS_SYNC_INTERVAL_SECONDS
    ) {
      return
    }

    playbackProgressMutation.mutate({
      media_file_id: snapshot.mediaFileId,
      position_seconds: snapshot.positionSeconds,
      duration_seconds: snapshot.durationSeconds,
      is_finished:
        isFinished ||
        shouldMarkPlaybackFinished({
          durationSeconds: snapshot.durationSeconds,
          positionSeconds: snapshot.positionSeconds,
        }),
    })
  }

  flushPlaybackProgressRef.current = () => {
    const snapshot = measurePlaybackProgress()
    if (
      !snapshot ||
      snapshot.positionSeconds <= 0 ||
      Math.abs(snapshot.positionSeconds - lastReportedSecondsRef.current) < 1
    ) {
      return
    }

    hasSubmittedProgressRef.current = true
    lastReportedSecondsRef.current = snapshot.positionSeconds
    flushMediaItemPlaybackProgress(mediaItemId, {
      media_file_id: snapshot.mediaFileId,
      position_seconds: snapshot.positionSeconds,
      duration_seconds: snapshot.durationSeconds,
      is_finished: shouldMarkPlaybackFinished({
        durationSeconds: snapshot.durationSeconds,
        positionSeconds: snapshot.positionSeconds,
      }),
    })
  }

  useEffect(() => {
    // 页面切路由、切后台、直接关闭时都在这里补一次强制上报，避免“没点暂停就丢进度”。
    const handlePageHide = () => {
      flushPlaybackProgressRef.current()
    }

    const handleVisibilityChange = () => {
      if (document.visibilityState === 'hidden') {
        flushPlaybackProgressRef.current()
      }
    }

    window.addEventListener('pagehide', handlePageHide)
    document.addEventListener('visibilitychange', handleVisibilityChange)

    return () => {
      window.removeEventListener('pagehide', handlePageHide)
      document.removeEventListener('visibilitychange', handleVisibilityChange)
      flushPlaybackProgressRef.current()

      if (hasSubmittedProgressRef.current) {
        void queryClient.invalidateQueries({ queryKey: ['continue-watching'] })
        void queryClient.invalidateQueries({ queryKey: ['watch-history'] })
        void queryClient.invalidateQueries({
          queryKey: ['media-item-playback-progress', mediaItemId],
        })
      }
    }
  }, [mediaItemId, queryClient])

  useEffect(() => {
    if (!isPlaying || selectedMediaFileId === null) {
      return
    }

    const intervalId = window.setInterval(() => {
      syncPlaybackProgressRef.current(false, false)
    }, PROGRESS_SYNC_INTERVAL_SECONDS * 1000)

    return () => {
      window.clearInterval(intervalId)
    }
  }, [isPlaying, selectedMediaFileId])

  const handleLoadedMetadata = () => {
    const video = videoRef.current
    const playbackProgress = playbackProgressQuery.data
    if (!video || !selectedMediaFile) {
      return
    }

    if (Number.isFinite(video.duration) && video.duration > 0) {
      setDurationSeconds(Math.round(video.duration))
    }

    const pendingPlaybackRestore = pendingPlaybackRestoreRef.current
    if (pendingPlaybackRestore) {
      const maxDuration =
        Number.isFinite(video.duration) && video.duration > 0
          ? Math.round(video.duration)
          : (selectedMediaFile.duration_seconds ?? undefined)
      const restorePosition = Math.max(
        0,
        Math.min(
          pendingPlaybackRestore.positionSeconds,
          typeof maxDuration === 'number' && maxDuration > 0
            ? maxDuration
            : Number.POSITIVE_INFINITY,
        ),
      )

      video.currentTime = restorePosition
      setPositionSeconds(Math.round(restorePosition))
      pendingPlaybackRestoreRef.current = null
      shouldHonorStartModeRef.current = false
      restoredForFileRef.current = selectedMediaFile.id

      if (pendingPlaybackRestore.shouldPersistSelection) {
        lastReportedSecondsRef.current = -1
        syncPlaybackProgressRef.current(true, false)
      }

      if (pendingPlaybackRestore.shouldAutoplay) {
        void video.play().catch((error) => {
          setInteractionWarning(buildPlaybackInteractionWarningMessage(error))
        })
      }

      return
    }

    if (shouldHonorStartModeRef.current) {
      // "Play from beginning" should win over any stored resume point, but only once per file
      // selection so metadata reloads or manual source switches do not keep rewinding playback.
      shouldHonorStartModeRef.current = false
      video.currentTime = 0
      setPositionSeconds(0)
      lastReportedSecondsRef.current = 0
      restoredForFileRef.current = selectedMediaFile.id
      return
    }

    if (
      restoredForFileRef.current === selectedMediaFile.id ||
      !playbackProgress ||
      playbackProgress.is_finished ||
      playbackProgress.media_file_id !== selectedMediaFile.id ||
      playbackProgress.position_seconds <= 0
    ) {
      return
    }

    // 详情页和接口展示都应以持久化进度为准，这里直接精确恢复到上次同步秒数，
    // 避免 UI 显示时间与后端记录出现 2 秒偏差。
    const resumePosition = Math.max(0, playbackProgress.position_seconds)
    video.currentTime = resumePosition
    setPositionSeconds(Math.round(resumePosition))
    lastReportedSecondsRef.current = playbackProgress.position_seconds
    restoredForFileRef.current = selectedMediaFile.id
  }

  const handleTimeUpdate = () => {
    const video = videoRef.current
    if (!video) {
      return
    }

    setPositionSeconds(Math.max(0, Math.round(video.currentTime)))
  }

  const handlePause = () => {
    syncPlaybackProgressRef.current(true, false)
  }

  const handleEnded = () => {
    const video = videoRef.current
    if (!video) {
      return
    }

    const endedDuration =
      Number.isFinite(video.duration) && video.duration > 0
        ? Math.round(video.duration)
        : (durationSeconds ?? 0)
    setPositionSeconds(endedDuration)
    syncPlaybackProgressRef.current(true, true)
  }

  const handlePlayerError = () => {
    setIsBuffering(false)
    const fallbackMessage = buildPlaybackSourceErrorMessage(videoRef.current)

    if (pendingAudioTrackSwitchRef.current) {
      const switchLabel = pendingAudioTrackSwitchRef.current.label
      pendingAudioTrackSwitchRef.current = null
      setPlayerError(`Switching audio to ${switchLabel} failed. ${fallbackMessage}`)
      clearAudioTrackNotice()
      return
    }

    setPlayerError(fallbackMessage)
  }

  const handleSubtitleTrackError = () => {
    const video = videoRef.current
    if (video) {
      forceSelectedTextTrack(video, false)
    }

    setSelectedSubtitleId(null)
    setSubtitleTrackError(SUBTITLE_LOAD_ERROR)
  }

  const persistProgressBeforeSwitch = () => {
    // 切源/切集不一定会触发暂停事件，先把当前播放点补报出去，避免刚看的几秒丢失。
    flushPlaybackProgressRef.current()
  }

  const queuePlaybackRestore = (input: PendingPlaybackRestore) => {
    pendingPlaybackRestoreRef.current = input
  }

  const retryCurrentSource = () => {
    const video = videoRef.current
    if (!video || !selectedMediaFile) {
      return
    }

    queuePlaybackRestore({
      positionSeconds: Math.max(0, video.currentTime || positionSeconds),
      shouldAutoplay: !video.paused,
      shouldPersistSelection: false,
    })
    resetTransientPlayerFeedback({ keepBuffering: true })
    setIsBuffering(true)
    video.load()
  }

  const switchMediaFile = (targetMediaFileId: number) => {
    const video = videoRef.current
    if (!video || !selectedMediaFile || targetMediaFileId === selectedMediaFile.id) {
      return
    }

    // 同一条目切换源时，直接把当前时间点迁移到新文件并在加载后立刻持久化，
    // 避免先补旧文件、再写新文件时被网络乱序覆盖回旧源选择。
    queuePlaybackRestore({
      positionSeconds: Math.max(0, video.currentTime || positionSeconds),
      shouldAutoplay: !video.paused,
      shouldPersistSelection: true,
    })
    resetTransientPlayerFeedback({ keepBuffering: true })
    setIsBuffering(true)
    setSelectedMediaFileId(targetMediaFileId)
  }

  const switchAudioTrack = (targetAudioTrackId: number | null) => {
    const video = videoRef.current
    if (!video || !selectedMediaFile || targetAudioTrackId === selectedAudioTrackId) {
      setIsAudioMenuOpen(false)
      return
    }

    const targetAudioTrack =
      targetAudioTrackId === null
        ? null
        : (audioTracks.find((audioTrack) => audioTrack.id === targetAudioTrackId) ?? null)

    persistProgressBeforeSwitch()
    queuePlaybackRestore({
      positionSeconds: Math.max(0, video.currentTime || positionSeconds),
      shouldAutoplay: !video.paused,
      shouldPersistSelection: false,
    })
    resetTransientPlayerFeedback({ keepBuffering: true })
    setIsBuffering(true)
    pendingAudioTrackSwitchRef.current = {
      label: describeAudioTrackSelection(targetAudioTrack),
      target: targetAudioTrackId,
    }
    showAudioTrackNotice(buildAudioTrackSwitchingMessage(targetAudioTrack))
    setSelectedAudioTrackId(targetAudioTrackId)
    setIsAudioMenuOpen(false)
  }

  const isImmersive = variant === 'immersive'
  const seekMax = Math.max(0, durationSeconds ?? selectedMediaFileDuration ?? 0)
  const playedProgressPercent = seekMax > 0 ? Math.min(100, (positionSeconds / seekMax) * 100) : 0
  const bufferedProgressPercent =
    seekMax > 0 ? Math.min(100, (Math.max(bufferedSeconds, positionSeconds) / seekMax) * 100) : 0
  const shouldShowOverlay = shouldShowImmersiveOverlay({
    hasInteractionWarning: interactionWarning !== null,
    hasMultipleSources: mediaFiles.length > 1,
    hasPlaybackSyncError: playbackSyncError !== null,
    hasPlayerError: playerError !== null,
    hasSubtitleWarning: subtitleWarning !== null,
    isBuffering,
  })
  const shouldRenderAudioMenu =
    audioTracks.length > 1 || audioTracksQuery.isError || audioTracksQuery.isLoading
  const timelineStyle = {
    '--player-range-buffered': `${Math.max(playedProgressPercent, bufferedProgressPercent)}%`,
    '--player-range-played': `${playedProgressPercent}%`,
  } as CSSProperties

  const togglePlay = async () => {
    const video = videoRef.current
    if (!video) {
      return
    }

    if (video.paused) {
      try {
        await video.play()
      } catch (error) {
        setInteractionWarning(buildPlaybackInteractionWarningMessage(error))
      }
      return
    }

    video.pause()
  }

  const seekTo = (targetSeconds: number) => {
    const video = videoRef.current
    if (!video) {
      return
    }

    const nextSeconds = Math.max(0, Math.min(seekMax || targetSeconds, targetSeconds))
    video.currentTime = nextSeconds
    setPositionSeconds(Math.round(nextSeconds))
    syncPlaybackProgressRef.current(true, false)
  }

  const seekBy = (deltaSeconds: number) => {
    const video = videoRef.current
    if (!video) {
      return
    }

    seekTo(video.currentTime + deltaSeconds)
  }

  const changeVolume = (nextVolume: number) => {
    const video = videoRef.current
    if (!video) {
      return
    }

    const normalizedVolume = Math.max(0, Math.min(1, nextVolume))
    video.volume = normalizedVolume
    video.muted = normalizedVolume === 0
  }

  const toggleFullscreen = async () => {
    const stage = stageRef.current
    if (!stage) {
      return
    }

    if (document.fullscreenElement === stage) {
      try {
        await document.exitFullscreen()
        setInteractionWarning(null)
      } catch (error) {
        setInteractionWarning(buildFullscreenWarningMessage(error))
      }
      return
    }

    if (typeof stage.requestFullscreen !== 'function' || document.fullscreenEnabled === false) {
      setInteractionWarning(buildFullscreenWarningMessage())
      return
    }

    try {
      await stage.requestFullscreen()
      setInteractionWarning(null)
    } catch (error) {
      setInteractionWarning(buildFullscreenWarningMessage(error))
    }
  }

  return (
    <section className={isImmersive ? 'player-panel player-panel--immersive' : 'player-panel'}>
      {!isImmersive ? (
        <div className="catalog-block__header">
          <div>
            <h3>Playback</h3>
          </div>
        </div>
      ) : null}

      {mediaFilesQuery.isLoading ? <p className="muted">Loading player…</p> : null}

      {mediaFilesQuery.isError ? (
        <p className="callout callout--danger">
          {mediaFilesQuery.error instanceof Error
            ? mediaFilesQuery.error.message
            : 'Failed to load media files'}
        </p>
      ) : null}

      {playbackProgressQuery.isError ? (
        <p className="callout callout--danger">
          {playbackProgressQuery.error instanceof Error
            ? playbackProgressQuery.error.message
            : 'Failed to load playback progress'}
        </p>
      ) : null}

      {mediaFiles.length === 0 && !mediaFilesQuery.isLoading ? (
        <div className="catalog-block__empty">
          <p className="muted">No playable media files are linked to this item yet.</p>
        </div>
      ) : null}

      {selectedMediaFile ? (
        <div
          className={
            isImmersive
              ? 'player-panel__content player-panel__content--immersive'
              : 'player-panel__content'
          }
        >
          <div className="player-stage" ref={stageRef}>
            <div className="player-stage__media">
              {isImmersive && shouldShowOverlay ? (
                <div className="player-panel__overlay">
                  <div className="player-panel__overlay-status">
                    {isBuffering && !playerError ? (
                      <p className="player-panel__status-badge">
                        {audioTrackNotice ?? 'Buffering playback…'}
                      </p>
                    ) : null}
                    {playerError ? (
                      <div className="player-panel__status-stack">
                        <p className="callout callout--danger">{playerError}</p>
                        <button className="button" onClick={retryCurrentSource} type="button">
                          Retry current source
                        </button>
                      </div>
                    ) : null}
                    {!playerError && playbackSyncError ? (
                      <p className="callout">{playbackSyncError}</p>
                    ) : null}
                    {!playerError && interactionWarning ? (
                      <p className="callout">{interactionWarning}</p>
                    ) : null}
                    {!playerError && subtitleWarning ? (
                      <p className="callout">{subtitleWarning}</p>
                    ) : null}
                    {!playerError &&
                    !isBuffering &&
                    !playbackSyncError &&
                    !interactionWarning &&
                    !subtitleWarning &&
                    audioTrackNotice ? (
                      <p className="callout">{audioTrackNotice}</p>
                    ) : null}
                  </div>

                  {mediaFiles.length > 1 ? (
                    <div className="player-source-list player-source-list--overlay">
                      {mediaFiles.map((file) => {
                        const isActive = file.id === selectedMediaFile.id

                        return (
                          <button
                            className={
                              isActive
                                ? 'player-source player-source--active player-source--compact'
                                : 'player-source player-source--compact'
                            }
                            key={file.id}
                            onClick={() => switchMediaFile(file.id)}
                            type="button"
                          >
                            <span className="player-source__title">
                              {file.container?.toUpperCase() ?? 'FILE'}
                            </span>
                            <span className="player-source__meta">
                              {formatVideoMeta(file) || file.file_path}
                            </span>
                          </button>
                        )
                      })}
                    </div>
                  ) : null}
                </div>
              ) : null}

              {/* biome-ignore lint/a11y/useMediaCaption: 当前播放器允许“关闭字幕”，未选中时不会挂载活动字幕轨道。 */}
              <video
                className="player-stage__video"
                controls={!isImmersive}
                controlsList="nodownload noplaybackrate"
                disablePictureInPicture={isImmersive}
                disableRemotePlayback={isImmersive}
                onClick={isImmersive ? () => void togglePlay() : undefined}
                onEnded={handleEnded}
                onError={handlePlayerError}
                onLoadedMetadata={handleLoadedMetadata}
                onPause={handlePause}
                onTimeUpdate={handleTimeUpdate}
                poster={undefined}
                preload="metadata"
                playsInline
                ref={videoRef}
                src={mediaFileStreamUrl(selectedMediaFile.id, {
                  audioTrackId: selectedAudioTrackId,
                })}
              >
                {selectedSubtitle ? (
                  // Web 端同一时间只挂一条字幕 track，切换时直接替换，避免内嵌/外挂叠加重影。
                  <track
                    default
                    key={selectedSubtitle.id}
                    kind="subtitles"
                    label={renderSubtitleLabel(selectedSubtitle)}
                    onError={handleSubtitleTrackError}
                    src={subtitleFileStreamUrl(selectedSubtitle.id)}
                    srcLang={normalizeSubtitleTrackLanguage(selectedSubtitle.language)}
                  />
                ) : null}
                Your browser does not support HTML5 video playback.
              </video>
            </div>

            {isImmersive ? (
              <div className="player-stage__controls">
                <div className="player-stage__timeline">
                  <input
                    aria-label="Seek playback position"
                    className="player-range player-range--timeline"
                    max={seekMax || 0}
                    min={0}
                    onChange={(event) => seekTo(Number(event.target.value))}
                    step={1}
                    style={timelineStyle}
                    type="range"
                    value={Math.min(positionSeconds, seekMax || positionSeconds)}
                  />
                </div>

                <div className="player-stage__control-row">
                  <div className="player-toolbar-cluster">
                    <div className="player-toolbar-pill">
                      <button
                        aria-label={isPlaying ? 'Pause playback' : 'Start playback'}
                        className="player-control-button player-control-button--icon player-control-button--toolbar player-control-button--primary"
                        onClick={() => void togglePlay()}
                        type="button"
                      >
                        {isPlaying ? <PauseIcon /> : <PlayIcon />}
                      </button>
                      <button
                        aria-label="Seek backward 10 seconds"
                        className="player-control-button player-control-button--icon player-control-button--toolbar"
                        onClick={() => seekBy(-10)}
                        title="Back 10 seconds"
                        type="button"
                      >
                        <SeekBackIcon />
                      </button>
                      <button
                        aria-label="Seek forward 10 seconds"
                        className="player-control-button player-control-button--icon player-control-button--toolbar"
                        onClick={() => seekBy(10)}
                        title="Forward 10 seconds"
                        type="button"
                      >
                        <SeekForwardIcon />
                      </button>
                    </div>

                    <div className="player-toolbar-pill player-toolbar-pill--time">
                      <span className="player-stage__time">
                        {formatDuration(positionSeconds)} / {formatDuration(durationSeconds)}
                      </span>
                    </div>
                  </div>

                  <div className="player-toolbar-cluster player-toolbar-cluster--right">
                    <div className="player-toolbar-pill">
                      {episodeSwitchOptions.length > 0 && onSelectEpisode ? (
                        <div
                          className={
                            isEpisodeMenuOpen
                              ? 'player-popover-menu player-popover-menu--open'
                              : 'player-popover-menu'
                          }
                          ref={episodeMenuRef}
                        >
                          <button
                            aria-expanded={isEpisodeMenuOpen}
                            aria-haspopup="menu"
                            aria-label="Switch episode"
                            className={
                              isEpisodeMenuOpen
                                ? 'player-control-button player-control-button--icon player-control-button--toolbar player-control-button--active'
                                : 'player-control-button player-control-button--icon player-control-button--toolbar'
                            }
                            onClick={() => {
                              setIsEpisodeMenuOpen((open) => !open)
                              setIsSubtitleMenuOpen(false)
                            }}
                            type="button"
                          >
                            <EpisodeSwitchIcon />
                          </button>

                          {isEpisodeMenuOpen ? (
                            <div className="player-popover-menu__bubble" role="menu">
                              {episodeSwitchOptions.map((episode) => (
                                <button
                                  className="player-popover-menu__option"
                                  key={episode.mediaItemId}
                                  onClick={() => {
                                    setIsEpisodeMenuOpen(false)
                                    persistProgressBeforeSwitch()
                                    onSelectEpisode(episode.mediaItemId)
                                  }}
                                  role="menuitem"
                                  type="button"
                                >
                                  <span>{episode.label}</span>
                                </button>
                              ))}
                            </div>
                          ) : null}
                        </div>
                      ) : null}

                      {shouldRenderAudioMenu ? (
                        <div
                          className={
                            isAudioMenuOpen
                              ? 'player-popover-menu player-popover-menu--open'
                              : 'player-popover-menu'
                          }
                          ref={audioMenuRef}
                        >
                          <button
                            aria-expanded={isAudioMenuOpen}
                            aria-haspopup="menu"
                            aria-label="Select audio track"
                            title={`Audio: ${currentAudioSelectionLabel}`}
                            className={
                              selectedAudioTrackId !== null || isAudioMenuOpen
                                ? 'player-control-button player-control-button--icon player-control-button--toolbar player-control-button--active'
                                : 'player-control-button player-control-button--icon player-control-button--toolbar'
                            }
                            onClick={() => {
                              setIsAudioMenuOpen((open) => !open)
                              setIsEpisodeMenuOpen(false)
                              setIsSubtitleMenuOpen(false)
                            }}
                            type="button"
                          >
                            <AudioTrackIcon />
                          </button>

                          {isAudioMenuOpen ? (
                            <div className="player-popover-menu__bubble" role="menu">
                              <div className="player-popover-menu__header">
                                <strong>Audio</strong>
                                <small>
                                  {audioTracksQuery.isLoading
                                    ? 'Loading embedded audio tracks…'
                                    : `Current: ${currentAudioSelectionLabel}`}
                                </small>
                              </div>

                              <button
                                className={
                                  selectedAudioTrackId === null
                                    ? 'player-popover-menu__option player-popover-menu__option--active'
                                    : 'player-popover-menu__option'
                                }
                                onClick={() => switchAudioTrack(null)}
                                role="menuitem"
                                type="button"
                              >
                                <span>Original default track</span>
                                <small>Use the source file&apos;s default audio</small>
                              </button>

                              {audioTracks.map((audioTrack) => (
                                <button
                                  className={
                                    selectedAudioTrackId === audioTrack.id
                                      ? 'player-popover-menu__option player-popover-menu__option--active'
                                      : 'player-popover-menu__option'
                                  }
                                  key={audioTrack.id}
                                  onClick={() => switchAudioTrack(audioTrack.id)}
                                  role="menuitem"
                                  type="button"
                                >
                                  <span>{formatAudioTrackLabel(audioTrack)}</span>
                                  <small>{formatAudioTrackMeta(audioTrack) || 'Embedded'}</small>
                                </button>
                              ))}

                              {audioTracks.length === 0 && !audioTracksQuery.isLoading ? (
                                <p className="player-popover-menu__empty">
                                  {audioTracksQuery.error
                                    ? buildAudioTrackLoadErrorMessage()
                                    : 'No alternate audio tracks found.'}
                                </p>
                              ) : null}
                            </div>
                          ) : null}
                        </div>
                      ) : null}

                      <div
                        className={
                          isSubtitleMenuOpen
                            ? 'player-popover-menu player-popover-menu--open'
                            : 'player-popover-menu'
                        }
                        ref={subtitleMenuRef}
                      >
                        <button
                          aria-expanded={isSubtitleMenuOpen}
                          aria-haspopup="menu"
                          aria-label="Select subtitles"
                          className={
                            selectedSubtitleId !== null || isSubtitleMenuOpen
                              ? 'player-control-button player-control-button--icon player-control-button--toolbar player-control-button--active'
                              : 'player-control-button player-control-button--icon player-control-button--toolbar'
                          }
                          onClick={() => {
                            setIsSubtitleMenuOpen((open) => !open)
                            setIsEpisodeMenuOpen(false)
                            setIsAudioMenuOpen(false)
                          }}
                          type="button"
                        >
                          <SubtitleIcon />
                        </button>

                        {isSubtitleMenuOpen ? (
                          <div className="player-popover-menu__bubble" role="menu">
                            <button
                              className={
                                selectedSubtitleId === null
                                  ? 'player-popover-menu__option player-popover-menu__option--active'
                                  : 'player-popover-menu__option'
                              }
                              onClick={() => {
                                setSubtitleTrackError(null)
                                setSelectedSubtitleId(null)
                                setIsSubtitleMenuOpen(false)
                              }}
                              role="menuitem"
                              type="button"
                            >
                              Off
                            </button>

                            {subtitleFiles.map((subtitle) => (
                              <button
                                className={
                                  selectedSubtitleId === subtitle.id
                                    ? 'player-popover-menu__option player-popover-menu__option--active'
                                    : 'player-popover-menu__option'
                                }
                                key={subtitle.id}
                                onClick={() => {
                                  setSubtitleTrackError(null)
                                  setSelectedSubtitleId(subtitle.id)
                                  setIsSubtitleMenuOpen(false)
                                }}
                                role="menuitem"
                                type="button"
                              >
                                <span>{renderSubtitleLabel(subtitle) || 'Unknown subtitle'}</span>
                                <small>
                                  {subtitle.source_kind === 'embedded' ? 'Embedded' : 'External'}
                                </small>
                              </button>
                            ))}

                            {subtitleFiles.length === 0 && !subtitleFilesQuery.isLoading ? (
                              <p className="player-popover-menu__empty">No subtitles found.</p>
                            ) : null}
                            {subtitleFilesQuery.isError ? (
                              <p className="player-popover-menu__empty">
                                {subtitleFilesQuery.error instanceof Error
                                  ? subtitleFilesQuery.error.message
                                  : 'Failed to load subtitles'}
                              </p>
                            ) : null}
                          </div>
                        ) : null}
                      </div>

                      <div className="player-volume-control">
                        <button
                          aria-label="Adjust volume"
                          className="player-control-button player-control-button--icon player-control-button--toolbar"
                          type="button"
                          title={
                            selectedAudioTrack
                              ? `Selected audio: ${formatAudioTrackLabel(selectedAudioTrack)}`
                              : 'Adjust volume'
                          }
                        >
                          <SpeakerIcon muted={isMuted} volume={volume} />
                        </button>
                        <div className="player-volume-control__slider">
                          <input
                            aria-label="Adjust volume"
                            className="player-range player-range--volume-inline"
                            max={1}
                            min={0}
                            onChange={(event) => changeVolume(Number(event.target.value))}
                            step={0.05}
                            type="range"
                            value={isMuted ? 0 : volume}
                          />
                        </div>
                      </div>
                      <button
                        aria-label={isFullscreen ? 'Exit fullscreen' : 'Enter fullscreen'}
                        className="player-control-button player-control-button--icon player-control-button--toolbar"
                        onClick={() => void toggleFullscreen()}
                        title={isFullscreen ? 'Exit fullscreen' : 'Enter fullscreen'}
                        type="button"
                      >
                        <FullscreenIcon />
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            ) : null}
          </div>

          {!isImmersive ? (
            <div className="player-panel__meta">
              <div className="player-panel__info">
                <strong>{title}</strong>
                <span className="muted">
                  {formatVideoMeta(selectedMediaFile) || 'Playable source'}
                </span>
              </div>
              <div className="player-panel__info player-panel__info--compact">
                <span className="muted">Current</span>
                <strong>{formatDuration(positionSeconds)}</strong>
              </div>
              <div className="player-panel__info player-panel__info--compact">
                <span className="muted">Duration</span>
                <strong>{formatDuration(durationSeconds)}</strong>
              </div>
            </div>
          ) : null}

          {playerError && !isImmersive ? (
            <div className="player-panel__status-stack">
              <p className="callout callout--danger">{playerError}</p>
              <button className="button" onClick={retryCurrentSource} type="button">
                Retry current source
              </button>
            </div>
          ) : null}

          {!playerError && playbackSyncError && !isImmersive ? (
            <p className="callout">{playbackSyncError}</p>
          ) : null}

          {!playerError && interactionWarning && !isImmersive ? (
            <p className="callout">{interactionWarning}</p>
          ) : null}

          {!playerError && subtitleWarning && !isImmersive ? (
            <p className="callout">{subtitleWarning}</p>
          ) : null}

          {!playerError &&
          !isBuffering &&
          !playbackSyncError &&
          !interactionWarning &&
          !subtitleWarning &&
          audioTrackNotice &&
          !isImmersive ? (
            <p className="callout">{audioTrackNotice}</p>
          ) : null}

          {isBuffering && !playerError && !isImmersive ? (
            <p className="player-panel__status-badge">
              {audioTrackNotice ?? 'Buffering playback…'}
            </p>
          ) : null}

          {mediaFiles.length > 1 && !isImmersive ? (
            <div className="player-source-list">
              {mediaFiles.map((file) => {
                const isActive = file.id === selectedMediaFile.id

                return (
                  <button
                    className={isActive ? 'player-source player-source--active' : 'player-source'}
                    key={file.id}
                    onClick={() => switchMediaFile(file.id)}
                    type="button"
                  >
                    <span className="player-source__title">
                      {file.container?.toUpperCase() ?? 'FILE'}
                    </span>
                    <span className="player-source__meta">
                      {formatVideoMeta(file) || file.file_path}
                    </span>
                  </button>
                )
              })}
            </div>
          ) : null}
        </div>
      ) : null}
    </section>
  )
}
