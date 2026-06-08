import type {
  ContinueWatchingItem,
  EpisodeOutline,
  Library,
  LibraryDetail,
  MediaItem,
} from '../../api/types'
import type { LibraryScanRuntime } from '../../components/app-shell/scan-runtime'

export interface HomeLibraryModuleData {
  detail: LibraryDetail | null
  detailError: Error | null
  detailLoading: boolean
  library: Library
  recentItems: MediaItem[]
  scanRuntime: LibraryScanRuntime
}

export interface ContinueWatchingCardData {
  artworkAlt: string
  artworkSrc: string | null
  description: string | null
  href: string
  id: number
  metaLabel: string | null
  placeholderLabel: string
  progressPercent: number
  status: 'idle' | 'progress' | 'complete'
  title: string
}

export type ContinueWatchingOutlineMap = Map<number, EpisodeOutline | null>

export type { ContinueWatchingItem }
