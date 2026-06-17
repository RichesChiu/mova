import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import type { Library } from '../../../api/types'
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
import { HomeIcon } from '../home-icons'
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

const LibraryActionsMenu = ({
  isOpen,
  isScanDisabled,
  isScanPending,
  library,
  onClose,
  onDeleteLibrary,
  onEditLibrary,
  onScanLibrary,
  onToggle,
}: {
  isOpen: boolean
  isScanDisabled: boolean
  isScanPending: boolean
  library: Library
  onClose: () => void
  onDeleteLibrary: (library: Library) => void
  onEditLibrary: (library: Library) => void
  onScanLibrary: (library: Library) => void
  onToggle: () => void
}) => {
  const { l } = useI18n()

  return (
    <div className="library-spotlight__actions" data-library-actions-menu={library.id}>
      <button
        aria-expanded={isOpen}
        aria-haspopup="menu"
        aria-label={l('Open library actions menu')}
        className="library-spotlight__menu"
        onClick={onToggle}
        type="button"
      >
        <span />
        <span />
        <span />
      </button>

      {isOpen ? (
        <div
          aria-label={l('Library actions')}
          className="library-spotlight__action-menu glass-popover-surface floating-transition"
          data-state="open"
          role="menu"
        >
          <button
            className="library-spotlight__action-menu-item"
            onClick={() => {
              onClose()
              onEditLibrary(library)
            }}
            role="menuitem"
            type="button"
          >
            <HomeIcon name="edit" />
            <span>{l('Edit Library')}</span>
          </button>
          <button
            className="library-spotlight__action-menu-item"
            disabled={isScanDisabled}
            onClick={() => {
              onClose()
              onScanLibrary(library)
            }}
            role="menuitem"
            type="button"
          >
            <HomeIcon name="scan" />
            <span>{isScanPending ? l('Triggering…') : l('Scan Library')}</span>
          </button>
          <button
            className="library-spotlight__action-menu-item library-spotlight__action-menu-item--danger"
            onClick={() => {
              onClose()
              onDeleteLibrary(library)
            }}
            role="menuitem"
            type="button"
          >
            <HomeIcon name="trash" />
            <span>{l('Delete Library')}</span>
          </button>
        </div>
      ) : null}
    </div>
  )
}

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
}: LibrariesSectionProps) => {
  const { formatNumber, l } = useI18n()
  const [openMenuLibraryId, setOpenMenuLibraryId] = useState<number | null>(null)
  const visibleLibraryModules = libraryModules.slice(0, HOME_LIBRARY_PREVIEW_LIMIT)

  useEffect(() => {
    if (openMenuLibraryId === null) {
      return
    }

    const handlePointerDown = (event: MouseEvent) => {
      if (
        event.target instanceof Element &&
        event.target.closest(`[data-library-actions-menu="${openMenuLibraryId}"]`)
      ) {
        return
      }

      setOpenMenuLibraryId(null)
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpenMenuLibraryId(null)
      }
    }

    document.addEventListener('mousedown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)

    return () => {
      document.removeEventListener('mousedown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [openMenuLibraryId])

  return (
    <section className="catalog-block libraries-section">
      <div className="catalog-block__header">
        <div className="catalog-block__title-row">
          <h3>{l('Your Libraries')}</h3>
          {libraryModules.length > 0 ? (
            <Link className="libraries-section__title-action" to="/libraries">
              {l('View all')}
            </Link>
          ) : null}
        </div>
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
        <>
          {actionErrorMessage ? (
            <p className="callout callout--danger">{actionErrorMessage}</p>
          ) : null}
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
                        isOpen={openMenuLibraryId === library.id}
                        isScanDisabled={isScanPending || isScanning}
                        isScanPending={isScanPending}
                        library={library}
                        onClose={() => setOpenMenuLibraryId(null)}
                        onDeleteLibrary={onDeleteLibrary}
                        onEditLibrary={onEditLibrary}
                        onScanLibrary={onScanLibrary}
                        onToggle={() =>
                          setOpenMenuLibraryId((current) =>
                            current === library.id ? null : library.id,
                          )
                        }
                      />
                    ) : null}
                  </article>
                )
              },
            )}
          </ScrollableRail>
        </>
      )}
    </section>
  )
}
