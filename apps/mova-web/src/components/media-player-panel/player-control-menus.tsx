import { useEffect, useRef } from 'react'
import type { AudioTrack, SubtitleFile } from '../../api/types'
import { translateCurrent } from '../../i18n'
import {
  buildAudioTrackLoadErrorMessage,
  formatAudioTrackLabel,
  formatAudioTrackMeta,
} from '../../lib/audio-tracks'
import { GlassMenu } from '../glass-menu'
import { AudioTrackIcon, EpisodeSwitchIcon, FullscreenIcon, SubtitleIcon } from './player-icons'
import { renderSubtitleLabel } from './player-utils'

export const PLAYBACK_RATE_OPTIONS = [0.5, 0.75, 1, 1.25, 1.5, 2] as const

type PlayerMenuName = 'episode' | 'audio' | 'subtitle' | 'rate'

interface PlayerControlMenusProps {
  audioTracks: AudioTrack[]
  audioTracksError: boolean
  audioTracksLoading: boolean
  currentAudioSelectionLabel: string
  episodeSwitchOptions: Array<{
    label: string
    mediaItemId: number
  }>
  isAudioMenuOpen: boolean
  isEpisodeMenuOpen: boolean
  isFullscreen: boolean
  isPlaybackRateMenuOpen: boolean
  isSubtitleMenuOpen: boolean
  mediaItemId: number
  nextEpisode: {
    label: string
    mediaItemId: number
    seasonNumber: number
    episodeNumber: number
  } | null
  onAudioMenuOpenChange: (isOpen: boolean) => void
  onEpisodeMenuOpenChange: (isOpen: boolean) => void
  onGoToNextEpisode: () => void
  onPlaybackRateMenuOpenChange: (isOpen: boolean) => void
  onSelectAudioTrack: (trackId: number | null) => void
  onSelectEpisode?: (mediaItemId: number) => void
  onSelectPlaybackRate: (rate: number) => void
  onSelectSubtitle: (subtitleId: number | null) => void
  onSubtitleMenuOpenChange: (isOpen: boolean) => void
  onToggleFullscreen: () => void
  playbackRate: number
  selectedAudioTrackId: number | null
  selectedSubtitleId: number | null
  shouldRenderAudioMenu: boolean
  subtitleFiles: SubtitleFile[]
  subtitleFilesError: string | null
  subtitleFilesLoading: boolean
}

