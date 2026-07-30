import { describe, expect, it } from 'vitest'
import { shouldLoadShellLibraries } from './shell-library-policy'

describe('shell library query policy', () => {
  it('uses /home as the only library source on the home route', () => {
    expect(shouldLoadShellLibraries('/')).toBe(false)
  })

  it.each([
    '/libraries',
    '/libraries/7',
    '/settings',
    '/continue',
  ])('loads the shared library list for %s', (pathname) => {
    expect(shouldLoadShellLibraries(pathname)).toBe(true)
  })
})
