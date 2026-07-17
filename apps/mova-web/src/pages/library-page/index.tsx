import { useQuery } from '@tanstack/react-query'
import { Link, useNavigate, useOutletContext, useParams } from 'react-router-dom'
import { getLibrary, listLibraryMediaItems } from '../../api/client'
import type { MediaItem } from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import type { ScanRuntimeItem } from '../../components/app-shell/scan-runtime'
import {
  formatFailedScanCopy,
  formatScanItemCardProgressLabel,
  formatScanItemCardSummary,
  formatScanItemMeta,
  formatScanJobStatusCopy,
  getEffectiveScanJob,
  getLibraryScanRuntime,
  getScanItemCardProgressPercent,
  getScanJobProgressPercent,
  getScanRuntimeItems,
  hasFailedLibraryScan,
  isLibraryScanActive,
  shouldShowScanPlaceholder,
} from '../../components/app-shell/scan-runtime'
import { EmptyState } from '../../components/empty-state'
import { useI18n } from '../../i18n'
import {
  filterCompletedScanItemsWithSavedMedia,
  filterLibraryMediaItemsForScanRuntime,
  getLibraryMediaSection,
  getLibraryScanSection,
} from '../../lib/library-media-sections'
import { mediaItemPrimaryPath } from '../../lib/media-routes'
import { formatLibraryMediaTypeLabel } from '../../lib/media-type-label'
import { DashboardPageHeader } from '../home-page/dashboard-page-header'
import { HomeDashboardShell } from '../home-page/home-dashboard-shell'
import { HomeIcon } from '../home-page/home-icons'
import { LibraryDetailTileArtwork } from './library-detail-tile-artwork'

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

const LibraryDetailMediaTile = ({ item }: { item: MediaItem }) => {
  const { l } = useI18n()
  const title = item.title.trim() || item.source_title.trim() || l('Untitled')
  const mediaTypeLabel = formatLibraryMediaTypeLabel(item.media_type, l)
  const metaLabel = item.year ? `${mediaTypeLabel} · ${item.year}` : mediaTypeLabel

  return (
    <Link className="library-detail-tile" to={mediaItemPrimaryPath(item)}>
      <LibraryDetailTileArtwork
        alt={l('{{title}} poster', { title })}
        placeholderLabel={mediaTypeLabel}
        src={item.poster_path}
      />
      <div className="library-detail-tile__copy">
        <strong title={title}>{title}</strong>
        <span>{metaLabel}</span>
      </div>
    </Link>
  )
}

const LibraryDetailScanTile = ({ item }: { item: ScanRuntimeItem }) => {
  const { l } = useI18n()
  const placeholderLabel = formatLibraryMediaTypeLabel(item.media_type, l)
  const progressLabel = formatScanItemCardProgressLabel(item)
  const progressText = formatScanItemCardSummary(item)
  const progressPercent = Math.max(0, Math.min(100, getScanItemCardProgressPercent(item)))
  const subtitle = formatLibraryScanItemSubtitle(item)

  return (
    <div aria-live="polite" className="library-detail-tile library-detail-tile--scanning">
      <LibraryDetailTileArtwork
        alt={l('{{title}} poster', { title: item.title })}
        placeholderLabel={placeholderLabel}
        src={item.poster_path}
      >
        <div className="library-detail-tile__sync">
          <div className="library-detail-tile__sync-row">
            <span>{progressLabel}</span>
            <strong>{progressPercent}%</strong>
          </div>
          <div aria-hidden="true" className="library-detail-tile__sync-track">
            <span style={{ width: `${progressPercent}%` }} />
          </div>
        </div>
      </LibraryDetailTileArtwork>
      <div className="library-detail-tile__copy">
        <strong title={item.title}>{item.title}</strong>
        <span title={progressText}>{subtitle ?? progressText}</span>
      </div>
    </div>
  )
}

