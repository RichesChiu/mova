import { useQueries, useQuery } from '@tanstack/react-query'
import { Link, useOutletContext } from 'react-router-dom'
import { getLibrary, listRecentlyAddedByLibrary } from '../../api/client'
import type { AppShellOutletContext } from '../../components/app-shell'
import {
  formatScanJobStatusCopy,
  getEffectiveScanJob,
  getLibraryScanRuntime,
  getScanJobProgressPercent,
  hasFailedLibraryScan,
  isLibraryScanActive,
} from '../../components/app-shell/scan-runtime'
import { useI18n } from '../../i18n'
import { cssBackgroundImage } from '../../lib/css'

export const LibrariesPage = () => {
  const { l } = useI18n()
  const { libraries, librariesLoading, scanRuntimeByLibrary } =
    useOutletContext<AppShellOutletContext>()
  const libraryDetailQueries = useQueries({
    queries: libraries.map((library) => ({
      queryKey: ['libraries-page-detail', library.id],
      queryFn: () => getLibrary(library.id),
    })),
  })
  const recentPreviewQuery = useQuery({
    queryKey: ['libraries-page-recently-added', 12, 4],
    queryFn: () => listRecentlyAddedByLibrary(12, 4),
  })
  const recentPreviewByLibraryId = new Map(
    (recentPreviewQuery.data ?? []).map((group) => [group.library.id, group.items]),
  )

  return (
    <div className="page-stack libraries-page">
      <section className="catalog-block libraries-page__header">
        <div>
          <p className="eyebrow">{l('Library Hub')}</p>
          <h2>{l('All Libraries')}</h2>
        </div>
        <span className="counter-badge">{libraries.length}</span>
      </section>

      {recentPreviewQuery.isError ? (
        <p className="callout callout--danger">
          {recentPreviewQuery.error instanceof Error
            ? recentPreviewQuery.error.message
            : l('Failed to load recently added media')}
        </p>
      ) : null}

      {librariesLoading ? (
        <div className="libraries-page__grid">
          {['library-a', 'library-b', 'library-c', 'library-d'].map((key) => (
            <div aria-hidden="true" className="library-hub-card library-hub-card--loading" key={key}>
              <span className="library-hub-card__artwork skeleton-shimmer" />
              <span className="library-hub-card__line library-hub-card__line--title skeleton-shimmer" />
              <span className="library-hub-card__line library-hub-card__line--meta skeleton-shimmer" />
            </div>
          ))}
        </div>
      ) : libraries.length === 0 ? (
        <div className="catalog-block__empty">
          <p className="muted">{l('No libraries yet.')}</p>
        </div>
      ) : (
        <div className="libraries-page__grid">
          {libraries.map((library, index) => {
            const detail = libraryDetailQueries[index]?.data ?? null
            const detailError =
              libraryDetailQueries[index]?.error instanceof Error
                ? libraryDetailQueries[index].error
                : null
            const detailLoading = libraryDetailQueries[index]?.isLoading ?? false
            const currentScanRuntime = getLibraryScanRuntime(scanRuntimeByLibrary, library.id)
            const currentScan = getEffectiveScanJob(detail?.last_scan ?? null, currentScanRuntime)
            const isScanning = isLibraryScanActive(currentScan, currentScanRuntime)
            const hasFailedScan = hasFailedLibraryScan(currentScan, currentScanRuntime)
            const scanCopy = isScanning
              ? formatScanJobStatusCopy(currentScan, currentScanRuntime)
              : hasFailedScan
                ? l('Recent scan failed')
                : null
            const scanProgressPercent = isScanning
              ? getScanJobProgressPercent(currentScan, currentScanRuntime)
              : 0
            const posters = (recentPreviewByLibraryId.get(library.id) ?? [])
              .map((item) => item.poster_path)
              .filter((value): value is string => Boolean(value))
              .slice(0, 4)

            return (
              <Link className="library-hub-card" key={library.id} to={`/libraries/${library.id}`}>
                <div className="library-hub-card__artwork" aria-hidden="true">
                  {posters.length > 0 ? (
                    posters.map((posterPath, posterIndex) => (
                      <span
                        className={`library-hub-card__poster library-hub-card__poster--${posterIndex + 1}`}
                        key={`${library.id}-${posterPath}`}
                        style={{ backgroundImage: cssBackgroundImage(posterPath) }}
                      />
                    ))
                  ) : null}
                </div>

                <div className="library-hub-card__body">
                  <strong>{library.name}</strong>
                  {detailError ? (
                    <span className="library-hub-card__meta">{l('Details unavailable')}</span>
                  ) : detailLoading && !detail ? (
                    <span className="library-hub-card__meta">{l('syncing…')}</span>
                  ) : (
                    <span className="library-hub-card__meta">
                      {l('{{count}} resources', { count: detail?.media_count ?? 0 })}
                    </span>
                  )}

                  {scanCopy ? (
                    <div className="library-hub-card__scan" role="status">
                      <div className="library-hub-card__scan-row">
                        <span>{scanCopy}</span>
                        <strong>{hasFailedScan ? l('failed') : `${scanProgressPercent}%`}</strong>
                      </div>
                      {!hasFailedScan ? (
                        <div className="library-hub-card__scan-track" aria-hidden="true">
                          <span style={{ width: `${scanProgressPercent}%` }} />
                        </div>
                      ) : null}
                    </div>
                  ) : null}
                </div>
              </Link>
            )
          })}
        </div>
      )}
    </div>
  )
}
