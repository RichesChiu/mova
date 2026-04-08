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
  description: '家庭电影库',
  library_type: 'movie',
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
  role: 'viewer',
  is_enabled: true,
  library_ids: [7],
  created_at: '2026-04-08T00:00:00Z',
  updated_at: '2026-04-08T00:00:00Z',
}

describe('settings admin helpers', () => {
  it('builds a placeholder detail and empty shelf for a new enabled library', () => {
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
      last_scan: expect.objectContaining({
        status: 'pending',
      }),
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

  it('merges library detail updates and seeds pending scan when re-enabled', () => {
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
      mergeUpdatedLibraryDetail(
        currentDetail,
        {
          ...library,
          is_enabled: true,
        },
        true,
      ),
    ).toMatchObject({
      is_enabled: true,
      last_scan: {
        status: 'pending',
      },
    })

    expect(mergeTriggeredScanLibraryDetail(undefined, library, scanJob)).toMatchObject({
      id: 7,
      last_scan: scanJob,
    })
    expect(
      buildUpdatedLibraryCacheState({
        currentLibraries: [currentDetail],
        previousLibrary: currentDetail,
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
        last_scan: {
          status: 'pending',
        },
      },
      homeLibraryDetail: {
        last_scan: {
          status: 'pending',
        },
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
        previousLibrary: {
          ...library,
          is_enabled: true,
        },
        updatedLibrary: {
          ...library,
          description: '更新后的说明',
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
        description: '更新后的说明',
        last_scan: scanJob,
      },
      homeLibraryDetail: {
        description: '更新后的说明',
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
    ).toBe('Access: All libraries')

    expect(getScanStatusLabel(scanJob)).toBe('Running')
    expect(getScanStatusTone(scanJob)).toBe('running')
    expect(getScanStatusSummary(scanJob)).toBe('已扫描 6/20 个文件。')
    expect(getScanStatusSummary(null)).toBe('还没有执行过扫描。')
    expect(
      getScanStatusSummary({
        ...scanJob,
        status: 'failed',
        error_message: '扫描目录阶段失败：目录不存在',
      }),
    ).toBe('扫描目录阶段失败：目录不存在')
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
