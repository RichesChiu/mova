import type { Library, LibraryDetail, ScanJob, UserAccount } from '../api/types'
import { translateCurrent } from '../i18n'

export interface ConfirmActionCopy {
  confirmLabel: string
  description: string
  title: string
}

export const getScanStatusLabel = (
  scanJob: ScanJob | null | undefined,
  progressPercent?: number,
) => {
  const statusLabel = (() => {
    switch (scanJob?.status) {
      case 'running':
        return translateCurrent('Running')
      case 'success':
        return translateCurrent('Success')
      case 'failed':
        return translateCurrent('Failed')
      case 'cancelled':
        return translateCurrent('Cancelled')
      case 'pending':
        return translateCurrent('Pending')
      default:
        return translateCurrent('Idle')
    }
  })()

  if (
    progressPercent === undefined ||
    (scanJob?.status !== 'pending' && scanJob?.status !== 'running')
  ) {
    return statusLabel
  }

  const normalizedProgressPercent = Math.round(Math.max(0, Math.min(100, progressPercent)))
  return `${statusLabel} ${normalizedProgressPercent}%`
}

export const getScanStatusTone = (scanJob: ScanJob | null | undefined) => {
  switch (scanJob?.status) {
    case 'running':
      return 'running'
    case 'success':
      return 'success'
    case 'failed':
      return 'failed'
    case 'cancelled':
      return 'muted'
    case 'pending':
      return 'pending'
    default:
      return 'muted'
  }
}

export const buildPlaceholderLibraryDetail = (library: Library): LibraryDetail => ({
  ...library,
  media_count: 0,
  movie_count: 0,
  series_count: 0,
  last_scan: null,
})

export const upsertLibrary = (libraries: Library[] | undefined, nextLibrary: Library) => {
  if (!libraries || libraries.length === 0) {
    return [nextLibrary]
  }

  let found = false
  const nextLibraries = libraries.map((library) => {
    if (library.id !== nextLibrary.id) {
      return library
    }

    found = true
    return nextLibrary
  })

  return found ? nextLibraries : [...nextLibraries, nextLibrary]
}

export const removeLibrary = (libraries: Library[] | undefined, libraryId: number) =>
  libraries?.filter((library) => library.id !== libraryId) ?? []

export const mergeUpdatedLibraryDetail = (
  current: LibraryDetail | undefined,
  updatedLibrary: Library,
): LibraryDetail => {
  const currentLastScan = current?.last_scan ?? null

  return {
    ...(current ?? buildPlaceholderLibraryDetail(updatedLibrary)),
    ...updatedLibrary,
    last_scan: currentLastScan,
  }
}

export const mergeTriggeredScanLibraryDetail = (
  current: LibraryDetail | undefined,
  fallbackLibrary: Library,
  scanJob: ScanJob,
): LibraryDetail => ({
  ...(current ?? buildPlaceholderLibraryDetail(fallbackLibrary)),
  last_scan: scanJob,
})

export const buildCreatedLibraryCacheState = (
  libraries: Library[] | undefined,
  createdLibrary: Library,
) => {
  const placeholderDetail = buildPlaceholderLibraryDetail(createdLibrary)

  return {
    libraries: upsertLibrary(libraries, createdLibrary),
    libraryDetail: placeholderDetail,
  }
}

export const buildUpdatedLibraryCacheState = ({
  currentLibraryDetail,
  currentLibraries,
  updatedLibrary,
}: {
  currentLibraryDetail: LibraryDetail | undefined
  currentLibraries: Library[] | undefined
  updatedLibrary: Library
}) => {
  return {
    libraries: upsertLibrary(currentLibraries, updatedLibrary),
    libraryDetail: mergeUpdatedLibraryDetail(currentLibraryDetail, updatedLibrary),
  }
}

export const buildTriggeredScanCacheState = ({
  fallbackLibrary,
  currentLibraryDetail,
  scanJob,
}: {
  fallbackLibrary: Library
  currentLibraryDetail: LibraryDetail | undefined
  scanJob: ScanJob
}) => ({
  libraryDetail: mergeTriggeredScanLibraryDetail(currentLibraryDetail, fallbackLibrary, scanJob),
})

export const buildDeletedLibraryCacheState = (
  libraries: Library[] | undefined,
  libraryId: number,
) => ({
  libraries: removeLibrary(libraries, libraryId),
})

export const buildDeleteLibraryConfirmationCopy = (library: Library): ConfirmActionCopy => ({
  confirmLabel: translateCurrent('Delete Library'),
  description: translateCurrent(
    'Delete "{{name}}"? This removes the library record and scan history. Already imported media files in the filesystem will not be deleted.',
    {
      name: library.name,
    },
  ),
  title: translateCurrent('Delete library'),
})

export const upsertUserAccount = (users: UserAccount[] | undefined, nextUser: UserAccount) => {
  if (!users || users.length === 0) {
    return [nextUser]
  }

  let found = false
  const nextUsers = users.map((user) => {
    if (user.id !== nextUser.id) {
      return user
    }

    found = true
    return nextUser
  })

  return found ? nextUsers : [...nextUsers, nextUser]
}

export const removeUserAccount = (users: UserAccount[] | undefined, userId: number) =>
  users?.filter((user) => user.id !== userId) ?? []

export const buildCreatedUserCacheState = (
  users: UserAccount[] | undefined,
  createdUser: UserAccount,
) => ({
  users: upsertUserAccount(users, createdUser),
})

export const buildUpdatedUserCacheState = (
  users: UserAccount[] | undefined,
  currentUserId: number,
  updatedUser: UserAccount,
) => ({
  users: upsertUserAccount(users, updatedUser),
  currentUser: updatedUser.id === currentUserId ? updatedUser : null,
})

export const buildDeletedUserCacheState = (users: UserAccount[] | undefined, userId: number) => ({
  users: removeUserAccount(users, userId),
})

export const buildDeleteUserConfirmationCopy = (user: UserAccount): ConfirmActionCopy => ({
  confirmLabel: translateCurrent('Delete User'),
  description: translateCurrent(
    'Delete "{{name}}"? This removes their access, active sessions, and playback progress.',
    {
      name: user.username,
    },
  ),
  title: translateCurrent('Delete user'),
})
