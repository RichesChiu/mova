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

const ratingSourcePriority = (retrievedVia: string) => {
  switch (retrievedVia.trim().toLowerCase()) {
    case 'manual':
      return 0
    case 'nfo':
      return 1
    default:
      return 100
  }
}

const ratingBrandPriority = (source: string) => (source.trim().toLowerCase() === 'tmdb' ? 0 : 100)

const compareRatings = (left: MediaRating, right: MediaRating) => {
  const ownershipDifference =
    ratingSourcePriority(left.retrieved_via) - ratingSourcePriority(right.retrieved_via)
  if (ownershipDifference !== 0) {
    return ownershipDifference
  }

  const brandDifference = ratingBrandPriority(left.source) - ratingBrandPriority(right.source)
  if (brandDifference !== 0) {
    return brandDifference
  }

  const sourceDifference = left.source
    .trim()
    .toLowerCase()
    .localeCompare(right.source.trim().toLowerCase())
  return sourceDifference !== 0
    ? sourceDifference
    : left.kind.trim().toLowerCase().localeCompare(right.kind.trim().toLowerCase())
}

export const selectDisplayRatings = (ratings: MediaRating[], limit: number) => {
  const selected = new Map<string, MediaRating>()

  for (const rating of ratings.filter(isDisplayableRating)) {
    const key = `${rating.source.trim().toLowerCase()}:${rating.kind.trim().toLowerCase()}`
    const current = selected.get(key)
    if (
      !current ||
      ratingSourcePriority(rating.retrieved_via) < ratingSourcePriority(current.retrieved_via)
    ) {
      selected.set(key, rating)
    }
  }

  return [...selected.values()].sort(compareRatings).slice(0, Math.max(0, limit))
}

export const selectPrimaryRating = (ratings: MediaRating[]) =>
  selectDisplayRatings(ratings, 1)[0] ?? null
