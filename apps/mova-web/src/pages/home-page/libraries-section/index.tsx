import { Link } from 'react-router-dom'
import {
  formatScanJobStatusCopy,
  getEffectiveScanJob,
  getScanJobProgressPercent,
  hasFailedLibraryScan,
  isLibraryScanActive,
} from '../../../components/app-shell/scan-runtime'
import { useI18n } from '../../../i18n'
import { ScrollableRail } from '../../../components/scrollable-rail'
import { SectionHelp } from '../../../components/section-help'
import type { HomeLibraryModuleData } from '../types'

interface LibrariesSectionProps {
  isLoading: boolean
  libraryModules: HomeLibraryModuleData[]
}

const LIBRARY_SPOTLIGHT_SKELETON_COUNT = 3
const LIBRARY_SPOTLIGHT_SKELETON_KEYS = ['library-a', 'library-b', 'library-c'] as const

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
  const { l } = useI18n()

  return (
    <section className="catalog-block libraries-section">
      <div className="catalog-block__header">
        <div className="catalog-block__title-row">
          <h3>{l('Libraries')}</h3>
          <SectionHelp
            detail={l('Browse every library from here. Open one to see the full catalog view.')}
            placement="bottom"
            title={l('About libraries')}
          />
        </div>
        <span className="counter-badge">{libraryModules.length}</span>
      </div>

      {isLoading ? (
        <>
          <p className="muted">{l('Loading libraries…')}</p>
          <ScrollableRail
            hint={l('Scroll horizontally.')}
            viewportClassName="libraries-section__viewport"
          >
            {LIBRARY_SPOTLIGHT_SKELETON_KEYS.slice(0, LIBRARY_SPOTLIGHT_SKELETON_COUNT).map((key) => (
              <LibrarySpotlightSkeleton key={key} />
            ))}
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
          {libraryModules.map(({ detail, detailLoading, library, scanRuntime, shelfItems }) => {
            // Use the first few posters as a lightweight library backdrop so a new library card still
            // feels alive before it gets custom artwork or richer metadata.
            const collagePosters = shelfItems
              .map((item) => item.poster_path ?? item.backdrop_path)
              .filter((value): value is string => Boolean(value))
              .slice(0, 4)
            const lastScan = getEffectiveScanJob(detail?.last_scan ?? null, scanRuntime)
            const isScanning = isLibraryScanActive(lastScan, scanRuntime)
            const hasFailedScan = hasFailedLibraryScan(lastScan, scanRuntime)
            const isSyncingLibraryState = detailLoading && !detail && !isScanning
            const scanCopy = isScanning
              ? formatScanJobStatusCopy(lastScan, scanRuntime)
              : hasFailedScan
                ? l('Recent scan failed')
                : isSyncingLibraryState
                  ? l('Syncing library state')
                  : null
            const scanProgressPercent = isScanning
              ? getScanJobProgressPercent(lastScan, scanRuntime)
              : isSyncingLibraryState
                ? 10
                : 0

            return (
              <Link className="library-spotlight" key={library.id} to={`/libraries/${library.id}`}>
                <div className="library-spotlight__backdrop" aria-hidden="true">
                  {collagePosters.length > 0 ? (
                    collagePosters.map((posterPath, posterIndex) => (
                      <span
                        className={`library-spotlight__poster library-spotlight__poster--${posterIndex + 1}`}
                        key={`${library.id}-${posterPath}`}
                        style={{ backgroundImage: `url(${posterPath})` }}
                      />
                    ))
                  ) : (
                    <span className="library-spotlight__fallback" />
                  )}
                </div>

                <div className="library-spotlight__content">
                  <strong className="library-spotlight__title">{library.name}</strong>

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
                    <span className="library-spotlight__stat">
                      {detailLoading && !detail
                        ? l('syncing…')
                        : l('{{count}} items', { count: detail?.media_count ?? 0 })}
                    </span>
                    <span className="library-spotlight__stat">
                      {l('{{count}} movies', { count: detail?.movie_count ?? 0 })}
                    </span>
                    <span className="library-spotlight__stat">
                      {l('{{count}} series', { count: detail?.series_count ?? 0 })}
                    </span>
                  </div>
                </div>
              </Link>
            )
          })}
        </ScrollableRail>
      )}
    </section>
  )
}
