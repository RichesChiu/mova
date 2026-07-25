import type { MediaRating } from '../api/types'

const RATING_SOURCE_LABELS: Record<string, string> = {
  imdb: 'IMDb',
  rotten_tomatoes: 'Rotten Tomatoes',
  tmdb: 'TMDB',
}

export const formatRatingSource = (source: string) => {
  const normalizedSource = source.trim().toLowerCase()

  return (
    RATING_SOURCE_LABELS[normalizedSource] ??
    normalizedSource
      .split('_')
      .filter(Boolean)
      .map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
      .join(' ')
  )
}

export const formatRatingValue = (value: number) =>
  Number.isInteger(value) ? String(value) : value.toFixed(1)

export const isDisplayableRating = (rating: MediaRating) =>
  Number.isFinite(rating.score) &&
  Number.isFinite(rating.scale) &&
  rating.scale > 0 &&
  rating.score >= 0 &&
  rating.score <= rating.scale

export const selectDisplayRatings = (ratings: MediaRating[], limit: number) =>
  ratings.filter(isDisplayableRating).slice(0, Math.max(0, limit))
