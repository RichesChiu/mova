import type { QueryKey } from '@tanstack/react-query'

export const REALTIME_PROTOCOL_VERSION = 1

export interface LibraryRealtimeResource {
  id: number
  kind: 'settings' | 'catalog' | 'scan'
}

export const parseLibraryRealtimeResource = (resource: string): LibraryRealtimeResource | null => {
  const match = /^library:(\d+):(settings|catalog|scan)$/.exec(resource)
  if (!match) {
    return null
  }

  return {
    id: Number(match[1]),
    kind: match[2] as LibraryRealtimeResource['kind'],
  }
}

export const getRealtimeResourceQueryKeys = (resource: string): QueryKey[] => {
  const library = parseLibraryRealtimeResource(resource)

  if (resource.endsWith(':libraries')) {
    return [['libraries'], ['libraries-page-recently-added'], ['global-search-page'], ['home']]
  }

  if (library?.kind === 'settings') {
    return [
      ['libraries'],
      ['library', library.id],
      ['libraries-page-detail', library.id],
      ['home-library-detail', library.id],
      ['home'],
    ]
  }

  if (library?.kind === 'catalog') {
    return [
      ['library', library.id],
      ['library-media', library.id],
      ['libraries-page-detail', library.id],
      ['libraries-page-recently-added'],
      ['recently-added-by-library'],
      ['home-library-detail', library.id],
      ['global-search-page'],
      ['media-item'],
      ['media-item-cast'],
      ['media-item-files'],
      ['media-item-playback-header'],
      ['media-episode-outline'],
      ['media-file-audio-tracks'],
      ['media-file-subtitles'],
      ['home'],
    ]
  }

  if (library?.kind === 'scan') {
    return [
      ['library', library.id],
      ['libraries-page-detail', library.id],
      ['home-library-detail', library.id],
      ['home'],
    ]
  }

  if (resource.endsWith(':continue-watching')) {
    return [['continue-watching'], ['home']]
  }

  if (resource.endsWith(':profile')) {
    return [['current-user'], ['home']]
  }

  if (resource === 'admin:users') {
    return [['users']]
  }

  return [['home']]
}

export const getRealtimeResourcesQueryKeys = (resources: string[]): QueryKey[] => {
  const uniqueKeys = new Map<string, QueryKey>()

  for (const resource of resources) {
    for (const queryKey of getRealtimeResourceQueryKeys(resource)) {
      uniqueKeys.set(JSON.stringify(queryKey), queryKey)
    }
  }

  return [...uniqueKeys.values()]
}
