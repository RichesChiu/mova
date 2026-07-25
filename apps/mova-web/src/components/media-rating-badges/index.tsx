import type { MediaRating } from '../../api/types'
import { useI18n } from '../../i18n'
import {
  formatRatingSource,
  formatRatingValue,
  selectDisplayRatings,
} from '../../lib/media-ratings'

interface MediaRatingBadgesProps {
  className?: string
  limit?: number
  ratings: MediaRating[]
}

export const MediaRatingBadges = ({ className, limit = 1, ratings }: MediaRatingBadgesProps) => {
  const { formatNumber, l } = useI18n()
  const displayRatings = selectDisplayRatings(ratings, limit)

  if (displayRatings.length === 0) {
    return null
  }

  const classes = ['media-rating-badges', className].filter(Boolean).join(' ')

  return (
    <span className={classes}>
      {displayRatings.map((rating) => {
        const sourceLabel = formatRatingSource(rating.source)
        const scoreLabel = formatRatingValue(rating.score)
        const scaleLabel = formatRatingValue(rating.scale)
        const title =
          typeof rating.rating_count === 'number' && rating.rating_count > 0
            ? l('{{source}} rating {{value}} out of {{scale}} from {{count}} votes', {
                source: sourceLabel,
                value: scoreLabel,
                scale: scaleLabel,
                count: formatNumber(rating.rating_count),
              })
            : l('{{source}} rating {{value}} out of {{scale}}', {
                source: sourceLabel,
                value: scoreLabel,
                scale: scaleLabel,
              })

        return (
          <span
            className="media-rating-badge"
            key={`${rating.source}:${rating.kind}`}
            title={title}
          >
            <span className="media-rating-badge__source">{sourceLabel}</span>
            <strong>{scoreLabel}</strong>
          </span>
        )
      })}
    </span>
  )
}
