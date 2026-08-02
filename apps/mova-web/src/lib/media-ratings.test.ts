import { describe, expect, it } from 'vitest'
import type { MediaRating } from '../api/types'
import {
  formatRatingSource,
  formatRatingValue,
  isDisplayableRating,
  selectDisplayRatings,
} from './media-ratings'

const rating = (overrides: Partial<MediaRating> = {}): MediaRating => ({
  source: 'tmdb',
  kind: 'audience',
  retrieved_via: 'tmdb',
  score: 8.4,
  scale: 10,
  rating_count: 12_345,
  attributes: {},
  fetched_at: '2026-07-26T00:00:00Z',
  ...overrides,
})

describe('media ratings', () => {
  it('keeps source-native labels and score precision', () => {
    expect(formatRatingSource('tmdb')).toBe('TMDB')
    expect(formatRatingSource('rotten_tomatoes')).toBe('Rotten Tomatoes')
    expect(formatRatingValue(8.4)).toBe('8.4')
    expect(formatRatingValue(8)).toBe('8')
  })

  it('rejects invalid scores and applies the display limit', () => {
    expect(isDisplayableRating(rating())).toBe(true)
    expect(isDisplayableRating(rating({ score: 11 }))).toBe(false)
    expect(isDisplayableRating(rating({ scale: 0 }))).toBe(false)
    expect(
      selectDisplayRatings(
        [rating(), rating({ source: 'imdb', score: 7.9 }), rating({ score: Number.NaN })],
        1,
      ),
    ).toHaveLength(1)
  })

  it('deduplicates the same source and kind by metadata ownership priority', () => {
    expect(
      selectDisplayRatings(
        [
          rating({ score: 7.5, retrieved_via: 'tmdb' }),
          rating({ score: 8.1, retrieved_via: 'nfo' }),
          rating({ score: 9.2, retrieved_via: 'manual' }),
          rating({ source: 'tmdb', kind: 'critic', score: 81, scale: 100 }),
        ],
        4,
      ),
    ).toEqual([
      rating({ score: 9.2, retrieved_via: 'manual' }),
      rating({ source: 'tmdb', kind: 'critic', score: 81, scale: 100 }),
    ])
  })
})
