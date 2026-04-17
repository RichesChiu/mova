import { describe, expect, it, vi } from 'vitest'
import type { Library, LibraryDetail, ScanJob, UserAccount } from '../api/types'
import {
  buildCreatedLibraryCacheState,
  buildCreatedUserCacheState,
  buildDeletedLibraryCacheState,
  buildDeletedUserCacheState,
  buildDeleteLibraryConfirmationCopy,
  buildDeleteUserConfirmationCopy,
  buildInitialScanJob,
  buildPlaceholderLibraryDetail,
  buildTriggeredScanCacheState,
  buildUpdatedLibraryCacheState,
  buildUpdatedUserCacheState,
  createEmptyLibraryShelf,
  getScanStatusLabel,
  getScanStatusSummary,
  getScanStatusTone,
  getUserAvatarInitial,
  getUserLibraryAccessSummary,
  mergeTriggeredScanLibraryDetail,
  mergeUpdatedLibraryDetail,
  removeLibrary,
  removeUserAccount,
  upsertLibrary,
  upsertUserAccount,
} from './settings-admin'

const library: Library = {
  id: 7,
  name: 'Movies',
  description: 'Family movie library',
  metadata_language: 'zh-CN',
  root_path: '/media/movies',
  is_enabled: true,
  created_at: '2026-04-08T00:00:00Z',
  updated_at: '2026-04-08T00:00:00Z',
}

const scanJob: ScanJob = {
  id: 41,
  library_id: 7,
  status: 'running',
  phase: 'discovering',
  total_files: 20,
  scanned_files: 6,
  created_at: '2026-04-08T00:00:00Z',
  started_at: '2026-04-08T00:00:05Z',
  finished_at: null,
  error_message: null,
}

const viewer: UserAccount = {
  id: 11,
  username: 'viewer01',
  nickname: 'viewer01',
  role: 'viewer',
  is_primary_admin: false,
  is_enabled: true,
  library_ids: [7],
  created_at: '2026-04-08T00:00:00Z',
  updated_at: '2026-04-08T00:00:00Z',
}

