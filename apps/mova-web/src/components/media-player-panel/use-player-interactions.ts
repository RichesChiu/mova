import {
  type Dispatch,
  type RefObject,
  type SetStateAction,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react'
import {
  buildFullscreenWarningMessage,
  buildPlaybackInteractionWarningMessage,
  isAutoplayBlockedError,
  isPlaybackAbortError,
} from '../../lib/player-feedback'
import { isInteractiveKeyboardTarget } from './player-utils'

const PLAYER_CONTROLS_IDLE_HIDE_MS = 1_400

interface UsePlayerInteractionsOptions {
  arePlayerControlsPinned: boolean
  isAutoplayBlocked: boolean
  isImmersive: boolean
  playbackRequestPendingRef: RefObject<boolean>
  seekMax: number
  setInteractionWarning: Dispatch<SetStateAction<string | null>>
  setIsAutoplayBlocked: Dispatch<SetStateAction<boolean>>
  setIsPlaybackRateMenuOpen: Dispatch<SetStateAction<boolean>>
  setPlaybackRate: Dispatch<SetStateAction<number>>
  setPositionSeconds: Dispatch<SetStateAction<number>>
  stageRef: RefObject<HTMLDivElement | null>
  shouldPlayRef: RefObject<boolean>
  syncPlaybackProgressRef: RefObject<(force?: boolean, isFinished?: boolean) => void>
  videoRef: RefObject<HTMLVideoElement | null>
}

export const usePlayerInteractions = ({
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
}: UsePlayerInteractionsOptions) => {
  const playerControlsHideTimeoutRef = useRef<number | null>(null)
  const [arePlayerControlsVisible, setArePlayerControlsVisible] = useState(false)

  const clearPlayerControlsHideTimeout = useCallback(() => {
    if (playerControlsHideTimeoutRef.current === null) {
      return
    }

    window.clearTimeout(playerControlsHideTimeoutRef.current)
    playerControlsHideTimeoutRef.current = null
  }, [])

  const schedulePlayerControlsHide = useCallback(() => {
    clearPlayerControlsHideTimeout()

    if (!isImmersive || arePlayerControlsPinned) {
      return
    }

    playerControlsHideTimeoutRef.current = window.setTimeout(() => {
      playerControlsHideTimeoutRef.current = null
      setArePlayerControlsVisible(false)
    }, PLAYER_CONTROLS_IDLE_HIDE_MS)
  }, [arePlayerControlsPinned, clearPlayerControlsHideTimeout, isImmersive])

  const revealPlayerControls = useCallback(() => {
    if (!isImmersive) {
      return
    }

    setArePlayerControlsVisible(true)
    schedulePlayerControlsHide()
  }, [isImmersive, schedulePlayerControlsHide])

  useEffect(() => {
    if (!isImmersive) {
      clearPlayerControlsHideTimeout()
      return
    }

    const handleMouseMove = () => revealPlayerControls()
    window.addEventListener('mousemove', handleMouseMove)
    revealPlayerControls()

    return () => {
      window.removeEventListener('mousemove', handleMouseMove)
      clearPlayerControlsHideTimeout()
    }
  }, [clearPlayerControlsHideTimeout, isImmersive, revealPlayerControls])

  useEffect(() => {
    if (!isImmersive) {
      return
    }

    if (arePlayerControlsPinned) {
      clearPlayerControlsHideTimeout()
      setArePlayerControlsVisible(true)
      return
    }

    schedulePlayerControlsHide()
  }, [
    arePlayerControlsPinned,
    clearPlayerControlsHideTimeout,
    isImmersive,
    schedulePlayerControlsHide,
  ])

  const togglePlay = useCallback(
    async (hideControlsAfterToggle = false) => {
      const video = videoRef.current
      if (!video) {
        shouldPlayRef.current = !shouldPlayRef.current
        return
      }

      const shouldPause =
        !video.paused ||
        playbackRequestPendingRef.current ||
        (video.readyState === HTMLMediaElement.HAVE_NOTHING && shouldPlayRef.current)

      if (!shouldPause) {
        const wasAutoplayBlocked = isAutoplayBlocked
        shouldPlayRef.current = true
        playbackRequestPendingRef.current = true
        setIsAutoplayBlocked(false)
        try {
          await video.play()
          playbackRequestPendingRef.current = false

          if (!shouldPlayRef.current) {
            video.pause()
            return
          }

          if (wasAutoplayBlocked || hideControlsAfterToggle) {
            clearPlayerControlsHideTimeout()
            setArePlayerControlsVisible(false)
          }
        } catch (error) {
          playbackRequestPendingRef.current = false

          if (!shouldPlayRef.current && isPlaybackAbortError(error)) {
            return
          }

          shouldPlayRef.current = false
          if (isAutoplayBlockedError(error)) {
            setIsAutoplayBlocked(true)
            setInteractionWarning(null)
            return
          }

          setInteractionWarning(buildPlaybackInteractionWarningMessage(error))
        }
        return
      }

      shouldPlayRef.current = false
      playbackRequestPendingRef.current = false
      video.pause()
      if (hideControlsAfterToggle) {
        clearPlayerControlsHideTimeout()
        setArePlayerControlsVisible(false)
      }
    },
    [
      clearPlayerControlsHideTimeout,
      isAutoplayBlocked,
      playbackRequestPendingRef,
      setInteractionWarning,
      setIsAutoplayBlocked,
      shouldPlayRef,
      videoRef,
    ],
  )

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || isInteractiveKeyboardTarget(event.target)) {
        return
      }

      if (event.code !== 'Space' && event.key !== ' ') {
        return
      }

      event.preventDefault()
      void togglePlay()
    }

    window.addEventListener('keydown', handleKeyDown)

    return () => {
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [togglePlay])

  const seekTo = (targetSeconds: number) => {
    const video = videoRef.current
    if (!video) {
      return
    }

    const nextSeconds = Math.max(0, Math.min(seekMax || targetSeconds, targetSeconds))
    video.currentTime = nextSeconds
    setPositionSeconds(Math.round(nextSeconds))
    syncPlaybackProgressRef.current?.(true, false)
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

  const changePlaybackRate = (nextPlaybackRate: number) => {
    const video = videoRef.current
    if (!video) {
      return
    }

    video.playbackRate = nextPlaybackRate
    setPlaybackRate(nextPlaybackRate)
    setIsPlaybackRateMenuOpen(false)
  }

  const toggleFullscreen = async () => {
    const stage = stageRef.current
    if (!stage) {
      return
    }

    const fullscreenTarget = stage.closest<HTMLElement>('.player-screen') ?? stage

    if (
      document.fullscreenElement === fullscreenTarget ||
      document.fullscreenElement?.contains(stage)
    ) {
      try {
        await document.exitFullscreen()
        setInteractionWarning(null)
      } catch (error) {
        setInteractionWarning(buildFullscreenWarningMessage(error))
      }
      return
    }

    if (
      typeof fullscreenTarget.requestFullscreen !== 'function' ||
      document.fullscreenEnabled === false
    ) {
      setInteractionWarning(buildFullscreenWarningMessage())
      return
    }

    try {
      await fullscreenTarget.requestFullscreen()
      setInteractionWarning(null)
    } catch (error) {
      setInteractionWarning(buildFullscreenWarningMessage(error))
    }
  }

  return {
    arePlayerControlsVisible,
    changePlaybackRate,
    changeVolume,
    seekBy,
    seekTo,
    toggleFullscreen,
    togglePlay,
  }
}
