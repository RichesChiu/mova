import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import {
  flushMediaItemPlaybackProgress,
  getMediaItemPlaybackProgress,
  listMediaFileAudioTracks,
  listMediaFileSubtitles,
  listMediaItemFiles,
  updateMediaItemPlaybackProgress,
} from '../../api/client'
import type { EpisodeOutline, MediaFile } from '../../api/types'
import { translateCurrent } from '../../i18n'
import {
  buildAudioTrackReadyMessage,
  buildAudioTrackSwitchingMessage,
  describeAudioTrackSelection,
} from '../../lib/audio-tracks'
import { shouldMarkPlaybackFinished } from '../../lib/playback'
import {
  buildPlaybackInteractionWarningMessage,
  isAutoplayBlockedError,
  isPlaybackAbortError,
} from '../../lib/player-feedback'
import { PlayerPanelView } from './player-panel-view'
import {
  buildPlaybackSourceErrorMessage,
  forceSelectedTextTrack,
  measureBufferedSeconds,
} from './player-utils'
import { usePlayerInteractions } from './use-player-interactions'

export { buildPlaybackSourceErrorMessage } from './player-utils'

const PROGRESS_SYNC_INTERVAL_SECONDS = 5
const PLAYBACK_PROGRESS_SAVE_ERROR = () =>
  translateCurrent('Playback progress could not be saved. We will retry on the next sync.')
const SUBTITLE_LOAD_ERROR = () =>
  translateCurrent(
    'The selected subtitle could not be loaded. Playback will continue without subtitles.',
  )

interface MediaPlayerPanelProps {
  episodeSwitchOptions?: Array<{
    label: string
    mediaItemId: number
  }>
  intro?: {
    startSeconds: number
    endSeconds: number
  } | null
  mediaItemId: number
  nextEpisode?: {
    label: string
    mediaItemId: number
    seasonNumber: number
    episodeNumber: number
  } | null
  onSelectEpisode?: (mediaItemId: number) => void
  preferredMediaFileId?: number | null
  seriesMediaItemId?: number | null
  title: string
  startMode?: 'resume' | 'from-start'
  variant?: 'panel' | 'immersive'
}

interface PendingPlaybackRestore {
  positionSeconds: number
  shouldAutoplay: boolean
  shouldPersistSelection: boolean
}