describe('settings admin helpers', () => {
  it('builds a placeholder detail and empty shelf for a new library', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-04-08T10:00:00Z'))

    expect(buildInitialScanJob(7)).toMatchObject({
      id: -7,
      library_id: 7,
      status: 'pending',
      phase: 'discovering',
    })
    expect(buildPlaceholderLibraryDetail(library)).toMatchObject({
      id: 7,
      media_count: 0,
      movie_count: 0,
      series_count: 0,
      last_scan: null,
    })
    expect(createEmptyLibraryShelf()).toEqual({
      items: [],
      total: 0,
      page: 1,
      page_size: 20,
    })
    expect(buildCreatedLibraryCacheState([], library)).toMatchObject({
      libraries: [library],
      libraryDetail: {
        id: 7,
      },
      homeLibraryShelf: {
        items: [],
      },
    })

    vi.useRealTimers()
  })

  it('upserts and removes libraries and users in cached collections', () => {
    expect(getUserAvatarInitial(' viewer01 ')).toBe('V')
    expect(getUserAvatarInitial('')).toBe('U')

    expect(
      upsertLibrary([library], {
        ...library,
        name: 'Cinema',
      }),
    ).toEqual([
      {
        ...library,
        name: 'Cinema',
      },
    ])
    expect(removeLibrary([library], library.id)).toEqual([])

    expect(
      upsertUserAccount([viewer], {
        ...viewer,
        is_enabled: false,
      }),
    ).toEqual([
      {
        ...viewer,
        is_enabled: false,
      },
    ])
    expect(removeUserAccount([viewer], viewer.id)).toEqual([])
    expect(buildCreatedUserCacheState([], viewer)).toEqual({
      users: [viewer],
    })
    expect(buildDeletedUserCacheState([viewer], viewer.id)).toEqual({
      users: [],
    })
    expect(buildDeletedLibraryCacheState([library], library.id)).toEqual({
      libraries: [],
    })
    expect(buildDeleteLibraryConfirmationCopy(library)).toEqual({
      confirmLabel: 'Delete Library',
      description:
        'Delete "Movies"? This removes the library record and scan history. Already imported media files in the filesystem will not be deleted.',
      title: 'Delete library',
    })
    expect(buildDeleteUserConfirmationCopy(viewer)).toEqual({
      confirmLabel: 'Delete User',
      description:
        'Delete "viewer01"? This removes their access, active sessions, playback progress, and watch history records.',
      title: 'Delete user',
    })
  })

  it('merges library detail updates while preserving the latest scan snapshot', () => {
    const currentDetail: LibraryDetail = {
      ...library,
      is_enabled: false,
      media_count: 14,
      movie_count: 14,
      series_count: 0,
      last_scan: {
        ...scanJob,
        status: 'success',
        phase: 'finished',
        scanned_files: 20,
        finished_at: '2026-04-08T10:10:00Z',
      },
    }

    expect(
      mergeUpdatedLibraryDetail(currentDetail, { ...library, is_enabled: true }),
    ).toMatchObject({
      is_enabled: true,
      last_scan: currentDetail.last_scan,
    })

    expect(mergeTriggeredScanLibraryDetail(undefined, library, scanJob)).toMatchObject({
      id: 7,
      last_scan: scanJob,
    })
    expect(
      buildUpdatedLibraryCacheState({
        currentLibraries: [currentDetail],
        updatedLibrary: {
          ...library,
          is_enabled: true,
        },
        currentLibraryDetail: currentDetail,
        currentHomeLibraryDetail: currentDetail,
      }),
    ).toMatchObject({
      libraries: [
        expect.objectContaining({
          is_enabled: true,
        }),
      ],
      libraryDetail: {
        last_scan: currentDetail.last_scan,
      },
      homeLibraryDetail: {
        last_scan: currentDetail.last_scan,
      },
    })
    expect(
      buildTriggeredScanCacheState({
        fallbackLibrary: library,
        currentLibraryDetail: undefined,
        currentHomeLibraryDetail: undefined,
        scanJob,
      }),
    ).toMatchObject({
      libraryDetail: {
        last_scan: scanJob,
      },
      homeLibraryDetail: {
        last_scan: scanJob,
      },
    })

    expect(
      buildUpdatedLibraryCacheState({
        currentLibraries: [
          {
            ...library,
            is_enabled: true,
          },
        ],
        updatedLibrary: {
          ...library,
          description: 'Updated description',
          is_enabled: true,
        },
        currentLibraryDetail: {
          ...currentDetail,
          is_enabled: true,
          last_scan: scanJob,
        },
        currentHomeLibraryDetail: {
          ...currentDetail,
          is_enabled: true,
          last_scan: scanJob,
        },
      }),
    ).toMatchObject({
      libraryDetail: {
        description: 'Updated description',
        last_scan: scanJob,
      },
      homeLibraryDetail: {
        description: 'Updated description',
        last_scan: scanJob,
      },
    })
  })

  it('formats scan and access summaries for the settings cards', () => {
    expect(getUserLibraryAccessSummary(viewer, [library])).toBe('Access: Movies')
    expect(
      getUserLibraryAccessSummary(
        {
          ...viewer,
          role: 'admin',
          library_ids: [],
        },
        [library],
      ),
    ).toBe('')

    expect(getScanStatusLabel(scanJob)).toBe('Running')
    expect(getScanStatusTone(scanJob)).toBe('running')
    expect(getScanStatusSummary(scanJob)).toBe('Scanned 6/20 files.')
    expect(getScanStatusSummary(null)).toBe('No scan has run yet.')
    expect(
      getScanStatusSummary({
        ...scanJob,
        status: 'failed',
        error_message: 'Directory scan failed: Directory does not exist',
      }),
    ).toBe('Directory scan failed: Directory does not exist')
    expect(
      buildUpdatedUserCacheState([viewer], viewer.id, { ...viewer, is_enabled: false }),
    ).toEqual({
      users: [{ ...viewer, is_enabled: false }],
      currentUser: { ...viewer, is_enabled: false },
    })
    expect(
      buildUpdatedUserCacheState([viewer], 99, {
        ...viewer,
        username: 'viewer02',
      }),
    ).toEqual({
      users: [{ ...viewer, username: 'viewer02' }],
      currentUser: null,
    })
  })
})
