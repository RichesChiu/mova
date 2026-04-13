import { useQuery } from '@tanstack/react-query'
import { Link, useOutletContext, useParams } from 'react-router-dom'
import { getLibrary, listLibraryMediaItems } from '../../api/client'
import type { MediaItem } from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import type { ScanRuntimeItem } from '../../components/app-shell/scan-runtime'
import {
  formatFailedScanCopy,
  formatPendingScanPlaceholderCopy,
  formatScanItemMeta,
  formatScanItemProgressCopy,
  formatScanJobStatusCopy,
  getEffectiveScanJob,
  getLibraryScanRuntime,
  getScanJobProgressPercent,
  getScanRuntimeItems,
  hasFailedLibraryScan,
  isLibraryScanActive,
  shouldShowScanPlaceholder,
} from '../../components/app-shell/scan-runtime'
import { MediaCard, MediaCardScanPlaceholder, MediaCardSkeleton } from '../../components/media-card'

const PAGE_SIZE = 500
const MEDIA_SECTION_SKELETON_COUNT = 6
const MEDIA_SECTION_SKELETON_KEYS = [
  'media-a',
  'media-b',
  'media-c',
  'media-d',
  'media-e',
  'media-f',
] as const

const MediaSection = ({
  items,
  pendingScanPlaceholder,
  scanItems,
  title,
}: {
  items: MediaItem[]
  pendingScanPlaceholder?: {
    placeholderLabel: string
    progressPercent: number
    progressText: string
    title: string
  } | null
  scanItems: ScanRuntimeItem[]
  title: string
}) => {
  return (
    <section className="catalog-block">
      <div className="catalog-block__header">
        <h3>{title}</h3>
      </div>

      {items.length === 0 && scanItems.length === 0 ? (
        <div className="catalog-block__empty">
          <p className="muted">No items in this section yet.</p>
        </div>
      ) : (
        <div className="media-grid">
          {pendingScanPlaceholder ? (
            <MediaCardScanPlaceholder
              placeholderLabel={pendingScanPlaceholder.placeholderLabel}
              progressPercent={pendingScanPlaceholder.progressPercent}
              progressText={pendingScanPlaceholder.progressText}
              subtitle="library"
              title={pendingScanPlaceholder.title}
            />
          ) : null}
          {scanItems.map((item) => (
            <MediaCardScanPlaceholder
              key={`scan-${item.item_key}`}
              placeholderLabel={item.media_type.toUpperCase()}
              progressPercent={item.progress_percent}
              progressText={formatScanItemProgressCopy(item)}
              subtitle={formatScanItemMeta(item)}
              title={item.title}
            />
          ))}
          {items.map((item) => (
            <MediaCard item={item} key={item.id} />
          ))}
        </div>
      )}
    </section>
  )
}

const MediaSectionSkeleton = ({
  placeholderLabel,
  title,
}: {
  placeholderLabel: string
  title: string
}) => {
  return (
    <section aria-hidden="true" className="catalog-block">
      <div className="catalog-block__header">
        <h3>{title}</h3>
      </div>

      <div className="media-grid">
        {MEDIA_SECTION_SKELETON_KEYS.slice(0, MEDIA_SECTION_SKELETON_COUNT).map((key) => (
          <MediaCardSkeleton key={`${title}-${key}`} placeholderLabel={placeholderLabel} />
        ))}
      </div>
    </section>
  )
}

