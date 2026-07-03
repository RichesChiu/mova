import { afterEach, describe, expect, it } from 'vitest'
import type { Library, RecentlyAddedLibraryMediaItems } from './types'
import { requestMockJson } from './mock-api'
import { isMockApiEnabled } from './mock-control'

const setLocation = (path: string) => {
  window.history.replaceState({}, '', path)
}

afterEach(() => {
  window.localStorage.clear()
  setLocation('/')
})

describe('mock api switch', () => {
  it('is disabled by default', async () => {
    expect(isMockApiEnabled()).toBe(false)
    await expect(requestMockJson<Library[]>('/api/libraries')).resolves.toBeNull()
  })

  it('enables from the url and persists the choice', async () => {
    setLocation('/?mova_mock_api=1')

    const libraries = await requestMockJson<Library[]>('/api/libraries')

    expect(isMockApiEnabled()).toBe(true)
    expect(libraries?.data.map((library) => library.name)).toContain('Overseas TV')

    setLocation('/')
    expect(isMockApiEnabled()).toBe(true)
  })

  it('disables from the url and persists the choice', () => {
    window.localStorage.setItem('mova:mock-api', '1')
    setLocation('/?mova_mock_api=0')

    expect(isMockApiEnabled()).toBe(false)

    setLocation('/')
    expect(isMockApiEnabled()).toBe(false)
  })

  it('returns limited recently added groups by library', async () => {
    setLocation('/?mova_mock_api=1')

    const response = await requestMockJson<RecentlyAddedLibraryMediaItems[]>(
      '/api/libraries/recently-added?library_limit=2&limit=3',
    )

    expect(response?.data).toHaveLength(2)
    expect(response?.data[0]?.items).toHaveLength(3)
    expect(response?.data[0]?.library.name).toBe('Overseas TV')
  })

  it('filters recently added mock data by days', async () => {
    setLocation('/?mova_mock_api=1')

    const response = await requestMockJson<RecentlyAddedLibraryMediaItems[]>(
      '/api/libraries/recently-added?days=1',
    )

    expect(response?.data.every((group) => group.items.length > 0)).toBe(true)
  })

  it('does not mock admin write routes', async () => {
    setLocation('/?mova_mock_api=1')

    await expect(
      requestMockJson<Library>('/api/libraries', {
        method: 'POST',
        body: JSON.stringify({
          is_enabled: true,
          metadata_language: 'zh-CN',
          name: 'New Library',
          root_path: '/media/new-library',
        }),
      }),
    ).resolves.toBeNull()
  })
})
