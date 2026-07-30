import type { ComponentProps, CSSProperties, RefObject } from 'react'
import { mediaFileStreamUrl, subtitleFileStreamUrl } from '../../api/client'
import type { AudioTrack, MediaFile, SubtitleFile } from '../../api/types'
import { translateCurrent } from '../../i18n'
import { formatAudioTrackLabel } from '../../lib/audio-tracks'
import { formatDuration, formatPlaybackTime } from '../../lib/format'
import { PlayerControlMenus } from './player-control-menus'
import { PauseIcon, PlayIcon, SeekBackIcon, SeekForwardIcon, SpeakerIcon } from './player-icons'
import { PlayerPanelPlaybackError } from './player-playback-error'
import {
  formatVideoMeta,
  normalizeSubtitleTrackLanguage,
  releasePointerButtonFocus,
  renderSubtitleLabel,
} from './player-utils'

interface PlayerPanelViewProps {
  arePlayerControlsVisible: boolean
  audioTrackNotice: string | null
  bufferedSeconds: number
  bufferingStatusMessage: string
  canSkipIntro: boolean
  centeredPlaybackErrorMessages: string[]
  controlMenus: ComponentProps<typeof PlayerControlMenus>
  durationSeconds: number | null
  interactionWarning: string | null
  isAutoplayBlocked: boolean
  isBuffering: boolean
  isImmersive: boolean
  isMuted: boolean
  isPlaying: boolean
  mediaFiles: MediaFile[]
  mediaFilesLoading: boolean
  onChangeVolume: (volume: number) => void
  onEnded: () => void
  onLoadedMetadata: () => void
  onPause: () => void
  onPlayerError: () => void
  onRetryCurrentSource: () => void
  onSeekBy: (deltaSeconds: number) => void
  onSeekTo: (targetSeconds: number) => void
  onSkipIntro: () => void
  onSubtitleTrackError: () => void
  onSwitchMediaFile: (mediaFileId: number) => void
  onTimeUpdate: () => void
  onTogglePlay: (hideControlsAfterToggle?: boolean) => Promise<void>
  playbackSyncError: string | null
  playerError: string | null
  positionSeconds: number
  seekMax: number
  selectedAudioTrack: AudioTrack | null
  selectedAudioTrackId: number | null
  selectedMediaFile: MediaFile | null
  selectedSubtitle: SubtitleFile | null
  stageRef: RefObject<HTMLDivElement | null>
  statePlaybackErrorMessages: string[]
  subtitleWarning: string | null
  title: string
  videoRef: RefObject<HTMLVideoElement | null>
  volume: number
}

