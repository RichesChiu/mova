import { describe, expect, it } from 'vitest'
import {
  getVisibleHomeLibraries,
  HOME_LIBRARY_LIMIT,
  shouldRenderHomeContinueWatching,
  shouldRenderHomeRecentlyAdded,
  shouldShowAllHomeLibraries,
} from './home-sections'

describe('home-sections', () => {
  it('keeps at most four libraries in the home row', () => {
    expect(HOME_LIBRARY_LIMIT).toBe(4)
    expect(getVisibleHomeLibraries([1, 2, 3, 4])).toEqual([1, 2, 3, 4])
    expect(getVisibleHomeLibraries([1, 2, 3, 4, 5])).toEqual([1, 2, 3, 4])
  })

  it('only shows the library collection link when more than four libraries exist', () => {
    expect(shouldShowAllHomeLibraries(4)).toBe(false)
    expect(shouldShowAllHomeLibraries(5)).toBe(true)
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

  it('hides recently added only after an empty library snapshot completes', () => {
    expect(
      shouldRenderHomeRecentlyAdded({ hasError: false, isLoading: false, libraryCount: 0 }),
    ).toBe(false)
    expect(
      shouldRenderHomeRecentlyAdded({ hasError: false, isLoading: true, libraryCount: 0 }),
    ).toBe(true)
    expect(
      shouldRenderHomeRecentlyAdded({ hasError: true, isLoading: false, libraryCount: 0 }),
    ).toBe(true)
    expect(
      shouldRenderHomeRecentlyAdded({ hasError: false, isLoading: false, libraryCount: 1 }),
    ).toBe(true)
  })
})
