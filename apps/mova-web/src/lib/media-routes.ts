import type { MediaItem, MediaType } from '../api/types'

export function mediaItemDetailPath(mediaItemId: number) {
  return `/media-items/${mediaItemId}`
}

export function mediaItemPlayPath(
  mediaItemId: number,
  options?: {
    fromStart?: boolean
  },
) {
  if (options?.fromStart) {
    return `/media-items/${mediaItemId}/play?fromStart=1`
  }

  return `/media-items/${mediaItemId}/play`
}

export function mediaItemPrimaryPath(item: Pick<MediaItem, 'id' | 'media_type'>) {
  return mediaTypePrimaryPath(item.id, item.media_type)
}

export function mediaTypePrimaryPath(mediaItemId: number, _mediaType: MediaType) {
  return mediaItemDetailPath(mediaItemId)
}
