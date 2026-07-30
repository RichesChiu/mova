import { translateCurrent } from '../../i18n'

export const PlayerPanelPlaybackError = ({
  messages,
  onRetry,
}: {
  messages: string[]
  onRetry?: () => void
}) => (
  <div aria-live="assertive" className="player-panel__playback-error" role="alert">
    <div className="player-panel__playback-error-copy">
      <strong>{translateCurrent('Playback unavailable')}</strong>
      {messages.map((message) => (
        <span key={message}>{message}</span>
      ))}
    </div>
    {onRetry ? (
      <button className="player-panel__playback-error-action" onClick={onRetry} type="button">
        {translateCurrent('Retry playback')}
      </button>
    ) : null}
  </div>
)
