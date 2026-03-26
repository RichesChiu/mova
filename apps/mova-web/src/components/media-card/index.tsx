import { Link } from 'react-router-dom'
import type { MediaItem } from '../../api/types'
import { mediaItemPrimaryPath } from '../../lib/media-routes'

interface MediaCardProps {
  item: MediaItem
}

export const MediaCard = ({ item }: MediaCardProps) => {
  const subtitle = item.overview ?? item.original_title ?? 'No summary yet'

  return (
    <Link className="media-card" to={mediaItemPrimaryPath(item)}>
      <div className="media-card__poster">
        {item.poster_path ? (
          <img alt={`${item.title} poster`} loading="lazy" src={item.poster_path} />
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
        <h3>{item.title}</h3>
        <p className="muted clamp-3">{subtitle}</p>
      </div>
    </Link>
  )
}
