import type { MediaItem, MediaType } from '../api/types'

export const mediaItemDetailPath = (mediaItemId: number) => `/media-items/${mediaItemId}`

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