export const LibraryPage = () => {
  const params = useParams()
  const { scanRuntimeByLibrary } = useOutletContext<AppShellOutletContext>()
  const libraryId = Number(params.libraryId)

  const libraryQuery = useQuery({
    enabled: Number.isFinite(libraryId),
    queryKey: ['library', libraryId],
    queryFn: () => getLibrary(libraryId),
    refetchInterval: (query) => {
      const status = query.state.data?.last_scan?.status
      return status === 'pending' || status === 'running' ? 3_000 : false
    },
  })

  const scanStatus = libraryQuery.data?.last_scan?.status

  const mediaItemsQuery = useQuery({
    enabled: Number.isFinite(libraryId),
    queryKey: ['library-media', libraryId, 'full'],
    queryFn: () =>
      listLibraryMediaItems(libraryId, {
        page: 1,
        pageSize: PAGE_SIZE,
      }),
    refetchInterval: scanStatus === 'pending' || scanStatus === 'running' ? 3_000 : false,
  })

  if (!Number.isFinite(libraryId)) {
    return <p className="callout callout--danger">Invalid library id.</p>
  }

  const currentLibrary = libraryQuery.data
  const currentScanRuntime = Number.isFinite(libraryId)
    ? getLibraryScanRuntime(scanRuntimeByLibrary, libraryId)
    : null
  const mediaItems = mediaItemsQuery.data?.items ?? []
  const libraryDescription = currentLibrary?.description?.trim() || null
  const currentScan = getEffectiveScanJob(currentLibrary?.last_scan, currentScanRuntime)
  const hasFailedScan = hasFailedLibraryScan(currentLibrary?.last_scan, currentScanRuntime)
  const scanItems = shouldShowScanPlaceholder(currentLibrary?.last_scan, currentScanRuntime)
    ? getScanRuntimeItems(currentScanRuntime)
    : []
  const movieItems = mediaItems.filter((item) => item.media_type === 'movie')
  const seriesItems = mediaItems.filter((item) => item.media_type === 'series')
  const movieScanItems = scanItems.filter((item) => item.media_type === 'movie')
  const seriesScanItems = scanItems.filter((item) => item.media_type !== 'movie')
  const shouldShowMediaSkeleton =
    mediaItemsQuery.isLoading && mediaItems.length === 0 && scanItems.length === 0
  const scanProgressPercent = getScanJobProgressPercent(currentScan, currentScanRuntime)
  const scanCopy = formatScanJobStatusCopy(currentScan, currentScanRuntime)
  const isScanning = isLibraryScanActive(currentScan, currentScanRuntime)
  const pendingScanPlaceholder =
    isScanning && scanItems.length === 0
      ? {
          placeholderLabel: 'MEDIA',
          progressPercent: scanProgressPercent,
          progressText: formatPendingScanPlaceholderCopy(
            currentScan,
            currentScanRuntime,
            currentLibrary?.name ?? 'Current library',
          ),
          title: currentLibrary?.name ?? 'Scanning library',
        }
      : null

  return (
    <div className="page-stack">
      <div className="library-page__toolbar">
        <Link className="back-link library-page__home-link" to="/">
          <svg aria-hidden="true" className="back-link__icon" fill="none" viewBox="0 0 16 16">
            <path
              d="M9.5 3.5L5.5 8L9.5 12.5"
              stroke="currentColor"
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth="1.8"
            />
          </svg>
          <span>Back Home</span>
        </Link>
      </div>

      <section className="library-hero library-hero--compact">
        <div className="library-hero__content">
          <div className="library-hero__copy">
            <h2>{currentLibrary?.name ?? 'Loading…'}</h2>
            {libraryDescription ? (
              <p className="library-hero__description">{libraryDescription}</p>
            ) : null}
          </div>

          <div className="library-hero__meta">
            <div className="hero-stat">
              <span className="hero-stat__label">Detected</span>
              <strong>
                {(currentLibrary?.movie_count ?? 0) + (currentLibrary?.series_count ?? 0) > 0
                  ? `${currentLibrary?.movie_count ?? 0} movies / ${currentLibrary?.series_count ?? 0} series`
                  : 'Auto'}
              </strong>
            </div>
            <div className="hero-stat">
              <span className="hero-stat__label">Items</span>
              <strong>{currentLibrary?.media_count ?? mediaItemsQuery.data?.total ?? 0}</strong>
            </div>
          </div>
        </div>
      </section>

      {libraryQuery.isError ? (
        <p className="callout callout--danger">
          {libraryQuery.error instanceof Error
            ? libraryQuery.error.message
            : 'Failed to load library'}
        </p>
      ) : null}

      {currentScan && (currentScan.status === 'pending' || currentScan.status === 'running') ? (
        <p className="callout">
          This library is syncing.{scanCopy ? ` ${scanCopy}.` : null}
          {currentScan.total_files > 0
            ? ` Current task progress is about ${scanProgressPercent}%.`
            : ' The sync discovers files first, then enriches metadata and artwork item by item.'}{' '}
          Browsing stays available while the sync is running.
        </p>
      ) : null}

      {hasFailedScan ? (
        <p className="callout callout--danger">
          The most recent scan failed.
          {` ${formatFailedScanCopy(currentLibrary?.last_scan, currentScanRuntime)}`}. Existing
          items are still available, and an admin can trigger another scan later.
        </p>
      ) : null}

      <section className="catalog-shell">
        {shouldShowMediaSkeleton ? <p className="muted">Loading media items…</p> : null}

        {mediaItemsQuery.isError ? (
          <p className="callout callout--danger">
            {mediaItemsQuery.error instanceof Error
              ? mediaItemsQuery.error.message
              : 'Failed to load media items'}
          </p>
        ) : null}

        {!shouldShowMediaSkeleton &&
        mediaItemsQuery.data &&
        mediaItems.length === 0 &&
        scanItems.length === 0 ? (
          <section className="empty-panel">
            <h3>No items available yet</h3>
            <p className="muted">This library does not have any visible items yet.</p>
          </section>
        ) : null}

        {shouldShowMediaSkeleton ? (
          <div className="catalog-stack">
            <MediaSectionSkeleton placeholderLabel="MOVIE" title="Movies" />
            <MediaSectionSkeleton placeholderLabel="SERIES" title="Series" />
          </div>
        ) : null}

        {!shouldShowMediaSkeleton && (mediaItems.length > 0 || scanItems.length > 0) ? (
          <div className="catalog-stack">
            <MediaSection
              items={movieItems}
              pendingScanPlaceholder={movieScanItems.length === 0 ? pendingScanPlaceholder : null}
              scanItems={movieScanItems}
              title="Movies"
            />
            <MediaSection
              items={seriesItems}
              pendingScanPlaceholder={null}
              scanItems={seriesScanItems}
              title="Series"
            />
          </div>
        ) : null}
      </section>
    </div>
  )
}
