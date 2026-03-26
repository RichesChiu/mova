import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import {
  getMediaItemPlaybackProgress,
  listMediaItemFiles,
  mediaFileStreamUrl,
  updateMediaItemPlaybackProgress,
} from '../../api/client'
import type { MediaFile } from '../../api/types'
import { formatDuration } from '../../lib/format'

const PROGRESS_SYNC_INTERVAL_SECONDS = 10
// Subtitles are not wired yet; keep an empty track mounted so the custom control surface can
// reserve the captions slot without exposing browser download/caption UI inconsistently.
const EMPTY_CAPTIONS_TRACK = 'data:text/vtt;charset=utf-8,WEBVTT%0A%0A'

interface MediaPlayerPanelProps {
  mediaItemId: number
  title: string
  startMode?: 'resume' | 'from-start'
  variant?: 'panel' | 'immersive'
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

export const MediaPlayerPanel = ({
  mediaItemId,
  startMode = 'resume',
  title,
  variant = 'panel',
}: MediaPlayerPanelProps) => {
  const queryClient = useQueryClient()
  const stageRef = useRef<HTMLDivElement | null>(null)
  const videoRef = useRef<HTMLVideoElement | null>(null)
  const restoredForFileRef = useRef<number | null>(null)
  const lastReportedSecondsRef = useRef(-1)
  const hasSubmittedProgressRef = useRef(false)
  const [selectedMediaFileId, setSelectedMediaFileId] = useState<number | null>(null)
  const [playerError, setPlayerError] = useState<string | null>(null)
  const [positionSeconds, setPositionSeconds] = useState(0)
  const [durationSeconds, setDurationSeconds] = useState<number | null>(null)
  const [isPlaying, setIsPlaying] = useState(false)
  const [isMuted, setIsMuted] = useState(false)
  const [volume, setVolume] = useState(1)
  const [isFullscreen, setIsFullscreen] = useState(false)

  const mediaFilesQuery = useQuery({
    queryKey: ['media-item-files', mediaItemId],
    queryFn: () => listMediaItemFiles(mediaItemId),
  })
  const playbackProgressQuery = useQuery({
    queryKey: ['media-item-playback-progress', mediaItemId],
    queryFn: () => getMediaItemPlaybackProgress(mediaItemId),
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
      queryClient.setQueryData(['media-item-playback-progress', mediaItemId], progress)
    },
  })

  const mediaFiles = mediaFilesQuery.data ?? []
  const selectedMediaFile =
    mediaFiles.find((file) => file.id === selectedMediaFileId) ?? mediaFiles[0] ?? null
  const selectedMediaFileDuration = selectedMediaFile?.duration_seconds ?? null

  useEffect(() => {
    if (mediaFiles.length === 0) {
      setSelectedMediaFileId(null)
      return
    }

    // Prefer the file tied to the saved playback progress so multi-file items reopen on the same
    // source instead of snapping back to the first file after every refresh.
    const playbackProgress = playbackProgressQuery.data
    const preferredFile =
      (playbackProgress && mediaFiles.find((file) => file.id === playbackProgress.media_file_id)) ??
      mediaFiles[0]

    setSelectedMediaFileId((current) =>
      current && mediaFiles.some((file) => file.id === current) ? current : preferredFile.id,
    )
  }, [mediaFiles, playbackProgressQuery.data])

  useEffect(() => {
    restoredForFileRef.current = null
    lastReportedSecondsRef.current = -1
    setPlayerError(null)
    setPositionSeconds(0)
    setDurationSeconds(selectedMediaFileId === null ? null : selectedMediaFileDuration)
    setIsPlaying(false)
  }, [selectedMediaFileDuration, selectedMediaFileId])

  useEffect(() => {
    const video = videoRef.current
    if (!video) {
      return
    }

    const syncVolumeState = () => {
      setIsMuted(video.muted || video.volume === 0)
      setVolume(video.volume)
    }

    const handlePlay = () => setIsPlaying(true)
    const handlePause = () => setIsPlaying(false)
    const handleFullscreenChange = () => {
      setIsFullscreen(document.fullscreenElement === stageRef.current)
    }

    syncVolumeState()
    video.addEventListener('play', handlePlay)
    video.addEventListener('pause', handlePause)
    video.addEventListener('volumechange', syncVolumeState)
    document.addEventListener('fullscreenchange', handleFullscreenChange)

    return () => {
      video.removeEventListener('play', handlePlay)
      video.removeEventListener('pause', handlePause)
      video.removeEventListener('volumechange', syncVolumeState)
      document.removeEventListener('fullscreenchange', handleFullscreenChange)
    }
  }, [])

