import { describe, expect, it } from 'vitest'
import {
  formatLibraryMediaTypeLabel,
  formatMediaTypeLabel,
  getLibraryMediaTypeLabelKey,
  getMediaTypeLabelKey,
} from './media-type-label'

const translate = (message: 'Episode' | 'Movie' | 'Other' | 'Series') =>
  ({
    Episode: '单集',
    Movie: '电影',
    Other: '其他',
    Series: '剧集',
  })[message]

describe('media type labels', () => {
  it('normalizes raw media type values into stable label keys', () => {
    expect(getMediaTypeLabelKey('movie')).toBe('Movie')
    expect(getMediaTypeLabelKey('series')).toBe('Series')
    expect(getMediaTypeLabelKey('episode')).toBe('Episode')
    expect(getMediaTypeLabelKey('unknown')).toBe('Other')
  })

  it('treats episodes as series in library card contexts', () => {
    expect(getLibraryMediaTypeLabelKey('episode')).toBe('Series')
    expect(formatLibraryMediaTypeLabel('episode', translate)).toBe('剧集')
  })

  it('formats labels through the provided translator', () => {
    expect(formatMediaTypeLabel('movie', translate)).toBe('电影')
    expect(formatMediaTypeLabel('series', translate)).toBe('剧集')
    expect(formatMediaTypeLabel('episode', translate)).toBe('单集')
    expect(formatMediaTypeLabel('something-new', translate)).toBe('其他')
  })
})
