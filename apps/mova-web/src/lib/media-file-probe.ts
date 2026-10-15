import type { MediaFile } from '../api/types'
import { localizeApiError } from './api-error'

/** Explains why a file could not be analyzed, or `null` when it was. */
export const describeMediaFileProbeIssue = (file: Pick<MediaFile, 'probe_issue'>): string | null =>
  file.probe_issue ? localizeApiError(`probe_${file.probe_issue}`) : null
