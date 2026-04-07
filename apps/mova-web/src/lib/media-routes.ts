import type { MediaItem, MediaType } from '../api/types'

export const mediaItemDetailPath = (mediaItemId: number) => `/media-items/${mediaItemId}`

export const mediaItemPlayPath = (
  mediaItemId: number,
  options?: {
    fromStart?: boolean
  },
) => {
  if (options?.fromStart) {
    return `/media-items/${mediaItemId}/play?fromStart=1`
  }

  return `/media-items/${mediaItemId}/play`
}

export const mediaItemPrimaryPath = (item: Pick<MediaItem, 'id' | 'media_type'>) => {
  return mediaTypePrimaryPath(item.id, item.media_type)
}

export const mediaTypePrimaryPath = (mediaItemId: number, _mediaType: MediaType) => {
  return mediaItemDetailPath(mediaItemId)
}
