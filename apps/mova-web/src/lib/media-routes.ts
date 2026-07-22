import type { MediaItem, MediaType } from '../api/types'

export const mediaItemDetailPath = (mediaItemId: number) => `/media-items/${mediaItemId}`

export const mediaItemPlaybackReturnPath = ({
  libraryId,
  mediaItemId,
  mediaType,
  seasonNumber,
  seriesMediaItemId,
}: {
  libraryId: number
  mediaItemId: number
  mediaType: MediaType
  seasonNumber: number | null
  seriesMediaItemId: number | null
}) => {
  if (mediaType !== 'episode') {
    return mediaItemDetailPath(mediaItemId)
  }

  if (seriesMediaItemId === null) {
    return `/libraries/${libraryId}`
  }

  const detailPath = mediaItemDetailPath(seriesMediaItemId)
  return typeof seasonNumber === 'number' && Number.isFinite(seasonNumber)
    ? `${detailPath}?season=${seasonNumber}`
    : detailPath
}

export const mediaItemPlayPath = (
  mediaItemId: number,
  options?: {
    fileId?: number | null
    fromStart?: boolean
  },
) => {
  const searchParams = new URLSearchParams()

  if (options?.fromStart) {
    searchParams.set('fromStart', '1')
  }

  if (typeof options?.fileId === 'number' && Number.isFinite(options.fileId)) {
    searchParams.set('file', String(options.fileId))
  }

  const queryString = searchParams.toString()
  return queryString
    ? `/media-items/${mediaItemId}/play?${queryString}`
    : `/media-items/${mediaItemId}/play`
}

export const mediaItemPrimaryPath = (item: Pick<MediaItem, 'id' | 'media_type'>) => {
  return mediaTypePrimaryPath(item.id, item.media_type)
}

export const mediaTypePrimaryPath = (mediaItemId: number, _mediaType: MediaType) => {
  return mediaItemDetailPath(mediaItemId)
}