const LibraryDetailTileSkeleton = ({ placeholderLabel }: { placeholderLabel: string }) => (
  <div aria-hidden="true" className="library-detail-tile library-detail-tile--loading">
    <div className="library-detail-tile__poster">
      <div className="library-detail-tile__placeholder library-detail-tile__placeholder--loading skeleton-shimmer">
        <span>{placeholderLabel}</span>
      </div>
    </div>
    <div className="library-detail-tile__copy">
      <span className="library-detail-tile__line library-detail-tile__line--title skeleton-shimmer" />
      <span className="library-detail-tile__line library-detail-tile__line--meta skeleton-shimmer" />
    </div>
  </div>
)

const MediaSection = ({
  items,
  scanItems,
  title,
}: {
  items: MediaItem[]
  scanItems: ScanRuntimeItem[]
  title: string
}) => {
  if (items.length === 0 && scanItems.length === 0) {
    return null
  }

  return (
    <section className="catalog-block library-detail-section">
      <div className="catalog-block__header library-detail-section__header">
        <div className="catalog-block__title-row">
          <h3>{title}</h3>
        </div>
      </div>

      <div className="media-grid library-detail-section__grid">
        {scanItems.map((item) => (
          <LibraryDetailScanTile item={item} key={`scan-${item.item_key}`} />
        ))}
        {items.map((item) => (
          <LibraryDetailMediaTile item={item} key={item.id} />
        ))}
      </div>
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
    <section aria-hidden="true" className="catalog-block library-detail-section">
      <div className="catalog-block__header library-detail-section__header">
        <div className="catalog-block__title-row">
          <h3>{title}</h3>
        </div>
      </div>

      <div className="media-grid library-detail-section__grid">
        {MEDIA_SECTION_SKELETON_KEYS.slice(0, MEDIA_SECTION_SKELETON_COUNT).map((key) => (
          <LibraryDetailTileSkeleton key={`${title}-${key}`} placeholderLabel={placeholderLabel} />
        ))}
      </div>
    </section>
  )
}

