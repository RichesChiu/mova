import type { MediaType } from '../api/types'

type MediaTypeLabelKey = 'Episode' | 'Movie' | 'Other' | 'Series'
type MediaTypeTranslator = (message: MediaTypeLabelKey) => string

const normalizeMediaType = (mediaType: MediaType) => mediaType.trim().toLowerCase()

export const getMediaTypeLabelKey = (mediaType: MediaType): MediaTypeLabelKey => {
  switch (normalizeMediaType(mediaType)) {
    case 'movie':
      return 'Movie'
    case 'series':
      return 'Series'
    case 'episode':
      return 'Episode'
    default:
      return 'Other'
  }
}

export const getLibraryMediaTypeLabelKey = (mediaType: MediaType): MediaTypeLabelKey => {
  const labelKey = getMediaTypeLabelKey(mediaType)

  return labelKey === 'Episode' ? 'Series' : labelKey
}

export const formatMediaTypeLabel = (mediaType: MediaType, translate: MediaTypeTranslator) =>
  translate(getMediaTypeLabelKey(mediaType))

export const formatLibraryMediaTypeLabel = (mediaType: MediaType, translate: MediaTypeTranslator) =>
  translate(getLibraryMediaTypeLabelKey(mediaType))
