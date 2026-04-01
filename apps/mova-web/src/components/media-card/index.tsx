import { Link } from 'react-router-dom'
import type { MediaItem } from '../../api/types'
import { mediaItemPrimaryPath } from '../../lib/media-routes'

interface MediaCardProps {
  item: MediaItem
}

interface MediaCardSkeletonProps {
  placeholderLabel?: string
}

export const MediaCard = ({ item }: MediaCardProps) => {
  const title = item.title.trim() || item.source_title.trim() || 'Untitled'
  const subtitle = item.overview ?? item.original_title ?? 'No summary yet'

  return (
    <Link className="media-card" to={mediaItemPrimaryPath(item)}>
      <div className="media-card__poster">
        {item.poster_path ? (
          <img alt={`${title} poster`} loading="lazy" src={item.poster_path} />
        ) : (
          <div className="media-card__placeholder">
            <span>{item.media_type}</span>
          </div>
        )}
      </div>

      <div className="media-card__body">
        <div className="media-card__meta">
          <span className="chip">{item.media_type}</span>
          {item.year ? <span className="muted">{item.year}</span> : null}
        </div>
        <h3>{title}</h3>
        <p className="muted clamp-3">{subtitle}</p>
      </div>
    </Link>
  )
}

export const MediaCardSkeleton = ({ placeholderLabel = 'MEDIA' }: MediaCardSkeletonProps) => {
  return (
    <div aria-hidden="true" className="media-card media-card--loading">
      <div className="media-card__poster">
        <div className="media-card__placeholder media-card__placeholder--loading">
          <span>{placeholderLabel}</span>
        </div>
      </div>

      <div className="media-card__body">
        <div className="media-card__meta">
          <span className="media-card__pill skeleton-shimmer" />
          <span className="media-card__line media-card__line--meta skeleton-shimmer" />
        </div>
        <span className="media-card__line media-card__line--title skeleton-shimmer" />
        <span className="media-card__line media-card__line--body skeleton-shimmer" />
        <span className="media-card__line media-card__line--body-alt skeleton-shimmer" />
      </div>
    </div>
  )
}
