import { Link } from 'react-router-dom'
import {
  formatScanJobStatusCopy,
  getScanJobProgressPercent,
  isScanJobActive,
} from '../../../components/app-shell/scan-runtime'
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
      <span className="library-spotlight__type">library</span>
      <span className="library-spotlight__line library-spotlight__line--title skeleton-shimmer" />

      <div className="library-spotlight__stats">
        <span className="library-spotlight__stat library-spotlight__stat--loading skeleton-shimmer" />
        <span className="library-spotlight__stat library-spotlight__stat--loading skeleton-shimmer" />
      </div>
    </div>
  </div>
)

export const LibrariesSection = ({ isLoading, libraryModules }: LibrariesSectionProps) => (
  <section className="catalog-block libraries-section">
    <div className="catalog-block__header">
      <div className="catalog-block__title-row">
        <h3>Libraries</h3>
        <SectionHelp
          detail="Browse every library from here. Open one to see the full catalog view."
          title="About libraries"
        />
      </div>
      <span className="counter-badge">{libraryModules.length}</span>
    </div>

    {isLoading ? (
      <>
        <p className="muted">Loading libraries…</p>
        <ScrollableRail hint="Scroll horizontally." viewportClassName="libraries-section__viewport">
          {LIBRARY_SPOTLIGHT_SKELETON_KEYS.slice(0, LIBRARY_SPOTLIGHT_SKELETON_COUNT).map((key) => (
            <LibrarySpotlightSkeleton key={key} />
          ))}
        </ScrollableRail>
      </>
    ) : libraryModules.length === 0 ? (
      <div className="catalog-block__empty">
        <p className="muted">No libraries yet.</p>
      </div>
    ) : (
      <ScrollableRail hint="Scroll horizontally." viewportClassName="libraries-section__viewport">
        {libraryModules.map(({ detail, library, scanRuntime, shelfItems }) => {
          // Use the first few posters as a lightweight library backdrop so a new library card still
          // feels alive before it gets custom artwork or richer metadata.
          const collagePosters = shelfItems
            .map((item) => item.poster_path ?? item.backdrop_path)
            .filter((value): value is string => Boolean(value))
            .slice(0, 4)
          const lastScan = detail?.last_scan ?? null
          const isScanning = isScanJobActive(lastScan)
          const scanCopy = formatScanJobStatusCopy(lastScan, scanRuntime)
          const scanProgressPercent = getScanJobProgressPercent(lastScan, scanRuntime)

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
                <span className="library-spotlight__type">{library.library_type}</span>
                <strong className="library-spotlight__title">{library.name}</strong>

                <div className="library-spotlight__stats">
                  <span className="library-spotlight__stat">{detail?.media_count ?? 0} items</span>
                  {library.library_type === 'mixed' ? (
                    <>
                      <span className="library-spotlight__stat">
                        {detail?.movie_count ?? 0} movies
                      </span>
                      <span className="library-spotlight__stat">
                        {detail?.series_count ?? 0} series
                      </span>
                    </>
                  ) : null}
                </div>

                {isScanning ? (
                  <div className="library-spotlight__scan" role="status">
                    <div className="library-spotlight__scan-row">
                      <span className="library-spotlight__scan-label">{scanCopy}</span>
                      <span className="library-spotlight__scan-value">{scanProgressPercent}%</span>
                    </div>
                    <div aria-hidden="true" className="library-spotlight__scan-track">
                      <span
                        className="library-spotlight__scan-fill"
                        style={{ width: `${scanProgressPercent}%` }}
                      />
                    </div>
                  </div>
                ) : null}
              </div>
            </Link>
          )
        })}
      </ScrollableRail>
    )}
  </section>
)
