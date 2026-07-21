import './MovaIcon.css'

const ICON_SPRITE = '/assets/mova-icons/sprite/mova-icons-sprite.svg'

export type IconName =
  | 'private-library'
  | 'device-access'
  | 'transcode'
  | 'permissions'
  | 'metadata'
  | 'home'
  | 'arrow-right'

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
