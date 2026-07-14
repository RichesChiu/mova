import { Link } from 'react-router-dom'
import type { Library } from '../../../api/types'
import {
  formatScanJobStatusCopy,
  getEffectiveScanJob,
  getScanJobProgressPercent,
  hasFailedLibraryScan,
  isLibraryScanActive,
} from '../../../components/app-shell/scan-runtime'
import { EmptyState } from '../../../components/empty-state'
import { LibraryActionsMenu } from '../../../components/library-actions-menu'
import { useI18n } from '../../../i18n'
import { cssBackgroundImage } from '../../../lib/css'
import {
  getVisibleHomeLibraries,
  HOME_LIBRARY_LIMIT,
  shouldShowAllHomeLibraries,
} from '../../../lib/home-sections'
import { getLibraryArtworkSrc } from '../library-artwork'
import type { HomeLibraryModuleData } from '../types'

interface LibrariesSectionProps {
  actionErrorMessage?: string | null
  canManageLibraries: boolean
  isLoading: boolean
  libraryModules: HomeLibraryModuleData[]
  pendingScanLibraryId?: number | null
  onDeleteLibrary: (library: Library) => void
  onEditLibrary: (library: Library) => void
  onScanLibrary: (library: Library) => void
  totalLibraryCount: number
}

const LIBRARY_SPOTLIGHT_SKELETON_KEYS = [
  'library-a',
  'library-b',
  'library-c',
  'library-d',
  'library-e',
] as const

const LibrarySpotlightSkeleton = () => (
  <div aria-hidden="true" className="library-spotlight library-spotlight--loading">
    <div className="library-spotlight__backdrop" aria-hidden="true">
      <span className="library-spotlight__fallback library-spotlight__fallback--loading skeleton-shimmer" />
    </div>

    <div className="library-spotlight__content">
      <span className="library-spotlight__line library-spotlight__line--title skeleton-shimmer" />

      <div className="library-spotlight__stats">
        <span className="library-spotlight__stat library-spotlight__stat--loading skeleton-shimmer" />
        <span className="library-spotlight__stat library-spotlight__stat--loading skeleton-shimmer" />
      </div>
    </div>
  </div>
)

export const LibrariesSection = (props: LibrariesSectionProps) => (
  <LibrariesSectionBody {...props} />
)

const LibrariesSectionBody = ({
  actionErrorMessage,
  canManageLibraries,
  isLoading,
  libraryModules,
  pendingScanLibraryId = null,
  onDeleteLibrary,
  onEditLibrary,
  onScanLibrary,
  totalLibraryCount,
}: LibrariesSectionProps) => {
  const { formatNumber, l } = useI18n()
  const visibleLibraryModules = getVisibleHomeLibraries(libraryModules)

  return (
    <section className="catalog-block libraries-section">
      <div className="catalog-block__header">
        <div className="catalog-block__title-row">
          <h3>{l('Your Libraries')}</h3>
          {shouldShowAllHomeLibraries(totalLibraryCount) ? (
            <Link className="libraries-section__title-action" to="/libraries">
              {l('View all')}
            </Link>
          ) : null}
        </div>
      </div>

      {isLoading ? (
        <>
          <p className="muted">{l('Loading libraries…')}</p>
          <div className="libraries-section__grid">
            {LIBRARY_SPOTLIGHT_SKELETON_KEYS.slice(0, HOME_LIBRARY_LIMIT).map((key) => (
              <LibrarySpotlightSkeleton key={key} />
            ))}
          </div>
        </>
      ) : libraryModules.length === 0 ? (
        <EmptyState
          description={l('Create a library in Server Settings to start organizing your media.')}
          title={l('No libraries yet.')}
        />
      ) : (
        <>
          {actionErrorMessage ? (
            <p className="callout callout--danger">{actionErrorMessage}</p>
          ) : null}
          <div className="libraries-section__grid">
            {visibleLibraryModules.map(
              ({ detail, detailError, detailLoading, library, recentItems, scanRuntime }) => {
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
                const isScanPending = pendingScanLibraryId === library.id
                const spotlightClassName = [
                  'library-spotlight',
                  isScanning ? 'library-spotlight--scanning' : '',
                  libraryArtworkSrc ? '' : 'library-spotlight--empty-artwork',
                ]
                  .filter(Boolean)
                  .join(' ')

                return (
                  <article className={spotlightClassName} key={library.id}>
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
              },
            )}
          </div>
        </>
      )}
    </section>
  )
}
