import type { Library, LibraryStorageIssue } from '../api/types'
import { localizeApiError } from './api-error'

export const isLibraryStorageUnavailable = (library: Pick<Library, 'storage_status'>): boolean =>
  library.storage_status === 'unavailable'

/** `cifs //nas/media`, or `null` when nothing was observed. */
export const formatStorageEndpoint = (
  fsType: string | null,
  source: string | null,
): string | null => {
  const parts = [fsType, source].filter((part): part is string => Boolean(part?.trim()))
  return parts.length > 0 ? parts.join(' ') : null
}

export const describeLibraryStorageIssue = (issue: LibraryStorageIssue) => ({
  reason: localizeApiError(issue.reason_code),
  mountPoint: issue.mount_point,
  expected: formatStorageEndpoint(issue.expected_fs_type, issue.expected_source),
  actual: formatStorageEndpoint(issue.actual_fs_type, issue.actual_source),
  diagnostic: issue.diagnostic_message?.trim() || null,
})
