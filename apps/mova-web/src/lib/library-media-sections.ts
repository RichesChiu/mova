import type { MediaItem } from '../api/types'

export type LibraryMediaSection = 'movies' | 'series' | 'other'

type MediaSectionInput = Pick<MediaItem, 'media_type' | 'metadata_status'>

type ScanSectionInput = {
  media_type: string
}

const needsReview = (item: MediaSectionInput) =>
  item.metadata_status === 'unmatched' || item.metadata_status === 'failed'

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

export const getLibraryScanSection = (item: ScanSectionInput): LibraryMediaSection => {
  if (item.media_type === 'movie') {
    return 'movies'
  }

  if (item.media_type === 'series' || item.media_type === 'episode') {
    return 'series'
  }

  return 'other'
}
