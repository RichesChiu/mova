import { describe, expect, it } from 'vitest'
import {
  getVisibleHomeContinueWatching,
  getVisibleHomeLibraries,
  HOME_CONTINUE_WATCHING_LIMIT,
  HOME_LIBRARY_LIMIT,
  shouldRenderHomeContinueWatching,
  shouldRenderHomeRecentlyAdded,
  shouldShowAllHomeContinueWatching,
  shouldShowAllHomeLibraries,
} from './home-sections'

describe('home-sections', () => {
  it('keeps at most six continue-watching items in the home preview', () => {
    expect(HOME_CONTINUE_WATCHING_LIMIT).toBe(6)
    expect(getVisibleHomeContinueWatching([1, 2, 3, 4, 5, 6, 7])).toEqual([1, 2, 3, 4, 5, 6])
  })

  it('keeps at most four libraries in the home row', () => {
    expect(HOME_LIBRARY_LIMIT).toBe(4)
    expect(getVisibleHomeLibraries([1, 2, 3, 4])).toEqual([1, 2, 3, 4])
    expect(getVisibleHomeLibraries([1, 2, 3, 4, 5])).toEqual([1, 2, 3, 4])
  })

  it('only shows the library collection link when more than four libraries exist', () => {
    expect(shouldShowAllHomeLibraries(4)).toBe(false)
    expect(shouldShowAllHomeLibraries(5)).toBe(true)
  })

  it('only shows the continue-watching collection link when items exceed the home limit', () => {
    expect(shouldShowAllHomeContinueWatching(6)).toBe(false)
    expect(shouldShowAllHomeContinueWatching(7)).toBe(true)
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
