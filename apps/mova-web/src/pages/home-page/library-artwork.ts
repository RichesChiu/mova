import type { MediaItem } from '../../api/types'

export const getLibraryArtworkSrc = (items: MediaItem[]) =>
  items.find((item) => item.backdrop_path)?.backdrop_path ?? null
