import { Link } from 'react-router-dom'

export interface ContinueWatchingCardData {
  artworkAlt: string
  artworkSrc: string | null
  href: string
  id: number
  metaLabel: string | null
  placeholderLabel: string
  progressPercent: number
  title: string
}

const PlayIcon = () => (
  <svg
    aria-hidden="true"
    fill="none"
    focusable="false"
    stroke="currentColor"
    strokeLinecap="round"
    strokeLinejoin="round"
    strokeWidth="1.8"
    viewBox="0 0 24 24"
  >
    <path d="M9 7.5v9l7-4.5-7-4.5Z" fill="currentColor" stroke="none" />
  </svg>
)

export const ContinueWatchingCard = ({ item }: { item: ContinueWatchingCardData }) => {
  const progressPercent = Math.max(0, Math.min(100, item.progressPercent))

  return (
    <Link className="continue-watching-card" to={item.href}>
      <div className="continue-watching-card__artwork">
        {item.artworkSrc ? (
          <img alt={item.artworkAlt} loading="lazy" src={item.artworkSrc} />
        ) : (
          <span className="continue-watching-card__placeholder">{item.placeholderLabel}</span>
        )}
        <span aria-hidden="true" className="continue-watching-card__play">
          <PlayIcon />
        </span>
        <div aria-hidden="true" className="continue-watching-card__progress">
          <span style={{ width: `${progressPercent}%` }} />
        </div>
      </div>
      <div className="continue-watching-card__copy">
        <strong title={item.title}>{item.title}</strong>
        {item.metaLabel ? <span>{item.metaLabel}</span> : null}
        <em>{progressPercent}%</em>
      </div>
    </Link>
  )
}

export const ContinueWatchingCardSkeleton = ({ label }: { label: string }) => (
  <div aria-hidden="true" className="continue-watching-card continue-watching-card--loading">
    <div className="continue-watching-card__artwork skeleton-shimmer">
      <span className="continue-watching-card__placeholder">{label}</span>
    </div>
    <div className="continue-watching-card__copy">
      <span className="continue-watching-card__line continue-watching-card__line--title skeleton-shimmer" />
      <span className="continue-watching-card__line continue-watching-card__line--meta skeleton-shimmer" />
    </div>
  </div>
)