export const LibraryPage = () => {
  const { l } = useI18n()
  const navigate = useNavigate()
  const params = useParams()
  const { currentUser, scanRuntimeByLibrary } = useOutletContext<AppShellOutletContext>()
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

  const currentLibrary = libraryQuery.data
  const currentScanRuntime = Number.isFinite(libraryId)
    ? getLibraryScanRuntime(scanRuntimeByLibrary, libraryId)
    : null
  const mediaItems = mediaItemsQuery.data?.items ?? []
  const currentScan = getEffectiveScanJob(currentLibrary?.last_scan, currentScanRuntime)
  const hasFailedScan = hasFailedLibraryScan(currentLibrary?.last_scan, currentScanRuntime)
  const scanItems = shouldShowScanPlaceholder(currentLibrary?.last_scan, currentScanRuntime)
    ? getScanRuntimeItems(currentScanRuntime)
    : []
  const visibleScanItems = filterCompletedScanItemsWithSavedMedia(
    scanItems.filter((item) => getLibraryScanSection(item) !== null),
    mediaItems,
  )
  const visibleMediaItems = filterLibraryMediaItemsForScanRuntime(mediaItems, visibleScanItems)
  const movieItems = visibleMediaItems.filter((item) => getLibraryMediaSection(item) === 'movies')
  const seriesItems = visibleMediaItems.filter((item) => getLibraryMediaSection(item) === 'series')
  const otherItems = visibleMediaItems.filter((item) => getLibraryMediaSection(item) === 'other')
  const movieScanItems = visibleScanItems.filter((item) => getLibraryScanSection(item) === 'movies')
  const seriesScanItems = visibleScanItems.filter(
    (item) => getLibraryScanSection(item) === 'series',
  )
  const otherScanItems = visibleScanItems.filter((item) => getLibraryScanSection(item) === 'other')
  const shouldShowMediaSkeleton =
    mediaItemsQuery.isLoading && mediaItems.length === 0 && visibleScanItems.length === 0
  const isScanning = isLibraryScanActive(currentScan, currentScanRuntime)
  const scanStatusCopy = hasFailedScan
    ? formatFailedScanCopy(currentLibrary?.last_scan, currentScanRuntime)
    : isScanning
      ? formatScanJobStatusCopy(currentLibrary?.last_scan, currentScanRuntime)
      : null
  const scanProgressPercent =
    isScanning || hasFailedScan
      ? getScanJobProgressPercent(currentLibrary?.last_scan, currentScanRuntime)
      : 0
  const headerItemCount = mediaItemsQuery.data
    ? visibleMediaItems.length + visibleScanItems.length
    : (currentLibrary?.media_count ?? null)
  const handleBack = () => {
    const historyIndex =
      typeof window !== 'undefined' && typeof window.history.state?.idx === 'number'
        ? window.history.state.idx
        : 0

    if (historyIndex > 0) {
      navigate(-1)
      return
    }

    navigate('/libraries')
  }

  if (!Number.isFinite(libraryId)) {
    return (
      <HomeDashboardShell ariaLabel={l('Library')} currentUser={currentUser}>
        <div className="home-dashboard__content home-dashboard__content--library-detail">
          <p className="callout callout--danger">{l('Invalid library id.')}</p>
        </div>
      </HomeDashboardShell>
    )
  }

  return (
    <HomeDashboardShell
      ariaLabel={currentLibrary?.name ?? l('Library')}
      currentUser={currentUser}
      shellClassName="home-shell--dense-content"
    >
      <div className="home-dashboard__content home-dashboard__content--library-detail">
        <DashboardPageHeader className="library-detail-header">
          <button
            aria-label={l('Back')}
            className="home-dashboard-page-header__back"
            onClick={handleBack}
            type="button"
          >
            <HomeIcon name="arrowLeft" />
          </button>
          <h2>{currentLibrary?.name ?? l('Loading…')}</h2>
          {headerItemCount !== null ? (
            <span className="home-dashboard-page-header__meta">
              {l('{{count}} items', { count: headerItemCount })}
            </span>
          ) : null}
        </DashboardPageHeader>

        {scanStatusCopy ? (
          <section
            className={
              hasFailedScan
                ? 'library-detail-scan library-detail-scan--failed'
                : 'library-detail-scan'
            }
            role="status"
          >
            <div className="library-detail-scan__row">
              <span>{hasFailedScan ? l('Recent scan failed') : l('Scanning library')}</span>
              <strong>{hasFailedScan ? l('failed') : `${scanProgressPercent}%`}</strong>
            </div>
            <p>{scanStatusCopy}</p>
            {!hasFailedScan ? (
              <div aria-hidden="true" className="library-detail-scan__track">
                <span
                  className="library-detail-scan__fill"
                  style={{ width: `${scanProgressPercent}%` }}
                />
              </div>
            ) : null}
          </section>
        ) : null}

        {libraryQuery.isError ? (
          <p className="callout callout--danger">
            {libraryQuery.error instanceof Error
              ? libraryQuery.error.message
              : l('Failed to load library')}
          </p>
        ) : null}

        {hasFailedScan ? (
          <p className="callout callout--danger">
            {l('The most recent scan failed.')}{' '}
            {formatFailedScanCopy(currentLibrary?.last_scan, currentScanRuntime)}.{' '}
            {l('Existing items are still available, and an admin can trigger another scan later.')}
          </p>
        ) : null}

        <section className="catalog-shell library-detail-catalog">
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
          visibleScanItems.length === 0 &&
          !isScanning ? (
            <EmptyState
              description={l('This library does not have any visible items yet.')}
              title={l('No items available yet')}
            />
          ) : null}

          {shouldShowMediaSkeleton ? (
            <div className="catalog-stack library-detail-stack">
              <MediaSectionSkeleton placeholderLabel={l('Movies')} title={l('Movies')} />
              <MediaSectionSkeleton placeholderLabel={l('Series')} title={l('Series')} />
              <MediaSectionSkeleton placeholderLabel={l('Other')} title={l('Other')} />
            </div>
          ) : null}

          {!shouldShowMediaSkeleton && (mediaItems.length > 0 || visibleScanItems.length > 0) ? (
            <div className="catalog-stack library-detail-stack">
              <MediaSection items={movieItems} scanItems={movieScanItems} title={l('Movies')} />
              <MediaSection items={seriesItems} scanItems={seriesScanItems} title={l('Series')} />
              <MediaSection items={otherItems} scanItems={otherScanItems} title={l('Other')} />
            </div>
          ) : null}
        </section>
      </div>
    </HomeDashboardShell>
  )
}
