import { describe, expect, it } from 'vitest'
import {
  libraryDetailReturnPath,
  mediaItemDetailReturnPath,
  mediaItemPlaybackReturnPath,
} from './media-routes'

describe('deterministic page return paths', () => {
  it('returns a library detail directly to home', () => {
    expect(libraryDetailReturnPath()).toBe('/')
  })

  it('returns a media item detail to its containing library', () => {
    expect(mediaItemDetailReturnPath(4)).toBe('/libraries/4')
  })
})

describe('mediaItemPlaybackReturnPath', () => {
  it('returns a movie to its own detail page', () => {
    expect(
      mediaItemPlaybackReturnPath({
        libraryId: 4,
        mediaItemId: 82,
        mediaType: 'movie',
        seasonNumber: null,
        seriesMediaItemId: null,
      }),
    ).toBe('/media-items/82')
  })

  it('returns an episode to its series detail page and current season', () => {
    expect(
      mediaItemPlaybackReturnPath({
        libraryId: 4,
        mediaItemId: 1860,
        mediaType: 'episode',
        seasonNumber: 2,
        seriesMediaItemId: 91,
      }),
    ).toBe('/media-items/91?season=2')
  })

  it('falls back to the library when an episode has no series relation', () => {
    expect(
      mediaItemPlaybackReturnPath({
        libraryId: 4,
        mediaItemId: 1860,
        mediaType: 'episode',
        seasonNumber: 2,
        seriesMediaItemId: null,
      }),
    ).toBe('/libraries/4')
  })
})
