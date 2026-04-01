import { Link } from 'react-router-dom'

type EpisodeCardStatus = 'idle' | 'progress' | 'complete'

interface EpisodeCardProps {
  artworkAlt: string
  artworkSrc?: string | null
  className?: string
  description?: string | null
  href?: string
  placeholderLabel: string
  progressPercent?: number | null
  status?: EpisodeCardStatus
  title?: string | null
  metaLabel?: string | null
}

interface EpisodeCardSkeletonProps {
  metaLabel?: string | null
  placeholderLabel: string
}

export const EpisodeCard = ({
  artworkAlt,
  artworkSrc,
  className,
  description,
  href,
  placeholderLabel,
  progressPercent,
  status = 'idle',
  title,
  metaLabel,
}: EpisodeCardProps) => {
  const normalizedProgress =
    typeof progressPercent === 'number' ? Math.max(0, Math.min(100, progressPercent)) : 0
  const classes = [
    'episode-card',
    href ? 'episode-card--available' : 'episode-card--missing',
    `episode-card--${status}`,
    className ?? null,
  ]
    .filter(Boolean)
    .join(' ')

  const content = (
    <>
      <div className="episode-card__artwork">
        {artworkSrc ? (
          <img alt={artworkAlt} loading="lazy" src={artworkSrc} />
        ) : (
          <div className="episode-card__placeholder">
            <span>{placeholderLabel}</span>
          </div>
        )}
      </div>
      <div className="episode-card__content">
        <div className="episode-card__status-badge" />
        <div className="episode-card__body">
          {metaLabel ? <p className="episode-card__meta">{metaLabel}</p> : null}
          {title ? (
            <p className="episode-card__title" title={title}>
              {title}
            </p>
          ) : null}
          {description ? (
            <p className="episode-card__description" title={description}>
              {description}
            </p>
          ) : null}
        </div>
        <div className="episode-card__progress">
          <span style={{ width: `${normalizedProgress}%` }} />
        </div>
      </div>
    </>
  )

  if (href) {
    return (
      <Link className={classes} to={href}>
        {content}
      </Link>
    )
  }

  return (
    <div aria-disabled="true" className={classes}>
      {content}
    </div>
  )
}

export const EpisodeCardSkeleton = ({ metaLabel, placeholderLabel }: EpisodeCardSkeletonProps) => {
  return (
    <div aria-hidden="true" className="episode-card episode-card--loading">
      <div className="episode-card__artwork">
        <div className="episode-card__placeholder episode-card__placeholder--loading">
          <span>{placeholderLabel}</span>
        </div>
      </div>

      <div className="episode-card__content">
        <div className="episode-card__status-badge episode-card__status-badge--loading" />

        <div className="episode-card__body">
          {metaLabel ? <span className="episode-card__meta">{metaLabel}</span> : null}
          <span className="episode-card__line episode-card__line--title skeleton-shimmer" />
          <span className="episode-card__line episode-card__line--title-alt skeleton-shimmer" />
          <span className="episode-card__line episode-card__line--description skeleton-shimmer" />
          <span className="episode-card__line episode-card__line--description-alt skeleton-shimmer" />
        </div>

        <div className="episode-card__progress episode-card__progress--loading">
          <span className="skeleton-shimmer" style={{ width: '42%' }} />
        </div>
      </div>
    </div>
  )
}