  useEffect(() => {
    return () => {
      if (hasSubmittedProgressRef.current) {
        void queryClient.invalidateQueries({ queryKey: ['continue-watching'] })
        void queryClient.invalidateQueries({ queryKey: ['watch-history'] })
        void queryClient.invalidateQueries({
          queryKey: ['media-item-playback-progress', mediaItemId],
        })
      }
    }
  }, [mediaItemId, queryClient])

  const syncPlaybackProgress = (force = false, isFinished = false) => {
    const video = videoRef.current
    if (!video || !selectedMediaFile) {
      return
    }

    const measuredDuration =
      Number.isFinite(video.duration) && video.duration > 0
        ? Math.round(video.duration)
        : (selectedMediaFile.duration_seconds ?? undefined)
    const measuredPosition = Math.max(
      0,
      Math.round(
        measuredDuration ? Math.min(video.currentTime, measuredDuration) : video.currentTime,
      ),
    )

    if (
      !force &&
      Math.abs(measuredPosition - lastReportedSecondsRef.current) < PROGRESS_SYNC_INTERVAL_SECONDS
    ) {
      return
    }

    playbackProgressMutation.mutate({
      media_file_id: selectedMediaFile.id,
      position_seconds: measuredPosition,
      duration_seconds: measuredDuration,
      is_finished: isFinished,
    })
  }

  const handleLoadedMetadata = () => {
    const video = videoRef.current
    const playbackProgress = playbackProgressQuery.data
    if (!video || !selectedMediaFile) {
      return
    }

    if (Number.isFinite(video.duration) && video.duration > 0) {
      setDurationSeconds(Math.round(video.duration))
    }

    if (startMode === 'from-start') {
      if (restoredForFileRef.current === selectedMediaFile.id) {
        return
      }

      // "Play from beginning" should win over any stored resume point, but only once per file
      // selection so metadata reloads do not keep rewinding the same source.
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

    const safeResumePosition = Math.max(0, playbackProgress.position_seconds - 2)
    video.currentTime = safeResumePosition
    setPositionSeconds(Math.round(safeResumePosition))
    lastReportedSecondsRef.current = playbackProgress.position_seconds
    restoredForFileRef.current = selectedMediaFile.id
  }

  const handleTimeUpdate = () => {
    const video = videoRef.current
    if (!video) {
      return
    }

    setPositionSeconds(Math.max(0, Math.round(video.currentTime)))
    syncPlaybackProgress(false, false)
  }

  const handlePause = () => {
    syncPlaybackProgress(true, false)
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
    syncPlaybackProgress(true, true)
  }

  const handlePlayerError = () => {
    setPlayerError(
      'This browser could not play the selected file. Try another version or container.',
    )
  }

  const isImmersive = variant === 'immersive'
  const seekMax = Math.max(0, durationSeconds ?? selectedMediaFileDuration ?? 0)

  const togglePlay = async () => {
    const video = videoRef.current
    if (!video) {
      return
    }

    if (video.paused) {
      try {
        await video.play()
      } catch {
        setPlayerError('Playback was blocked by the browser. Click play again to continue.')
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
      await document.exitFullscreen()
      return
    }

    await stage.requestFullscreen()
  }

  return (
    <section className={isImmersive ? 'player-panel player-panel--immersive' : 'player-panel'}>
      {!isImmersive ? (
        <div className="catalog-block__header">
          <div>
            <h3>Playback</h3>
            <p className="muted">Uses direct browser playback with automatic progress sync.</p>
          </div>
        </div>
      ) : null}

      {mediaFilesQuery.isLoading || playbackProgressQuery.isLoading ? (
        <p className="muted">Loading player…</p>
      ) : null}

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
              {isImmersive && (mediaFiles.length > 1 || playerError) ? (
                <div className="player-panel__overlay">
                  <div className="player-panel__overlay-status">
                    {playerError ? <p className="callout callout--danger">{playerError}</p> : null}
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
                            onClick={() => setSelectedMediaFileId(file.id)}
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
                src={mediaFileStreamUrl(selectedMediaFile.id)}
              >
                <track
                  default
                  kind="captions"
                  label="Captions unavailable"
                  src={EMPTY_CAPTIONS_TRACK}
                  srcLang="en"
                />
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
                      <div className="player-volume-control">
                        <button
                          aria-label="Adjust volume"
                          className="player-control-button player-control-button--icon player-control-button--toolbar"
                          type="button"
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
            <p className="callout callout--danger">{playerError}</p>
          ) : null}

          {mediaFiles.length > 1 && !isImmersive ? (
            <div className="player-source-list">
              {mediaFiles.map((file) => {
                const isActive = file.id === selectedMediaFile.id

                return (
                  <button
                    className={isActive ? 'player-source player-source--active' : 'player-source'}
                    key={file.id}
                    onClick={() => setSelectedMediaFileId(file.id)}
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
