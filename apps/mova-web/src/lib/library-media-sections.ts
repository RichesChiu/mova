import type { MediaItem } from '../api/types'

export type LibraryMediaSection = 'movies' | 'series' | 'other'
export type LibraryScanSection = LibraryMediaSection | null

type MediaSectionInput = Pick<MediaItem, 'media_type' | 'metadata_status'> & {
  remote_media_type?: string | null
}
type MediaScanMatchInput = Pick<
  MediaItem,
  | 'media_type'
  | 'metadata_status'
  | 'metadata_provider_item_id'
  | 'original_title'
  | 'overview'
  | 'poster_path'
  | 'backdrop_path'
  | 'remote_media_type'
  | 'title'
  | 'source_title'
  | 'year'
>

type ScanSectionInput = {
  backdrop_path?: string | null
  media_type: string
  metadata_status?: string | null
  overview?: string | null
  poster_path?: string | null
  remote_media_type?: string | null
  stage?: string | null
  title?: string | null
  year?: number | null
}

const normalizeLibraryMediaMatchText = (value: string | null | undefined) =>
  (value ?? '')
    .toLocaleLowerCase()
    .replace(/[\s._\-()[\]{}:/\\|'"`,!?]+/g, '')
    .trim()

const buildLibraryMediaMatchTexts = (
  item: Pick<MediaScanMatchInput, 'title' | 'source_title' | 'original_title'>,
) =>
  [...new Set([item.title, item.source_title, item.original_title])]
    .map((value) => normalizeLibraryMediaMatchText(value))
    .filter((value) => value.length > 0)

const hasReviewStatus = (item: { metadata_status?: string | null }) =>
  item.metadata_status === 'skipped' ||
  item.metadata_status === 'unmatched' ||
  item.metadata_status === 'failed'

const getLibraryMediaBucket = (mediaType: string) => {
  if (mediaType === 'movie') {
    return 'movie'
  }

  if (mediaType === 'series' || mediaType === 'episode') {
    return 'series'
  }

  return null
}

const hasUnconfirmedMediaType = (item: {
  media_type: string
  remote_media_type?: string | null
}) => {
  const localBucket = getLibraryMediaBucket(item.media_type)
  const remoteBucket = getLibraryMediaBucket(item.remote_media_type ?? '')

  return localBucket === null || remoteBucket === null || localBucket !== remoteBucket
}

const needsReview = (item: MediaSectionInput) =>
  hasReviewStatus(item) && hasUnconfirmedMediaType(item)

const hasCompatibleScanYear = (mediaItem: MediaScanMatchInput, scanItem: ScanSectionInput) =>
  mediaItem.year === null ||
  mediaItem.year === undefined ||
  scanItem.year === null ||
  scanItem.year === undefined ||
  mediaItem.year === scanItem.year

const shouldScanItemReplaceMediaItem = (
  mediaItem: MediaScanMatchInput,
  scanItem: ScanSectionInput,
) => {
  const scanSection = getLibraryScanSection(scanItem)
  if (scanSection === null) {
    return false
  }

  const scanBucket = getLibraryMediaBucket(scanItem.media_type)
  if (scanBucket === null) {
    return false
  }

  const mediaBucket = getLibraryMediaBucket(mediaItem.media_type)
  if (mediaBucket !== null && mediaBucket !== scanBucket) {
    return false
  }

  if (!hasCompatibleScanYear(mediaItem, scanItem)) {
    return false
  }

  const scanTitle = normalizeLibraryMediaMatchText(scanItem.title)
  if (scanTitle.length === 0 || !buildLibraryMediaMatchTexts(mediaItem).includes(scanTitle)) {
    return false
  }

  const mediaSection = getLibraryMediaSection(mediaItem)
  return mediaSection === scanSection || mediaSection === 'other'
}

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
  if (item.stage && item.stage !== 'completed') {
    if (item.media_type === 'movie') {
      return 'movies'
    }

    if (item.media_type === 'series' || item.media_type === 'episode') {
      return 'series'
    }

    return null
  }

  if (hasReviewStatus(item) && hasUnconfirmedMediaType(item)) {
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

export const filterLibraryMediaItemsForScanRuntime = <Item extends MediaScanMatchInput>(
  items: Item[],
  scanItems: ScanSectionInput[],
) =>
  items.filter(
    (item) => !scanItems.some((scanItem) => shouldScanItemReplaceMediaItem(item, scanItem)),
  )

export const filterCompletedScanItemsWithSavedMedia = <Item extends ScanSectionInput>(
  scanItems: Item[],
  mediaItems: MediaScanMatchInput[],
) =>
  scanItems.filter(
    (scanItem) =>
      scanItem.stage !== 'completed' ||
      !mediaItems.some((mediaItem) => shouldScanItemReplaceMediaItem(mediaItem, scanItem)),
  )
