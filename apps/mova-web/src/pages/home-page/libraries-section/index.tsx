import { Link } from 'react-router-dom'
import {
  formatScanJobStatusCopy,
  getEffectiveScanJob,
  getScanJobProgressPercent,
  hasFailedLibraryScan,
  isLibraryScanActive,
} from '../../../components/app-shell/scan-runtime'
import { ScrollableRail } from '../../../components/scrollable-rail'
import { useI18n } from '../../../i18n'
import { cssBackgroundImage } from '../../../lib/css'
import { getLibraryArtworkSrc } from '../library-artwork'
import type { HomeLibraryModuleData } from '../types'

interface LibrariesSectionProps {
  isLoading: boolean
  libraryModules: HomeLibraryModuleData[]
}

const LIBRARY_SPOTLIGHT_SKELETON_COUNT = 3
const LIBRARY_SPOTLIGHT_SKELETON_KEYS = ['library-a', 'library-b', 'library-c'] as const
const HOME_LIBRARY_PREVIEW_LIMIT = 3

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

export const LibrariesSection = ({ isLoading, libraryModules }: LibrariesSectionProps) => (
  <LibrariesSectionBody isLoading={isLoading} libraryModules={libraryModules} />
)

const LibrariesSectionBody = ({ isLoading, libraryModules }: LibrariesSectionProps) => {
  const { formatNumber, l } = useI18n()
  const visibleLibraryModules = libraryModules.slice(0, HOME_LIBRARY_PREVIEW_LIMIT)

  return (
    <section className="catalog-block libraries-section">
      <div className="catalog-block__header">
        <div className="catalog-block__title-row">
          <h3>{l('Your Libraries')}</h3>
        </div>
        {libraryModules.length > 0 ? (
          <Link className="catalog-block__inline-action" to="/libraries">
            {l('View all')}
          </Link>
        ) : null}
      </div>

      {isLoading ? (
        <>
          <p className="muted">{l('Loading libraries…')}</p>
          <ScrollableRail
            hint={l('Scroll horizontally.')}
            viewportClassName="libraries-section__viewport"
          >
            {LIBRARY_SPOTLIGHT_SKELETON_KEYS.slice(0, LIBRARY_SPOTLIGHT_SKELETON_COUNT).map(
              (key) => (
                <LibrarySpotlightSkeleton key={key} />
              ),
            )}
          </ScrollableRail>
        </>
      ) : libraryModules.length === 0 ? (
        <div className="catalog-block__empty">
          <p className="muted">{l('No libraries yet.')}</p>
        </div>
      ) : (
        <ScrollableRail
          hint={l('Scroll horizontally.')}
          viewportClassName="libraries-section__viewport"
        >
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
              const spotlightClassName = [
                'library-spotlight',
                isScanning ? 'library-spotlight--scanning' : '',
                libraryArtworkSrc ? '' : 'library-spotlight--empty-artwork',
              ]
                .filter(Boolean)
                .join(' ')

              return (
                <Link
                  className={spotlightClassName}
                  key={library.id}
                  to={`/libraries/${library.id}`}
                >
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
                  <span className="library-spotlight__menu" aria-hidden="true">
                    <span />
                    <span />
                    <span />
                  </span>

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
              )
            },
          )}
        </ScrollableRail>
      )}
    </section>
  )
}