export const MediaPlayerPanel = ({
  episodeSwitchOptions = [],
  intro = null,
  mediaItemId,
  nextEpisode = null,
  onSelectEpisode,
  preferredMediaFileId = null,
  seriesMediaItemId = null,
  startMode = 'resume',
  title,
  variant = 'panel',
}: MediaPlayerPanelProps) => {
  const queryClient = useQueryClient()
  const isImmersive = variant === 'immersive'
  const stageRef = useRef<HTMLDivElement | null>(null)
  const videoRef = useRef<HTMLVideoElement | null>(null)
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
  const shouldPlayRef = useRef(true)
  const playbackRequestPendingRef = useRef(false)
  const pendingPlaybackRestoreRef = useRef<PendingPlaybackRestore | null>(null)
  const continueRegistrationKeyRef = useRef<string | null>(null)
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
  const [isAutoplayBlocked, setIsAutoplayBlocked] = useState(false)
  const [isMuted, setIsMuted] = useState(false)
  const [volume, setVolume] = useState(1)
  const [isFullscreen, setIsFullscreen] = useState(false)
  const [isEpisodeMenuOpen, setIsEpisodeMenuOpen] = useState(false)
  const [isAudioMenuOpen, setIsAudioMenuOpen] = useState(false)
  const [isSubtitleMenuOpen, setIsSubtitleMenuOpen] = useState(false)
  const [isPlaybackRateMenuOpen, setIsPlaybackRateMenuOpen] = useState(false)
  const [playbackRate, setPlaybackRate] = useState(1)
  const [selectedAudioTrackId, setSelectedAudioTrackId] = useState<number | null>(null)
  const [selectedSubtitleId, setSelectedSubtitleId] = useState<number | null>(null)
  const [hasSkippedIntro, setHasSkippedIntro] = useState(false)
  const arePlayerControlsPinned =
    isAutoplayBlocked ||
    isEpisodeMenuOpen ||
    isAudioMenuOpen ||
    isSubtitleMenuOpen ||
    isPlaybackRateMenuOpen

  const syncEpisodeOutlinePlaybackProgress = ({
    duration_seconds,
    is_finished,
    last_media_file_id,
    last_watched_at,
    position_seconds,
  }: {
    duration_seconds: number | null
    is_finished: boolean
    last_media_file_id: number | null
    last_watched_at: string
    position_seconds: number
  }) => {
    if (!seriesMediaItemId) {
      return
    }

    queryClient.setQueryData<EpisodeOutline>(
      ['media-episode-outline', seriesMediaItemId],
      (currentOutline) => {
        if (!currentOutline) {
          return currentOutline
        }

        return {
          ...currentOutline,
          seasons: currentOutline.seasons.map((season) => ({
            ...season,
            episodes: season.episodes.map((episode) =>
              episode.media_item_id === mediaItemId
                ? {
                    ...episode,
                    playback_progress: {
                      last_media_file_id,
                      position_seconds,
                      duration_seconds,
                      last_watched_at,
                      is_finished,
                    },
                  }
                : episode,
            ),
          })),
        }
      },
    )
  }

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
      syncEpisodeOutlinePlaybackProgress(progress)
    },
    onError: () => {
      setPlaybackSyncError(PLAYBACK_PROGRESS_SAVE_ERROR())
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
    selectedMediaFileId === null
      ? null
      : (mediaFiles.find((file) => file.id === selectedMediaFileId) ?? null)
  const selectedMediaFileDuration = selectedMediaFile?.duration_seconds ?? null
  const selectedAudioTrack =
    audioTracks.find((audioTrack) => audioTrack.id === selectedAudioTrackId) ?? null
  const selectedSubtitle =
    subtitleFiles.find((subtitle) => subtitle.id === selectedSubtitleId) ?? null
  const subtitleWarning =
    subtitleTrackError ?? (subtitleFilesQuery.isError ? SUBTITLE_LOAD_ERROR() : null)
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
    setIsAutoplayBlocked(false)
    setInteractionWarning(null)
    setPlaybackSyncError(null)
    setSubtitleTrackError(null)
    setIsEpisodeMenuOpen(false)
    setIsAudioMenuOpen(false)
    setIsSubtitleMenuOpen(false)
    setIsPlaybackRateMenuOpen(false)

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
      shouldPlayRef.current = true
      playbackRequestPendingRef.current = false

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

    const requestedFile = mediaFiles.find((file) => file.id === preferredMediaFileId)
    if (startMode !== 'from-start' && playbackProgressQuery.isLoading) {
      return
    }

    // Prefer the file tied to the saved playback progress so multi-file items reopen on the same
    // source instead of snapping back to the first file after every refresh.
    const playbackProgress = playbackProgressQuery.data
    const preferredFile =
      requestedFile ??
      (playbackProgress &&
        mediaFiles.find((file) => file.id === playbackProgress.last_media_file_id)) ??
      mediaFiles[0]

    setSelectedMediaFileId((current) =>
      current && mediaFiles.some((file) => file.id === current) ? current : preferredFile.id,
    )
  }, [
    mediaFiles,
    playbackProgressQuery.data,
    playbackProgressQuery.isLoading,
    preferredMediaFileId,
    startMode,
  ])

  useEffect(() => {
    restoredForFileRef.current = null
    playbackRequestPendingRef.current = false
    lastReportedSecondsRef.current = -1
    setPlayerError(null)
    setInteractionWarning(null)
    setPlaybackSyncError(null)
    setSubtitleTrackError(null)
    setIsEpisodeMenuOpen(false)
    setIsAudioMenuOpen(false)
    setIsSubtitleMenuOpen(false)
    setIsPlaybackRateMenuOpen(false)
    setIsBuffering(selectedMediaFileId !== null)
    setBufferedSeconds(0)
    setPositionSeconds(0)
    setDurationSeconds(selectedMediaFileId === null ? null : selectedMediaFileDuration)
    setIsPlaying(false)
    setIsAutoplayBlocked(false)
    setHasSkippedIntro(false)
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
      playbackRequestPendingRef.current = false

      if (!shouldPlayRef.current) {
        video.pause()
        setIsPlaying(false)
        return
      }

      setIsPlaying(true)
      setIsAutoplayBlocked(false)
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
      const stage = stageRef.current
      const fullscreenElement = document.fullscreenElement
      setIsFullscreen(
        Boolean(
          stage &&
            fullscreenElement &&
            (fullscreenElement === stage || fullscreenElement.contains(stage)),
        ),
      )
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

  useEffect(() => {
    if (
      !selectedMediaFile ||
      selectedMediaFile.media_item_id !== mediaItemId ||
      playbackProgressQuery.isLoading
    ) {
      return
    }

    const registrationKey = `${mediaItemId}:${selectedMediaFile.id}`
    if (continueRegistrationKeyRef.current === registrationKey) {
      return
    }

    continueRegistrationKeyRef.current = registrationKey
    const savedProgress = playbackProgressQuery.data
    const canResumeSelectedFile =
      startMode !== 'from-start' &&
      savedProgress?.last_media_file_id === selectedMediaFile.id &&
      !savedProgress.is_finished

    // Opening a selected movie or episode must enter Continue immediately without losing its resume point.
    playbackProgressMutation.mutate({
      media_file_id: selectedMediaFile.id,
      position_seconds: canResumeSelectedFile ? savedProgress.position_seconds : 0,
      duration_seconds: canResumeSelectedFile
        ? (savedProgress.duration_seconds ?? selectedMediaFile.duration_seconds ?? undefined)
        : (selectedMediaFile.duration_seconds ?? undefined),
      is_finished: false,
    })
  }, [
    mediaItemId,
    playbackProgressMutation.mutate,
    playbackProgressQuery.data,
    playbackProgressQuery.isLoading,
    selectedMediaFile,
    startMode,
  ])

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
        void queryClient.invalidateQueries({
          queryKey: ['media-item-playback-progress', mediaItemId],
        })
        if (seriesMediaItemId) {
          void queryClient.invalidateQueries({
            queryKey: ['media-episode-outline', seriesMediaItemId],
          })
        }
      }
    }
  }, [mediaItemId, queryClient, seriesMediaItemId])

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

    video.playbackRate = playbackRate

    const handleAutomaticPlaybackFailure = (error: unknown) => {
      playbackRequestPendingRef.current = false

      if (!shouldPlayRef.current && isPlaybackAbortError(error)) {
        return
      }

      shouldPlayRef.current = false
      if (isAutoplayBlockedError(error)) {
        setIsAutoplayBlocked(true)
        setIsBuffering(false)
        setInteractionWarning(null)
        return
      }

      setInteractionWarning(buildPlaybackInteractionWarningMessage(error))
    }

    const applyPlaybackIntent = () => {
      if (!shouldPlayRef.current) {
        return
      }

      playbackRequestPendingRef.current = true
      void video
        .play()
        .then(() => {
          playbackRequestPendingRef.current = false

          if (!shouldPlayRef.current) {
            video.pause()
          }
        })
        .catch(handleAutomaticPlaybackFailure)
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

      applyPlaybackIntent()

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
      applyPlaybackIntent()
      return
    }

    if (
      restoredForFileRef.current === selectedMediaFile.id ||
      !playbackProgress ||
      playbackProgress.is_finished ||
      playbackProgress.last_media_file_id !== selectedMediaFile.id ||
      playbackProgress.position_seconds <= 0
    ) {
      applyPlaybackIntent()
      return
    }

    // 详情页和接口展示都应以持久化进度为准，这里直接精确恢复到上次同步秒数，
    // 避免 UI 显示时间与后端记录出现 2 秒偏差。
    const resumePosition = Math.max(0, playbackProgress.position_seconds)
    video.currentTime = resumePosition
    setPositionSeconds(Math.round(resumePosition))
    lastReportedSecondsRef.current = playbackProgress.position_seconds
    restoredForFileRef.current = selectedMediaFile.id
    applyPlaybackIntent()
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
    shouldPlayRef.current = false
    playbackRequestPendingRef.current = false
    setPositionSeconds(endedDuration)
    syncPlaybackProgressRef.current(true, true)
  }

  const handlePlayerError = () => {
    shouldPlayRef.current = false
    playbackRequestPendingRef.current = false
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
    setSubtitleTrackError(SUBTITLE_LOAD_ERROR())
  }

  const persistProgressBeforeSwitch = () => {
    // 切源/切集不一定会触发暂停事件，先把当前播放点补报出去，避免刚看的几秒丢失。
    flushPlaybackProgressRef.current()
  }

  const queuePlaybackRestore = (input: PendingPlaybackRestore) => {
    shouldPlayRef.current = input.shouldAutoplay
    playbackRequestPendingRef.current = false
    pendingPlaybackRestoreRef.current = input
  }

  const retryCurrentSource = () => {
    const video = videoRef.current
    if (!video || !selectedMediaFile) {
      return
    }

    queuePlaybackRestore({
      positionSeconds: Math.max(0, video.currentTime || positionSeconds),
      shouldAutoplay: shouldPlayRef.current,
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
      shouldAutoplay: shouldPlayRef.current,
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
      shouldAutoplay: shouldPlayRef.current,
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

  const seekMax = Math.max(0, durationSeconds ?? selectedMediaFileDuration ?? 0)
  const canSkipIntro =
    intro !== null &&
    intro.endSeconds > intro.startSeconds &&
    !hasSkippedIntro &&
    positionSeconds >= intro.startSeconds &&
    positionSeconds < intro.endSeconds
  const bufferingStatusMessage = audioTrackNotice ?? translateCurrent('Buffering playback…')
  const shouldRenderAudioMenu =
    audioTracks.length > 1 || audioTracksQuery.isError || audioTracksQuery.isLoading
  const playbackLoadErrorMessages = [
    mediaFilesQuery.isError
      ? mediaFilesQuery.error instanceof Error
        ? mediaFilesQuery.error.message
        : translateCurrent('Failed to load media files')
      : null,
    playbackProgressQuery.isError
      ? playbackProgressQuery.error instanceof Error
        ? playbackProgressQuery.error.message
        : translateCurrent('Failed to load playback progress')
      : null,
  ].filter((message): message is string => message !== null)
  const centeredPlaybackErrorMessages = selectedMediaFile
    ? [...playbackLoadErrorMessages, playerError].filter(
        (message): message is string => message !== null,
      )
    : []
  const statePlaybackErrorMessages = selectedMediaFile ? [] : playbackLoadErrorMessages

  const {
    arePlayerControlsVisible,
    changePlaybackRate,
    changeVolume,
    seekBy,
    seekTo,
    toggleFullscreen,
    togglePlay,
  } = usePlayerInteractions({
    arePlayerControlsPinned,
    isAutoplayBlocked,
    isImmersive,
    playbackRequestPendingRef,
    seekMax,
    setInteractionWarning,
    setIsAutoplayBlocked,
    setIsPlaybackRateMenuOpen,
    setPlaybackRate,
    setPositionSeconds,
    stageRef,
    shouldPlayRef,
    syncPlaybackProgressRef,
    videoRef,
  })

  const skipIntro = () => {
    if (!intro) {
      return
    }

    setHasSkippedIntro(true)
    seekTo(intro.endSeconds)
  }

  const goToNextEpisode = () => {
    if (!nextEpisode || !onSelectEpisode) {
      return
    }

    persistProgressBeforeSwitch()
    onSelectEpisode(nextEpisode.mediaItemId)
  }

  return (
    <PlayerPanelView
      arePlayerControlsVisible={arePlayerControlsVisible}
      audioTrackNotice={audioTrackNotice}
      bufferedSeconds={bufferedSeconds}
      bufferingStatusMessage={bufferingStatusMessage}
      canSkipIntro={canSkipIntro}
      centeredPlaybackErrorMessages={centeredPlaybackErrorMessages}
      controlMenus={{
        audioTracks,
        audioTracksError: audioTracksQuery.isError,
        audioTracksLoading: audioTracksQuery.isLoading,
        currentAudioSelectionLabel,
        episodeSwitchOptions,
        isAudioMenuOpen,
        isEpisodeMenuOpen,
        isFullscreen,
        isPlaybackRateMenuOpen,
        isSubtitleMenuOpen,
        mediaItemId,
        nextEpisode,
        onAudioMenuOpenChange: setIsAudioMenuOpen,
        onEpisodeMenuOpenChange: setIsEpisodeMenuOpen,
        onGoToNextEpisode: goToNextEpisode,
        onPlaybackRateMenuOpenChange: setIsPlaybackRateMenuOpen,
        onSelectAudioTrack: switchAudioTrack,
        onSelectEpisode: onSelectEpisode
          ? (targetMediaItemId) => {
              persistProgressBeforeSwitch()
              onSelectEpisode(targetMediaItemId)
            }
          : undefined,
        onSelectPlaybackRate: changePlaybackRate,
        onSelectSubtitle: (subtitleId) => {
          setSubtitleTrackError(null)
          setSelectedSubtitleId(subtitleId)
        },
        onSubtitleMenuOpenChange: setIsSubtitleMenuOpen,
        onToggleFullscreen: () => void toggleFullscreen(),
        playbackRate,
        selectedAudioTrackId,
        selectedSubtitleId,
        shouldRenderAudioMenu,
        subtitleFiles,
        subtitleFilesError: subtitleFilesQuery.isError
          ? subtitleFilesQuery.error instanceof Error
            ? subtitleFilesQuery.error.message
            : translateCurrent('Failed to load subtitles')
          : null,
        subtitleFilesLoading: subtitleFilesQuery.isLoading,
      }}
      durationSeconds={durationSeconds}
      interactionWarning={interactionWarning}
      isAutoplayBlocked={isAutoplayBlocked}
      isBuffering={isBuffering}
      isImmersive={isImmersive}
      isMuted={isMuted}
      isPlaying={isPlaying}
      mediaFiles={mediaFiles}
      mediaFilesLoading={
        mediaFilesQuery.isLoading ||
        (mediaFiles.length > 0 && selectedMediaFile === null && playbackProgressQuery.isLoading)
      }
      onChangeVolume={changeVolume}
      onEnded={handleEnded}
      onLoadedMetadata={handleLoadedMetadata}
      onPause={handlePause}
      onPlayerError={handlePlayerError}
      onRetryCurrentSource={retryCurrentSource}
      onSeekBy={seekBy}
      onSeekTo={seekTo}
      onSkipIntro={skipIntro}
      onSubtitleTrackError={handleSubtitleTrackError}
      onSwitchMediaFile={switchMediaFile}
      onTimeUpdate={handleTimeUpdate}
      onTogglePlay={togglePlay}
      playbackSyncError={playbackSyncError}
      playerError={playerError}
      positionSeconds={positionSeconds}
      seekMax={seekMax}
      selectedAudioTrack={selectedAudioTrack}
      selectedAudioTrackId={selectedAudioTrackId}
      selectedMediaFile={selectedMediaFile}
      selectedSubtitle={selectedSubtitle}
      stageRef={stageRef}
      statePlaybackErrorMessages={statePlaybackErrorMessages}
      subtitleWarning={subtitleWarning}
      title={title}
      videoRef={videoRef}
      volume={volume}
    />
  )
}
