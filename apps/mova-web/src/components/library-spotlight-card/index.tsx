import { Link } from 'react-router-dom'
import type { Library, LibraryDetail, MediaItem } from '../../api/types'
import { useI18n } from '../../i18n'
import { cssBackgroundImage } from '../../lib/css'
import {
  formatScanJobStatusCopy,
  getEffectiveScanJob,
  getScanJobProgressPercent,
  hasFailedLibraryScan,
  isLibraryScanActive,
  type LibraryScanRuntime,
} from '../app-shell/scan-runtime'
import { LibraryActionsMenu } from '../library-actions-menu'

interface LibrarySpotlightCardProps {
  canManageLibraries: boolean
  className?: string
  detail: LibraryDetail | null
  detailError: Error | null
  detailLoading: boolean
  isScanPending?: boolean
  library: Library
  onDeleteLibrary: (library: Library) => void
  onEditLibrary: (library: Library) => void
  onScanLibrary: (library: Library) => void
  recentItems: MediaItem[]
  scanRuntime: LibraryScanRuntime
}

const getLibraryArtworkSrc = (items: MediaItem[]) =>
  items.find((item) => item.backdrop_path)?.backdrop_path ??
  items.find((item) => item.poster_path)?.poster_path ??
  null

export const LibrarySpotlightCardSkeleton = ({ className }: { className?: string }) => (
  <div
    aria-hidden="true"
    className={['library-spotlight', 'library-spotlight--loading', className]
      .filter(Boolean)
      .join(' ')}
  >
    <div className="library-spotlight__backdrop" aria-hidden="true">
      <span className="library-spotlight__fallback library-spotlight__fallback--loading skeleton-shimmer" />
    </div>

    <div className="library-spotlight__content">
      <span className="library-spotlight__line library-spotlight__line--title skeleton-shimmer" />
      <span className="library-spotlight__line library-spotlight__line--meta skeleton-shimmer" />
    </div>
  </div>
)

export const LibrarySpotlightCard = ({
  canManageLibraries,
  className,
  detail,
  detailError,
  detailLoading,
  isScanPending = false,
  library,
  onDeleteLibrary,
  onEditLibrary,
  onScanLibrary,
  recentItems,
  scanRuntime,
}: LibrarySpotlightCardProps) => {
  const { formatNumber, l } = useI18n()
  const mediaCount = detail?.media_count ?? 0
  const movieCount = detail?.movie_count ?? 0
  const seriesCount = detail?.series_count ?? 0
  const otherCount = mediaCount - movieCount - seriesCount
  const libraryArtworkSrc = getLibraryArtworkSrc(recentItems)
  const lastScan = getEffectiveScanJob(detail?.last_scan ?? null, scanRuntime)
  const isScanning = isLibraryScanActive(lastScan, scanRuntime)
  const hasFailedScan = hasFailedLibraryScan(lastScan, scanRuntime)
  const isSyncingLibraryState = detailLoading && !detail && !isScanning
  const scanCopy = isScanning
    ? formatScanJobStatusCopy(lastScan, scanRuntime)
    : hasFailedScan
      ? l('Recent scan failed')
      : detailError
        ? l('Failed to load library details')
        : isSyncingLibraryState
          ? l('Syncing library state')
          : null
  const scanProgressPercent = isScanning
    ? getScanJobProgressPercent(lastScan, scanRuntime)
    : isSyncingLibraryState
      ? 10
      : 0
  const cardClassName = [
    'library-spotlight',
    className,
    isScanning ? 'library-spotlight--scanning' : '',
    libraryArtworkSrc ? '' : 'library-spotlight--empty-artwork',
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <article className={cardClassName}>
      <Link className="library-spotlight__link" to={`/libraries/${library.id}`}>
        <div className="library-spotlight__backdrop" aria-hidden="true">
          {libraryArtworkSrc ? (
            <span
              className="library-spotlight__poster"
              style={{ backgroundImage: cssBackgroundImage(libraryArtworkSrc) }}
            />
          ) : (
            <span className="library-spotlight__fallback" />
          )}
        </div>

        <div className="library-spotlight__content">
          <div className="library-spotlight__summary">
            <strong className="library-spotlight__title">{library.name}</strong>
            <span className="library-spotlight__resource-count">
              {formatNumber(mediaCount)} {l('Resources')}
            </span>
          </div>

          {scanCopy ? (
            <div
              className={
                hasFailedScan
                  ? 'library-spotlight__scan library-spotlight__scan--failed'
                  : 'library-spotlight__scan'
              }
              role="status"
            >
              <div className="library-spotlight__scan-row">
                <span className="library-spotlight__scan-label">{scanCopy}</span>
                <span className="library-spotlight__scan-value">
                  {hasFailedScan ? l('failed') : `${scanProgressPercent}%`}
                </span>
              </div>
              {!hasFailedScan ? (
                <div aria-hidden="true" className="library-spotlight__scan-track">
                  <span
                    className="library-spotlight__scan-fill"
                    style={{ width: `${scanProgressPercent}%` }}
                  />
                </div>
              ) : null}
            </div>
          ) : null}

          <div className="library-spotlight__stats">
            {detailError ? (
              <span className="library-spotlight__stat library-spotlight__stat--wide">
                {l('Details unavailable')}
              </span>
            ) : (
              <>
                <span className="library-spotlight__stat">
                  {detailLoading && !detail ? (
                    l('syncing…')
                  ) : (
                    <>
                      <strong>{formatNumber(seriesCount)}</strong>
                      <span>{l('Series')}</span>
                    </>
                  )}
                </span>
                <span className="library-spotlight__stat">
                  <strong>{formatNumber(movieCount)}</strong>
                  <span>{l('Movies')}</span>
                </span>
                <span className="library-spotlight__stat">
                  <strong>{formatNumber(otherCount)}</strong>
                  <span>{l('Other')}</span>
                </span>
              </>
            )}
          </div>
        </div>
      </Link>

      {canManageLibraries ? (
        <LibraryActionsMenu
          className="library-spotlight__actions"
          isScanDisabled={isScanPending || isScanning}
          isScanPending={isScanPending}
          library={library}
          onDeleteLibrary={onDeleteLibrary}
          onEditLibrary={onEditLibrary}
          onScanLibrary={onScanLibrary}
        />
      ) : null}
    </article>
  )
}
