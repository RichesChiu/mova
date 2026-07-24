import { describe, expect, it } from 'vitest'
import {
  getRealtimeResourceQueryKeys,
  getRealtimeResourcesQueryKeys,
  parseLibraryRealtimeResource,
} from './realtime-resources'

describe('realtime resource query mapping', () => {
  it('parses scoped library resources', () => {
    expect(parseLibraryRealtimeResource('library:7:catalog')).toEqual({ id: 7, kind: 'catalog' })
    expect(parseLibraryRealtimeResource('library:7:notifications')).toEqual({
      id: 7,
      kind: 'notifications',
    })
    expect(parseLibraryRealtimeResource('admin:libraries')).toBeNull()
  })

  it('covers every catalog read model used by the Web client', () => {
    const keys = getRealtimeResourceQueryKeys('library:7:catalog')

    expect(keys).toEqual(
      expect.arrayContaining([
        ['library', 7],
        ['library-media', 7],
        ['libraries-page-detail', 7],
        ['libraries-page-recently-added'],
        ['global-search-page'],
        ['media-item'],
        ['media-item-cast'],
        ['media-item-files'],
        ['media-item-playback-header'],
        ['media-episode-outline'],
        ['media-file-audio-tracks'],
        ['media-file-subtitles'],
        ['home'],
      ]),
    )
  })

  it('deduplicates shared query keys across a server batch', () => {
    const keys = getRealtimeResourcesQueryKeys([
      'admin:notifications',
      'library:7:catalog',
      'library:7:scan',
      'library:7:notifications',
      'user:3:continue-watching',
    ])

    expect(keys.filter((queryKey) => JSON.stringify(queryKey) === '["home"]')).toHaveLength(1)
    expect(keys.filter((queryKey) => JSON.stringify(queryKey) === '["library",7]')).toHaveLength(1)
    expect(keys).toContainEqual(['notifications'])
  })

  it('maps administrator notification revisions to the notification feed', () => {
    expect(getRealtimeResourceQueryKeys('admin:notifications')).toEqual([['notifications']])
  })
})
