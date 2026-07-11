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
import { DashboardPageHeader } from '../home-page/dashboard-page-header'
import { HomeDashboardShell } from '../home-page/home-dashboard-shell'
import { getLibraryArtworkSrc } from '../home-page/library-artwork'

export const LibrariesPage = () => {
  const { formatNumber, l } = useI18n()
  const { currentUser, libraries, librariesLoading, scanRuntimeByLibrary } =
    useOutletContext<AppShellOutletContext>()
  const libraryDetailQueries = useQueries({
    queries: libraries.map((library) => ({
      queryKey: ['libraries-page-detail', library.id],
      queryFn: () => getLibrary(library.id),
    })),
  })
  const recentPreviewQuery = useQuery({
    enabled: libraries.length > 0,
    queryKey: ['libraries-page-recently-added', 8],
    queryFn: () => listRecentlyAddedByLibrary({ limit: 8 }),
  })
  const recentPreviewByLibraryId = new Map(
    (recentPreviewQuery.data ?? []).map((group) => [group.library.id, group.items]),
  )

  return (
    <HomeDashboardShell ariaLabel={l('Libraries')} currentUser={currentUser}>
      <div className="home-dashboard__content home-dashboard__content--libraries">
        <DashboardPageHeader>
          <h2>{l('All Libraries')}</h2>
          <span className="home-dashboard-page-header__meta">
            {formatNumber(libraries.length)} {l('Libraries')}
          </span>
        </DashboardPageHeader>

        <section className="catalog-block libraries-page">
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
                <div
                  aria-hidden="true"
                  className="library-spotlight library-spotlight--loading"
                  key={key}
                >
                  <div className="library-spotlight__backdrop">
                    <span className="library-spotlight__fallback library-spotlight__fallback--loading skeleton-shimmer" />
                  </div>
                  <div className="library-spotlight__content">
                    <span className="library-spotlight__line library-spotlight__line--title skeleton-shimmer" />
                    <span className="library-spotlight__line library-spotlight__line--meta skeleton-shimmer" />
                  </div>
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
                const currentScan = getEffectiveScanJob(
                  detail?.last_scan ?? null,
                  currentScanRuntime,
                )
                const isScanning = isLibraryScanActive(currentScan, currentScanRuntime)
                const hasFailedScan = hasFailedLibraryScan(currentScan, currentScanRuntime)
                const scanCopy = isScanning
                  ? formatScanJobStatusCopy(currentScan, currentScanRuntime)
                  : hasFailedScan
                    ? l('Recent scan failed')
                    : detailError
                      ? l('Failed to load library details')
                      : detailLoading && !detail
                        ? l('Syncing library state')
                        : null
                const scanProgressPercent = isScanning
                  ? getScanJobProgressPercent(currentScan, currentScanRuntime)
                  : detailLoading && !detail
                    ? 10
                    : 0
                const mediaCount = detail?.media_count ?? 0
                const movieCount = detail?.movie_count ?? 0
                const seriesCount = detail?.series_count ?? 0
                const otherCount = mediaCount - movieCount - seriesCount
                const libraryArtworkSrc = getLibraryArtworkSrc(
                  recentPreviewByLibraryId.get(library.id) ?? [],
                )
                const cardClassName = [
                  'library-spotlight',
                  'libraries-page__card',
                  isScanning ? 'library-spotlight--scanning' : '',
                  libraryArtworkSrc ? '' : 'library-spotlight--empty-artwork',
                ]
                  .filter(Boolean)
                  .join(' ')

                return (
                  <article className={cardClassName} key={library.id}>
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
                  </article>
                )
              })}
            </div>
          )}
        </section>
      </div>
    </HomeDashboardShell>
  )
}
