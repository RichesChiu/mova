import { describe, expect, it } from 'vitest'
import { formatPlaybackTime } from './format'

describe('formatPlaybackTime', () => {
  it('uses a stable minutes and seconds format for playback under one hour', () => {
    expect(formatPlaybackTime(0)).toBe('00:00')
    expect(formatPlaybackTime(5)).toBe('00:05')
    expect(formatPlaybackTime(775)).toBe('12:55')
  })

  it('adds hours only when playback reaches one hour', () => {
    expect(formatPlaybackTime(3_600)).toBe('1:00:00')
    expect(formatPlaybackTime(7_445)).toBe('2:04:05')
  })

  it('uses a neutral placeholder when playback time is unavailable', () => {
    expect(formatPlaybackTime(null)).toBe('--:--')
    expect(formatPlaybackTime(Number.NaN)).toBe('--:--')
    expect(formatPlaybackTime(-1)).toBe('--:--')
  })
})
