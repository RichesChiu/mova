import type { MediaItem } from '../api/types'

export type LibraryMediaSection = 'movies' | 'series' | 'other'
export type LibraryScanSection = LibraryMediaSection | null

type EnrichmentSignalInput = {
  backdrop_path?: string | null
  metadata_provider_item_id?: number | null
  original_title?: string | null
  overview?: string | null
  poster_path?: string | null
}
type MediaSectionInput = Pick<MediaItem, 'media_type' | 'metadata_status'> & EnrichmentSignalInput

type ScanSectionInput = {
  backdrop_path?: string | null
  media_type: string
  metadata_status?: string | null
  overview?: string | null
  poster_path?: string | null
}

const hasText = (value: string | null | undefined) => Boolean(value?.trim())

const hasRemoteEnrichment = (item: EnrichmentSignalInput) =>
  item.metadata_provider_item_id !== null && item.metadata_provider_item_id !== undefined
    ? true
    : hasText(item.original_title) ||
      hasText(item.overview) ||
      hasText(item.poster_path) ||
      hasText(item.backdrop_path)

const hasReviewStatus = (item: { metadata_status?: string | null }) =>
  item.metadata_status === 'skipped' ||
  item.metadata_status === 'unmatched' ||
  item.metadata_status === 'failed'

const needsReview = (item: MediaSectionInput) =>
  hasReviewStatus(item) && !hasRemoteEnrichment(item)

export const getLibraryMediaSection = (item: MediaSectionInput): LibraryMediaSection => {
  if (needsReview(item)) {
    return 'other'
  }

  if (item.media_type === 'series' || item.media_type === 'episode') {
    return 'series'
  }

  if (item.media_type === 'movie') {
    return 'movies'
  }

  return 'other'
}

export const getLibraryScanSection = (item: ScanSectionInput): LibraryScanSection => {
  if (hasReviewStatus(item) && !hasRemoteEnrichment(item)) {
    return 'other'
  }

  if (item.media_type === 'movie') {
    return 'movies'
  }

  if (item.media_type === 'series' || item.media_type === 'episode') {
    return 'series'
  }

  return null
}
