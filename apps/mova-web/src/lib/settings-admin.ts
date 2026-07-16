import type { Library, LibraryDetail, ScanJob, UserAccount } from '../api/types'
import { translateCurrent } from '../i18n'

export interface ConfirmActionCopy {
  confirmLabel: string
  description: string
  title: string
}

export const getUserAvatarInitial = (username: string) =>
  username.trim().charAt(0).toUpperCase() || 'U'

export const getUserLibraryAccessSummary = (user: UserAccount, libraries: Library[]) => {
  if (user.role === 'admin') {
    return ''
  }

  const libraryNames = libraries
    .filter((library) => user.library_ids.includes(library.id))
    .map((library) => library.name)

  return libraryNames.length > 0
    ? `Access: ${libraryNames.join(', ')}`
    : 'Access: No libraries assigned'
}

export const getScanStatusLabel = (scanJob: ScanJob | null | undefined) => {
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

export const buildInitialScanJob = (
  libraryId: number,
  createdAt = new Date().toISOString(),
): ScanJob => ({
  id: -libraryId,
  library_id: libraryId,
  status: 'pending',
  phase: 'discovering',
  total_files: 0,
  scanned_files: 0,
  local_analyzed_files: 0,
  local_committed_files: 0,
  remote_completed_files: 0,
  progress_percent: 0,
  created_at: createdAt,
  started_at: null,
  finished_at: null,
  error_message: null,
})

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
    homeLibraryDetail: placeholderDetail,
  }
}

export const buildUpdatedLibraryCacheState = ({
  currentHomeLibraryDetail,
  currentLibraryDetail,
  currentLibraries,
  updatedLibrary,
}: {
  currentHomeLibraryDetail: LibraryDetail | undefined
  currentLibraryDetail: LibraryDetail | undefined
  currentLibraries: Library[] | undefined
  updatedLibrary: Library
}) => {
  return {
    libraries: upsertLibrary(currentLibraries, updatedLibrary),
    libraryDetail: mergeUpdatedLibraryDetail(currentLibraryDetail, updatedLibrary),
    homeLibraryDetail: mergeUpdatedLibraryDetail(currentHomeLibraryDetail, updatedLibrary),
  }
}

export const buildTriggeredScanCacheState = ({
  fallbackLibrary,
  currentHomeLibraryDetail,
  currentLibraryDetail,
  scanJob,
}: {
  fallbackLibrary: Library
  currentHomeLibraryDetail: LibraryDetail | undefined
  currentLibraryDetail: LibraryDetail | undefined
  scanJob: ScanJob
}) => ({
  libraryDetail: mergeTriggeredScanLibraryDetail(currentLibraryDetail, fallbackLibrary, scanJob),
  homeLibraryDetail: mergeTriggeredScanLibraryDetail(
    currentHomeLibraryDetail,
    fallbackLibrary,
    scanJob,
  ),
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
