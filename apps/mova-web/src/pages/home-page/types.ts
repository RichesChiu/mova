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

export type ContinueWatchingOutlineMap = Map<number, EpisodeOutline | null>

export type { ContinueWatchingItem }
