import type { MediaItem } from '../api/types'

export type LibraryMediaSection = 'movies' | 'series' | 'other'

type MediaSectionInput = Pick<MediaItem, 'media_type' | 'metadata_provider_item_id'>

type ScanSectionInput = {
  media_type: string
}

export const hasMatchedMetadata = (item: MediaSectionInput) =>
  typeof item.metadata_provider_item_id === 'number' && Number.isFinite(item.metadata_provider_item_id)

export const getLibraryMediaSection = (item: MediaSectionInput): LibraryMediaSection => {
  if (!hasMatchedMetadata(item)) {
    return 'other'
  }

  if (item.media_type === 'movie') {
    return 'movies'
  }

  if (item.media_type === 'series') {
    return 'series'
  }

  return 'other'
}

export const getLibraryScanSection = (item: ScanSectionInput): LibraryMediaSection => {
  if (item.media_type === 'movie') {
    return 'movies'
  }

  if (item.media_type === 'series') {
    return 'series'
  }

  return 'other'
}
