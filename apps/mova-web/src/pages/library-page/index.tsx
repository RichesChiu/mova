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
  getEffectiveScanJob,
  getLibraryScanRuntime,
  getScanJobProgressPercent,
  getScanRuntimeItems,
  hasFailedLibraryScan,
  isLibraryScanActive,
  shouldShowScanPlaceholder,
} from '../../components/app-shell/scan-runtime'
import { MediaCard, MediaCardScanPlaceholder, MediaCardSkeleton } from '../../components/media-card'
import { useI18n } from '../../i18n'
import { getLibraryMediaSection, getLibraryScanSection } from '../../lib/library-media-sections'

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

const formatLibraryScanItemSubtitle = (item: ScanRuntimeItem) => {
  if (
    typeof item.season_number === 'number' &&
    Number.isFinite(item.season_number) &&
    typeof item.episode_number === 'number' &&
    Number.isFinite(item.episode_number)
  ) {
    return formatScanItemMeta(item)
  }

  return null
}

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
  const { l } = useI18n()
  return (
    <section className="catalog-block">
      <div className="catalog-block__header">
        <h3>{title}</h3>
      </div>

      {items.length === 0 && scanItems.length === 0 ? (
        <div className="catalog-block__empty">
          <p className="muted">{l('No items in this section yet.')}</p>
        </div>
      ) : (
        <div className="media-grid">
          {pendingScanPlaceholder ? (
            <MediaCardScanPlaceholder
              placeholderLabel={pendingScanPlaceholder.placeholderLabel}
              progressPercent={pendingScanPlaceholder.progressPercent}
              progressText={pendingScanPlaceholder.progressText}
              title={pendingScanPlaceholder.title}
            />
          ) : null}
          {scanItems.map((item) => (
            <MediaCardScanPlaceholder
              key={`scan-${item.item_key}`}
              placeholderLabel={item.media_type.toUpperCase()}
              progressPercent={item.progress_percent}
              progressText={formatScanItemProgressCopy(item)}
              subtitle={formatLibraryScanItemSubtitle(item)}
              title={item.title}
            />
          ))}
          {items.map((item) => (
            <MediaCard item={item} key={item.id} showTypeTag={false} />
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
  const { l } = useI18n()
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
    return <p className="callout callout--danger">{l('Invalid library id.')}</p>
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
  const movieItems = mediaItems.filter((item) => getLibraryMediaSection(item) === 'movies')
  const seriesItems = mediaItems.filter((item) => getLibraryMediaSection(item) === 'series')
  const otherItems = mediaItems.filter((item) => getLibraryMediaSection(item) === 'other')
  const movieScanItems = scanItems.filter((item) => getLibraryScanSection(item) === 'movies')
  const seriesScanItems = scanItems.filter((item) => getLibraryScanSection(item) === 'series')
  const otherScanItems = scanItems.filter((item) => getLibraryScanSection(item) === 'other')
  const shouldShowMediaSkeleton =
    mediaItemsQuery.isLoading && mediaItems.length === 0 && scanItems.length === 0
  const scanProgressPercent = getScanJobProgressPercent(currentScan, currentScanRuntime)
  const isScanning = isLibraryScanActive(currentScan, currentScanRuntime)
  const pendingScanPlaceholder =
    isScanning && scanItems.length === 0
      ? {
          placeholderLabel: l('Media'),
          progressPercent: scanProgressPercent,
          progressText: formatPendingScanPlaceholderCopy(
            currentScan,
            currentScanRuntime,
            currentLibrary?.name ?? l('Current library'),
          ),
          title: currentLibrary?.name ?? l('Scanning library'),
        }
      : null
  const detectedMovieCount = mediaItemsQuery.data ? movieItems.length : (currentLibrary?.movie_count ?? 0)
  const detectedSeriesCount = mediaItemsQuery.data
    ? seriesItems.length
    : (currentLibrary?.series_count ?? 0)
  const detectedOtherCount = mediaItemsQuery.data ? otherItems.length : 0
  const detectedSummary =
    detectedMovieCount + detectedSeriesCount + detectedOtherCount > 0
      ? detectedOtherCount > 0
        ? l('{{movies}} movies / {{series}} series / {{other}} other', {
            movies: detectedMovieCount,
            other: detectedOtherCount,
            series: detectedSeriesCount,
          })
        : l('{{movies}} movies / {{series}} series', {
            movies: detectedMovieCount,
            series: detectedSeriesCount,
          })
      : l('Automatic')

  return (
    <div className="page-stack library-page">
      <section className="library-hero library-hero--compact">
        <div className="library-hero__content">
          <div className="library-hero__navigation-row">
            <Link className="back-link library-hero__back-link" to="/">
              <svg aria-hidden="true" className="back-link__icon" fill="none" viewBox="0 0 16 16">
                <path
                  d="M9.5 3.5L5.5 8L9.5 12.5"
                  stroke="currentColor"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth="1.8"
                />
              </svg>
              <span>{l('Back Home')}</span>
            </Link>
          </div>

          <div className="library-hero__copy">
            <h2>{currentLibrary?.name ?? l('Loading…')}</h2>
            {libraryDescription ? (
              <p className="library-hero__description">{libraryDescription}</p>
            ) : null}
          </div>

          <div className="library-hero__meta">
            <div className="hero-stat">
              <span className="hero-stat__label">{l('Detected')}</span>
              <strong>{detectedSummary}</strong>
            </div>
            <div className="hero-stat">
              <span className="hero-stat__label">{l('Items')}</span>
              <strong>{currentLibrary?.media_count ?? mediaItemsQuery.data?.total ?? 0}</strong>
            </div>
          </div>
        </div>
      </section>

      {libraryQuery.isError ? (
        <p className="callout callout--danger">
          {libraryQuery.error instanceof Error
            ? libraryQuery.error.message
            : l('Failed to load library')}
        </p>
      ) : null}

      {hasFailedScan ? (
        <p className="callout callout--danger">
          {l('The most recent scan failed.')}
          {` ${formatFailedScanCopy(currentLibrary?.last_scan, currentScanRuntime)}`}. Existing
          {` ${l(
            'Existing items are still available, and an admin can trigger another scan later.',
          )}`}
        </p>
      ) : null}

      <section className="catalog-shell">
        {shouldShowMediaSkeleton ? <p className="muted">{l('Loading media items…')}</p> : null}

        {mediaItemsQuery.isError ? (
          <p className="callout callout--danger">
            {mediaItemsQuery.error instanceof Error
              ? mediaItemsQuery.error.message
              : l('Failed to load media items')}
          </p>
        ) : null}

        {!shouldShowMediaSkeleton &&
        mediaItemsQuery.data &&
        mediaItems.length === 0 &&
        scanItems.length === 0 ? (
          <section className="empty-panel">
            <h3>{l('No items available yet')}</h3>
            <p className="muted">{l('This library does not have any visible items yet.')}</p>
          </section>
        ) : null}

        {shouldShowMediaSkeleton ? (
          <div className="catalog-stack">
            <MediaSectionSkeleton placeholderLabel={l('Movies')} title={l('Movies')} />
            <MediaSectionSkeleton placeholderLabel={l('Series')} title={l('Series')} />
            <MediaSectionSkeleton placeholderLabel={l('Other')} title={l('Other')} />
          </div>
        ) : null}

        {!shouldShowMediaSkeleton && (mediaItems.length > 0 || scanItems.length > 0) ? (
          <div className="catalog-stack">
            <MediaSection
              items={movieItems}
              pendingScanPlaceholder={null}
              scanItems={movieScanItems}
              title={l('Movies')}
            />
            <MediaSection
              items={seriesItems}
              pendingScanPlaceholder={null}
              scanItems={seriesScanItems}
              title={l('Series')}
            />
            <MediaSection
              items={otherItems}
              pendingScanPlaceholder={otherScanItems.length === 0 ? pendingScanPlaceholder : null}
              scanItems={otherScanItems}
              title={l('Other')}
            />
          </div>
        ) : null}
      </section>
    </div>
  )
}