export const PlayerPanelView = ({
  arePlayerControlsVisible,
  audioTrackNotice,
  bufferedSeconds,
  bufferingStatusMessage,
  canSkipIntro,
  centeredPlaybackErrorMessages,
  controlMenus,
  durationSeconds,
  interactionWarning,
  isAutoplayBlocked,
  isBuffering,
  isImmersive,
  isMuted,
  isPlaying,
  mediaFiles,
  mediaFilesLoading,
  onChangeVolume,
  onEnded,
  onLoadedMetadata,
  onPause,
  onPlayerError,
  onRetryCurrentSource,
  onSeekBy,
  onSeekTo,
  onSkipIntro,
  onSubtitleTrackError,
  onSwitchMediaFile,
  onTimeUpdate,
  onTogglePlay,
  playbackSyncError,
  playerError,
  positionSeconds,
  seekMax,
  selectedAudioTrack,
  selectedAudioTrackId,
  selectedMediaFile,
  selectedSubtitle,
  stageRef,
  statePlaybackErrorMessages,
  subtitleWarning,
  title,
  videoRef,
  volume,
}: PlayerPanelViewProps) => {
  const playedProgressPercent = seekMax > 0 ? Math.min(100, (positionSeconds / seekMax) * 100) : 0
  const bufferedProgressPercent =
    seekMax > 0 ? Math.min(100, (Math.max(bufferedSeconds, positionSeconds) / seekMax) * 100) : 0
  const timelineStyle = {
    '--player-range-buffered': `${Math.max(playedProgressPercent, bufferedProgressPercent)}%`,
    '--player-range-played': `${playedProgressPercent}%`,
  } as CSSProperties
  const shouldShowTopOverlay =
    canSkipIntro ||
    playbackSyncError !== null ||
    interactionWarning !== null ||
    subtitleWarning !== null ||
    (!isBuffering && audioTrackNotice !== null)

  return (
    <section className={isImmersive ? 'player-panel player-panel--immersive' : 'player-panel'}>
      {!isImmersive ? (
        <div className="catalog-block__header">
          <div>
            <h3>{translateCurrent('Playback')}</h3>
          </div>
        </div>
      ) : null}

      {mediaFilesLoading ? <p className="muted">{translateCurrent('Loading player…')}</p> : null}

      {statePlaybackErrorMessages.length > 0 ? (
        <div className="player-panel__state-center">
          <PlayerPanelPlaybackError messages={statePlaybackErrorMessages} />
        </div>
      ) : null}

      {mediaFiles.length === 0 && !mediaFilesLoading ? (
        <div className="catalog-block__empty">
          <p className="muted">
            {translateCurrent('No playable media files are linked to this item yet.')}
          </p>
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
          <div className="player-stage" onPointerUp={releasePointerButtonFocus} ref={stageRef}>
            <div className="player-stage__media">
              {isImmersive && shouldShowTopOverlay ? (
                <div className="player-panel__overlay">
                  <div className="player-panel__overlay-status">
                    {canSkipIntro ? (
                      <button
                        className="player-panel__floating-action"
                        onClick={onSkipIntro}
                        type="button"
                      >
                        {translateCurrent('Skip Intro')}
                      </button>
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
                </div>
              ) : null}

              {isImmersive && isBuffering && !isAutoplayBlocked && !playerError ? (
                <div aria-live="polite" className="player-panel__center-status" role="status">
                  <p className="player-panel__status-badge">{bufferingStatusMessage}</p>
                </div>
              ) : null}

              {isImmersive && arePlayerControlsVisible && !isBuffering && !playerError ? (
                <div className="player-panel__center-status player-panel__center-status--interactive">
                  <button
                    aria-label={translateCurrent(isPlaying ? 'Pause playback' : 'Start playback')}
                    className="player-panel__center-playback-control"
                    onClick={() => void onTogglePlay(true)}
                    type="button"
                  >
                    {isPlaying ? <PauseIcon /> : <PlayIcon />}
                  </button>
                </div>
              ) : null}

              {centeredPlaybackErrorMessages.length > 0 ? (
                <div className="player-panel__center-status player-panel__center-status--interactive">
                  <PlayerPanelPlaybackError
                    messages={centeredPlaybackErrorMessages}
                    onRetry={playerError ? onRetryCurrentSource : undefined}
                  />
                </div>
              ) : null}

              {/* biome-ignore lint/a11y/useMediaCaption: 当前播放器允许“关闭字幕”，未选中时不会挂载活动字幕轨道。 */}
              <video
                className="player-stage__video"
                controls={!isImmersive}
                controlsList="nodownload noplaybackrate"
                disablePictureInPicture={isImmersive}
                disableRemotePlayback={isImmersive}
                onClick={isImmersive ? () => void onTogglePlay() : undefined}
                onEnded={onEnded}
                onError={onPlayerError}
                onLoadedMetadata={onLoadedMetadata}
                onPause={onPause}
                onTimeUpdate={onTimeUpdate}
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
                    onError={onSubtitleTrackError}
                    src={subtitleFileStreamUrl(selectedSubtitle.id)}
                    srcLang={normalizeSubtitleTrackLanguage(selectedSubtitle.language)}
                  />
                ) : null}
                {translateCurrent('Your browser does not support HTML5 video playback.')}
              </video>
            </div>

            {isImmersive ? (
              <div
                className={
                  arePlayerControlsVisible
                    ? 'player-stage__controls player-stage__controls--visible'
                    : 'player-stage__controls'
                }
              >
                <div className="player-stage__control-row">
                  <div className="player-toolbar-cluster">
                    <div className="player-toolbar-pill player-toolbar-pill--primary">
                      <button
                        aria-label={
                          isPlaying
                            ? translateCurrent('Pause playback')
                            : translateCurrent('Start playback')
                        }
                        className="player-control-button player-control-button--icon player-control-button--toolbar player-control-button--primary"
                        onClick={() => void onTogglePlay()}
                        type="button"
                      >
                        {isPlaying ? <PauseIcon /> : <PlayIcon />}
                      </button>
                      <button
                        aria-label={translateCurrent('Seek backward 10 seconds')}
                        className="player-control-button player-control-button--icon player-control-button--toolbar player-control-button--seek"
                        onClick={() => onSeekBy(-10)}
                        title={translateCurrent('Back 10 seconds')}
                        type="button"
                      >
                        <SeekBackIcon />
                      </button>
                      <button
                        aria-label={translateCurrent('Seek forward 10 seconds')}
                        className="player-control-button player-control-button--icon player-control-button--toolbar player-control-button--seek"
                        onClick={() => onSeekBy(10)}
                        title={translateCurrent('Forward 10 seconds')}
                        type="button"
                      >
                        <SeekForwardIcon />
                      </button>
                      <div className="player-volume-control">
                        <button
                          aria-label={translateCurrent('Adjust volume')}
                          className="player-control-button player-control-button--icon player-control-button--toolbar"
                          type="button"
                          title={
                            selectedAudioTrack
                              ? translateCurrent('Selected audio: {{name}}', {
                                  name: formatAudioTrackLabel(selectedAudioTrack),
                                })
                              : translateCurrent('Adjust volume')
                          }
                        >
                          <SpeakerIcon muted={isMuted} volume={volume} />
                        </button>
                        <div className="player-volume-control__slider">
                          <input
                            aria-label={translateCurrent('Adjust volume')}
                            className="player-range player-range--volume-inline"
                            max={1}
                            min={0}
                            onChange={(event) => onChangeVolume(Number(event.target.value))}
                            step={0.05}
                            type="range"
                            value={isMuted ? 0 : volume}
                          />
                        </div>
                      </div>
                      <span className="player-stage__time">
                        {formatPlaybackTime(positionSeconds)} /{' '}
                        {formatPlaybackTime(durationSeconds)}
                      </span>
                    </div>
                  </div>

                  <div className="player-stage__timeline">
                    <input
                      aria-label={translateCurrent('Seek playback position')}
                      className="player-range player-range--timeline"
                      max={seekMax || 0}
                      min={0}
                      onChange={(event) => onSeekTo(Number(event.target.value))}
                      step={1}
                      style={timelineStyle}
                      type="range"
                      value={Math.min(positionSeconds, seekMax || positionSeconds)}
                    />
                  </div>

                  <div className="player-toolbar-cluster player-toolbar-cluster--right">
                    <div className="player-toolbar-pill player-toolbar-pill--tools">
                      <PlayerControlMenus {...controlMenus} />
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
                  {formatVideoMeta(selectedMediaFile) || translateCurrent('Playable source')}
                </span>
              </div>
              <div className="player-panel__info player-panel__info--compact">
                <span className="muted">{translateCurrent('Current')}</span>
                <strong>{formatDuration(positionSeconds)}</strong>
              </div>
              <div className="player-panel__info player-panel__info--compact">
                <span className="muted">{translateCurrent('Duration')}</span>
                <strong>{formatDuration(durationSeconds)}</strong>
              </div>
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
            <p className="player-panel__status-badge">{bufferingStatusMessage}</p>
          ) : null}

          {mediaFiles.length > 1 && !isImmersive ? (
            <div className="player-source-list">
              {mediaFiles.map((file) => {
                const isActive = file.id === selectedMediaFile.id

                return (
                  <button
                    className={isActive ? 'player-source player-source--active' : 'player-source'}
                    key={file.id}
                    onClick={() => onSwitchMediaFile(file.id)}
                    type="button"
                  >
                    <span className="player-source__title">
                      {file.container?.toUpperCase() ?? translateCurrent('FILE')}
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
