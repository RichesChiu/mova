const PlayerGlyph = ({ children }: { children: React.ReactNode }) => (
  <svg aria-hidden="true" className="player-control-button__glyph" fill="none" viewBox="0 0 24 24">
    {children}
  </svg>
)

export const SpeakerIcon = ({ muted, volume }: { muted: boolean; volume: number }) => {
  if (muted || volume === 0) {
    return (
      <PlayerGlyph>
        <path
          d="M5 10H8L12 6V18L8 14H5V10Z"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="1.8"
        />
        <path d="M16 9L20 15" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" />
        <path d="M20 9L16 15" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" />
      </PlayerGlyph>
    )
  }

  return (
    <PlayerGlyph>
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
    </PlayerGlyph>
  )
}

export const FullscreenIcon = () => (
  <PlayerGlyph>
    <path d="M9 4H5V8" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" />
    <path d="M15 4H19V8" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" />
    <path d="M9 20H5V16" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" />
    <path d="M15 20H19V16" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" />
  </PlayerGlyph>
)

export const PlayIcon = () => (
  <PlayerGlyph>
    <path
      d="M8 6.5L17 12L8 17.5V6.5Z"
      fill="currentColor"
      stroke="currentColor"
      strokeLinejoin="round"
      strokeWidth="1.2"
    />
  </PlayerGlyph>
)

export const PauseIcon = () => (
  <PlayerGlyph>
    <path d="M8.5 6.5V17.5" stroke="currentColor" strokeLinecap="round" strokeWidth="2.2" />
    <path d="M15.5 6.5V17.5" stroke="currentColor" strokeLinecap="round" strokeWidth="2.2" />
  </PlayerGlyph>
)

export const SeekBackIcon = () => (
  <PlayerGlyph>
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
  </PlayerGlyph>
)

export const SeekForwardIcon = () => (
  <PlayerGlyph>
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
  </PlayerGlyph>
)

export const SubtitleIcon = () => (
  <PlayerGlyph>
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
  </PlayerGlyph>
)

export const AudioTrackIcon = () => (
  <PlayerGlyph>
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
  </PlayerGlyph>
)

export const EpisodeSwitchIcon = () => (
  <PlayerGlyph>
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
  </PlayerGlyph>
)
