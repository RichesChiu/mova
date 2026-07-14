import { describe, expect, it } from 'vitest'
import {
  getVisibleHomeLibraries,
  HOME_LIBRARY_LIMIT,
  shouldRenderHomeContinueWatching,
  shouldShowAllHomeLibraries,
} from './home-sections'

describe('home-sections', () => {
  it('keeps at most five libraries in the home row', () => {
    expect(HOME_LIBRARY_LIMIT).toBe(5)
    expect(getVisibleHomeLibraries([1, 2, 3, 4])).toEqual([1, 2, 3, 4])
    expect(getVisibleHomeLibraries([1, 2, 3, 4, 5, 6])).toEqual([1, 2, 3, 4, 5])
  })

  it('only shows the library collection link when more than five libraries exist', () => {
    expect(shouldShowAllHomeLibraries(5)).toBe(false)
    expect(shouldShowAllHomeLibraries(6)).toBe(true)
  })

  it('hides an empty completed continue-watching module', () => {
    expect(
      shouldRenderHomeContinueWatching({ hasError: false, isLoading: false, itemCount: 0 }),
    ).toBe(false)
    expect(
      shouldRenderHomeContinueWatching({ hasError: false, isLoading: true, itemCount: 0 }),
    ).toBe(true)
    expect(
      shouldRenderHomeContinueWatching({ hasError: true, isLoading: false, itemCount: 0 }),
    ).toBe(true)
    expect(
      shouldRenderHomeContinueWatching({ hasError: false, isLoading: false, itemCount: 1 }),
    ).toBe(true)
  })
})
