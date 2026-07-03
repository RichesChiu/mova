const ICON_SPRITE = '/assets/mova-icons/sprite/mova-icons-sprite.svg'

export type IconName =
  | 'private-library'
  | 'device-access'
  | 'transcode'
  | 'permissions'
  | 'metadata'
  | 'self-host'
  | 'tv'
  | 'mobile'
  | 'tablet'
  | 'desktop'
  | 'rocket'
  | 'multi-terminal'
  | 'data-shield'
  | 'scalable'
  | 'home'
  | 'library'
  | 'movie'
  | 'series'
  | 'music'
  | 'photo'
  | 'playlist'
  | 'user'
  | 'settings'
  | 'search'
  | 'bell'
  | 'docs'
  | 'arrow-right'
  | 'play'

export function MovaIcon({
  name,
  className,
  title,
}: {
  name: IconName
  className?: string
  title?: string
}) {
  const iconClassName = ['mova-icon', className].filter(Boolean).join(' ')

  return (
    <svg
      className={iconClassName}
      role={title ? 'img' : undefined}
      aria-hidden={title ? undefined : true}
      aria-label={title}
    >
      <use href={`${ICON_SPRITE}#mova-icon-${name}`} />
    </svg>
  )
}