export const PlayerControlMenus = ({
  audioTracks,
  audioTracksError,
  audioTracksLoading,
  currentAudioSelectionLabel,
  episodeSwitchOptions,
  isAudioMenuOpen,
  isEpisodeMenuOpen,
  isFullscreen,
  isPlaybackRateMenuOpen,
  isSubtitleMenuOpen,
  mediaItemId,
  nextEpisode,
  onAudioMenuOpenChange,
  onEpisodeMenuOpenChange,
  onGoToNextEpisode,
  onPlaybackRateMenuOpenChange,
  onSelectAudioTrack,
  onSelectEpisode,
  onSelectPlaybackRate,
  onSelectSubtitle,
  onSubtitleMenuOpenChange,
  onToggleFullscreen,
  playbackRate,
  selectedAudioTrackId,
  selectedSubtitleId,
  shouldRenderAudioMenu,
  subtitleFiles,
  subtitleFilesError,
  subtitleFilesLoading,
}: PlayerControlMenusProps) => {
  const episodeListRef = useRef<HTMLDivElement | null>(null)
  const setMenuOpen = (menu: PlayerMenuName, isOpen: boolean) => {
    onEpisodeMenuOpenChange(menu === 'episode' ? isOpen : false)
    onAudioMenuOpenChange(menu === 'audio' ? isOpen : false)
    onSubtitleMenuOpenChange(menu === 'subtitle' ? isOpen : false)
    onPlaybackRateMenuOpenChange(menu === 'rate' ? isOpen : false)
  }

  useEffect(() => {
    if (!isEpisodeMenuOpen) {
      return
    }

    const animationFrame = window.requestAnimationFrame(() => {
      const list = episodeListRef.current
      const currentOption = list?.querySelector<HTMLElement>(
        `[data-media-item-id="${mediaItemId}"]`,
      )
      if (!list || !currentOption) {
        return
      }

      const listRect = list.getBoundingClientRect()
      const optionRect = currentOption.getBoundingClientRect()
      list.scrollTop +=
        optionRect.top - listRect.top - (list.clientHeight - currentOption.clientHeight) / 2
    })

    return () => window.cancelAnimationFrame(animationFrame)
  }, [isEpisodeMenuOpen, mediaItemId])

  return (
    <>
      {episodeSwitchOptions.length > 1 && onSelectEpisode ? (
        <GlassMenu
          ariaLabel={translateCurrent('Switch episode')}
          isOpen={isEpisodeMenuOpen}
          onOpenChange={(isOpen) => setMenuOpen('episode', isOpen)}
          popoverClassName="player-popover-menu__bubble"
          rootClassName={
            isEpisodeMenuOpen
              ? 'player-popover-menu player-popover-menu--open'
              : 'player-popover-menu'
          }
          trigger={(triggerProps) => (
            <button
              {...triggerProps}
              aria-label={translateCurrent('Switch episode')}
              className={
                isEpisodeMenuOpen
                  ? 'player-control-button player-control-button--icon player-control-button--toolbar player-control-button--active'
                  : 'player-control-button player-control-button--icon player-control-button--toolbar'
              }
              type="button"
            >
              <EpisodeSwitchIcon />
            </button>
          )}
        >
          {(closeMenu) => (
            <div className="player-popover-menu__list scrollbar-thin" ref={episodeListRef}>
              {episodeSwitchOptions.map((episode) => {
                const isCurrentEpisode = episode.mediaItemId === mediaItemId
                return (
                  <button
                    aria-current={isCurrentEpisode ? 'true' : undefined}
                    className={
                      isCurrentEpisode
                        ? 'player-popover-menu__option player-popover-menu__option--active'
                        : 'player-popover-menu__option'
                    }
                    data-media-item-id={episode.mediaItemId}
                    key={episode.mediaItemId}
                    onClick={() => {
                      closeMenu()
                      if (!isCurrentEpisode) {
                        onSelectEpisode(episode.mediaItemId)
                      }
                    }}
                    role="menuitem"
                    type="button"
                  >
                    <span>{episode.label}</span>
                  </button>
                )
              })}
            </div>
          )}
        </GlassMenu>
      ) : null}

      {shouldRenderAudioMenu ? (
        <GlassMenu
          ariaLabel={translateCurrent('Select audio track')}
          isOpen={isAudioMenuOpen}
          onOpenChange={(isOpen) => setMenuOpen('audio', isOpen)}
          popoverClassName="player-popover-menu__bubble"
          rootClassName={
            isAudioMenuOpen
              ? 'player-popover-menu player-popover-menu--open'
              : 'player-popover-menu'
          }
          trigger={(triggerProps) => (
            <button
              {...triggerProps}
              aria-label={translateCurrent('Select audio track')}
              className={
                selectedAudioTrackId !== null || isAudioMenuOpen
                  ? 'player-control-button player-control-button--icon player-control-button--toolbar player-control-button--active'
                  : 'player-control-button player-control-button--icon player-control-button--toolbar'
              }
              title={translateCurrent('Audio: {{name}}', {
                name: currentAudioSelectionLabel,
              })}
              type="button"
            >
              <AudioTrackIcon />
            </button>
          )}
        >
          {(closeMenu) => (
            <>
              <div className="player-popover-menu__header">
                <strong>{translateCurrent('Audio')}</strong>
                <small>
                  {audioTracksLoading
                    ? translateCurrent('Loading embedded audio tracks…')
                    : translateCurrent('Current: {{name}}', {
                        name: currentAudioSelectionLabel,
                      })}
                </small>
              </div>
              <div className="player-popover-menu__list scrollbar-thin">
                <button
                  className={
                    selectedAudioTrackId === null
                      ? 'player-popover-menu__option player-popover-menu__option--active'
                      : 'player-popover-menu__option'
                  }
                  onClick={() => {
                    closeMenu()
                    onSelectAudioTrack(null)
                  }}
                  role="menuitem"
                  type="button"
                >
                  <span>{translateCurrent('Original default track')}</span>
                  <small>{translateCurrent("Use the source file's default audio")}</small>
                </button>
                {audioTracks.map((audioTrack) => (
                  <button
                    className={
                      selectedAudioTrackId === audioTrack.id
                        ? 'player-popover-menu__option player-popover-menu__option--active'
                        : 'player-popover-menu__option'
                    }
                    key={audioTrack.id}
                    onClick={() => {
                      closeMenu()
                      onSelectAudioTrack(audioTrack.id)
                    }}
                    role="menuitem"
                    type="button"
                  >
                    <span>{formatAudioTrackLabel(audioTrack)}</span>
                    <small>
                      {formatAudioTrackMeta(audioTrack) || translateCurrent('Embedded')}
                    </small>
                  </button>
                ))}
                {audioTracks.length === 0 && !audioTracksLoading ? (
                  <p className="player-popover-menu__empty">
                    {audioTracksError
                      ? buildAudioTrackLoadErrorMessage()
                      : translateCurrent('No alternate audio tracks found.')}
                  </p>
                ) : null}
              </div>
            </>
          )}
        </GlassMenu>
      ) : null}

      <GlassMenu
        ariaLabel={translateCurrent('Select subtitles')}
        isOpen={isSubtitleMenuOpen}
        onOpenChange={(isOpen) => setMenuOpen('subtitle', isOpen)}
        popoverClassName="player-popover-menu__bubble"
        rootClassName={
          isSubtitleMenuOpen
            ? 'player-popover-menu player-popover-menu--open'
            : 'player-popover-menu'
        }
        trigger={(triggerProps) => (
          <button
            {...triggerProps}
            aria-label={translateCurrent('Select subtitles')}
            className={
              selectedSubtitleId !== null || isSubtitleMenuOpen
                ? 'player-control-button player-control-button--icon player-control-button--toolbar player-control-button--active'
                : 'player-control-button player-control-button--icon player-control-button--toolbar'
            }
            type="button"
          >
            <SubtitleIcon />
          </button>
        )}
      >
        {(closeMenu) => (
          <div className="player-popover-menu__list scrollbar-thin">
            <button
              className={
                selectedSubtitleId === null
                  ? 'player-popover-menu__option player-popover-menu__option--active'
                  : 'player-popover-menu__option'
              }
              onClick={() => {
                closeMenu()
                onSelectSubtitle(null)
              }}
              role="menuitem"
              type="button"
            >
              {translateCurrent('Off')}
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
                  closeMenu()
                  onSelectSubtitle(subtitle.id)
                }}
                role="menuitem"
                type="button"
              >
                <span>{renderSubtitleLabel(subtitle) || translateCurrent('Unknown subtitle')}</span>
                <small>
                  {subtitle.source_kind === 'embedded'
                    ? translateCurrent('Embedded')
                    : translateCurrent('External')}
                </small>
              </button>
            ))}
            {subtitleFiles.length === 0 && !subtitleFilesLoading && !subtitleFilesError ? (
              <p className="player-popover-menu__empty">
                {translateCurrent('No subtitles found.')}
              </p>
            ) : null}
            {subtitleFilesError ? (
              <p className="player-popover-menu__empty">{subtitleFilesError}</p>
            ) : null}
          </div>
        )}
      </GlassMenu>

      <GlassMenu
        ariaLabel={translateCurrent('Playback Speed')}
        isOpen={isPlaybackRateMenuOpen}
        onOpenChange={(isOpen) => setMenuOpen('rate', isOpen)}
        popoverClassName="player-popover-menu__bubble player-popover-menu__bubble--compact"
        rootClassName={
          isPlaybackRateMenuOpen
            ? 'player-popover-menu player-popover-menu--open'
            : 'player-popover-menu'
        }
        trigger={(triggerProps) => (
          <button
            {...triggerProps}
            aria-label={translateCurrent('Playback speed: {{rate}}', {
              rate: `${playbackRate}×`,
            })}
            className={
              playbackRate !== 1 || isPlaybackRateMenuOpen
                ? 'player-control-button player-control-button--toolbar player-control-button--rate player-control-button--active'
                : 'player-control-button player-control-button--toolbar player-control-button--rate'
            }
            title={translateCurrent('Playback speed: {{rate}}', {
              rate: `${playbackRate}×`,
            })}
            type="button"
          >
            {playbackRate}×
          </button>
        )}
      >
        {(closeMenu) => (
          <>
            <div className="player-popover-menu__header">
              <strong>{translateCurrent('Playback Speed')}</strong>
            </div>
            <div className="player-popover-menu__list">
              {PLAYBACK_RATE_OPTIONS.map((rate) => {
                const isCurrentRate = rate === playbackRate
                return (
                  <button
                    aria-checked={isCurrentRate}
                    className={
                      isCurrentRate
                        ? 'player-popover-menu__option player-popover-menu__option--active'
                        : 'player-popover-menu__option'
                    }
                    key={rate}
                    onClick={() => {
                      closeMenu()
                      onSelectPlaybackRate(rate)
                    }}
                    role="menuitemradio"
                    type="button"
                  >
                    <span>{rate}×</span>
                  </button>
                )
              })}
            </div>
          </>
        )}
      </GlassMenu>

      <button
        aria-label={
          isFullscreen ? translateCurrent('Exit fullscreen') : translateCurrent('Enter fullscreen')
        }
        className="player-control-button player-control-button--icon player-control-button--toolbar"
        onClick={onToggleFullscreen}
        title={
          isFullscreen ? translateCurrent('Exit fullscreen') : translateCurrent('Enter fullscreen')
        }
        type="button"
      >
        <FullscreenIcon />
      </button>

      {nextEpisode && onSelectEpisode ? (
        <button
          aria-label={translateCurrent('Play next episode: {{label}}', {
            label: nextEpisode.label,
          })}
          className="player-control-button player-control-button--toolbar player-control-button--next"
          onClick={onGoToNextEpisode}
          type="button"
        >
          {translateCurrent('Next Episode')}
        </button>
      ) : null}
    </>
  )
}
