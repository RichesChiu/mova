import type { MediaItem } from '../../api/types'

export const getLibraryArtworkSrc = (items: MediaItem[]) =>
  items.find((item) => item.backdrop_path)?.backdrop_path ??
  items.find((item) => item.poster_path)?.poster_path ??
  null
