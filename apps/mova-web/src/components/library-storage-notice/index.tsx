import type { Library } from '../../api/types'
import { useI18n } from '../../i18n'
import { describeLibraryStorageIssue, isLibraryStorageUnavailable } from '../../lib/library-storage'
import './library-storage-notice.scss'

interface LibraryStorageNoticeProps {
  canManageLibraries: boolean
  className?: string
  library: Library
  onDeleteLibrary?: (library: Library) => void
  showLibraryName?: boolean
}

/**
 * Explains why a library is unavailable. Owners and admins see the storage
 * facts and can delete the library; everyone else sees that it is paused.
 */
export const LibraryStorageNotice = ({
  canManageLibraries,
  className,
  library,
  onDeleteLibrary,
  showLibraryName = false,
}: LibraryStorageNoticeProps) => {
  const { formatDateTime, l } = useI18n()
  if (!isLibraryStorageUnavailable(library)) {
    return null
  }

  const details =
    canManageLibraries && library.storage_issue
      ? describeLibraryStorageIssue(library.storage_issue)
      : null
  const title = showLibraryName
    ? l('"{{name}}" storage unavailable', { name: library.name })
    : l('Library storage unavailable')

  return (
    <section
      className={['library-storage-notice', className].filter(Boolean).join(' ')}
      role="alert"
    >
      <div className="library-storage-notice__copy">
        <strong>{title}</strong>
        <p>{details ? details.reason : l('This library is temporarily unavailable.')}</p>
        <p className="library-storage-notice__hint">
          {canManageLibraries
            ? l(
                'Existing media and playback history are kept. Scans resume once the storage is back.',
              )
            : l('Existing media and playback history are kept.')}
        </p>
      </div>

      {details ? (
        <dl className="library-storage-notice__facts">
          <div>
            <dt>{l('Mount point')}</dt>
            <dd>{details.mountPoint}</dd>
          </div>
          {details.expected ? (
            <div>
              <dt>{l('Expected')}</dt>
              <dd>{details.expected}</dd>
            </div>
          ) : null}
          <div>
            <dt>{l('Current')}</dt>
            <dd>{details.actual ?? l('Not mounted')}</dd>
          </div>
          {library.storage_unavailable_since ? (
            <div>
              <dt>{l('Since')}</dt>
              <dd>
                <time dateTime={library.storage_unavailable_since}>
                  {formatDateTime(library.storage_unavailable_since)}
                </time>
              </dd>
            </div>
          ) : null}
          {details.diagnostic ? (
            <div>
              <dt>{l('Info')}</dt>
              <dd className="library-storage-notice__diagnostic">{details.diagnostic}</dd>
            </div>
          ) : null}
        </dl>
      ) : null}

      {canManageLibraries ? (
        <div className="library-storage-notice__actions">
          <p className="library-storage-notice__hint">
            {l(
              'If the share was mounted after Mova started, restart the container. If this storage is gone for good, delete the library.',
            )}
          </p>
          {onDeleteLibrary ? (
            <button
              className="button button--danger"
              onClick={() => onDeleteLibrary(library)}
              type="button"
            >
              {l('Delete Library')}
            </button>
          ) : null}
        </div>
      ) : null}
    </section>
  )
}
