import { useQuery } from '@tanstack/react-query'
import { useCallback, useEffect, useState } from 'react'
import { Link, useOutletContext, useParams } from 'react-router-dom'
import { getLibrary, listLibraryMediaItems } from '../../api/client'
import type {
  LibraryMediaCategory,
  LibraryMediaSortBy,
  MediaItem,
  SortOrder,
} from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import type { ScanRuntimeItem } from '../../components/app-shell/scan-runtime'
import {
  formatScanItemCardProgressLabel,
  formatScanItemCardSummary,
  formatScanItemMeta,
  formatScanJobStatusCopy,
  getEffectiveScanJob,
  getLibraryScanRuntime,
  getScanItemCardProgressPercent,
  getScanJobProgressPercent,
  getScanRuntimeItems,
  isLibraryScanActive,
  shouldShowScanPlaceholder,
} from '../../components/app-shell/scan-runtime'
import { EmptyState } from '../../components/empty-state'
import { MediaRatingBadges } from '../../components/media-rating-badges'
import { useI18n } from '../../i18n'
import { libraryDetailReturnPath, mediaItemPrimaryPath } from '../../lib/media-routes'
import { formatLibraryMediaTypeLabel } from '../../lib/media-type-label'
import { DashboardPageHeader } from '../home-page/dashboard-page-header'
import { HomeDashboardShell } from '../home-page/home-dashboard-shell'
import { HomeIcon } from '../home-page/home-icons'
import { LibraryDetailTileArtwork } from './library-detail-tile-artwork'
import { LibraryFilterPopover, LibrarySortMenu } from './library-toolbar-popovers'

const PAGE_SIZE = 60
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
        <div className="media-card-title-row">
          <strong className="library-detail-tile__title media-card-title-row__title" title={title}>
            {title}
          </strong>
          <MediaRatingBadges ratings={item.ratings} />
        </div>
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
        <strong className="library-detail-tile__title" title={item.title}>
          {item.title}
        </strong>
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

const MediaSection = ({ items }: { items: MediaItem[] }) => {
  if (items.length === 0) {
    return null
  }

  return (
    <section className="catalog-block library-detail-section">
      <div className="media-grid library-detail-section__grid">
        {items.map((item) => (
          <LibraryDetailMediaTile item={item} key={item.id} />
        ))}
      </div>
    </section>
  )
}

const ScanSection = ({ items }: { items: ScanRuntimeItem[] }) => {
  const { l } = useI18n()

  if (items.length === 0) {
    return null
  }

  return (
    <section className="catalog-block library-detail-section library-detail-section--scanning">
      <div className="catalog-block__header library-detail-section__header">
        <div className="catalog-block__title-row">
          <h3>{l('Scanning items')}</h3>
        </div>
      </div>
      <div className="media-grid library-detail-section__grid">
        {items.map((item) => (
          <LibraryDetailScanTile item={item} key={item.item_key} />
        ))}
      </div>
    </section>
  )
}

const MediaSectionSkeleton = ({ placeholderLabel }: { placeholderLabel: string }) => {
  return (
    <section aria-hidden="true" className="catalog-block library-detail-section">
      <div className="media-grid library-detail-section__grid">
        {MEDIA_SECTION_SKELETON_KEYS.slice(0, MEDIA_SECTION_SKELETON_COUNT).map((key) => (
          <LibraryDetailTileSkeleton key={key} placeholderLabel={placeholderLabel} />
        ))}
      </div>
    </section>
  )
}

export const LibraryPage = () => {
  const { l } = useI18n()
  const params = useParams()
  const { currentUser, scanRuntimeByLibrary } = useOutletContext<AppShellOutletContext>()
  const libraryId = Number(params.libraryId)
  const [page, setPage] = useState(1)
  const [queryFilter, setQueryFilter] = useState('')
  const [yearFilter, setYearFilter] = useState<number | undefined>()
  const [category, setCategory] = useState<LibraryMediaCategory>('all')
  const [sortBy, setSortBy] = useState<LibraryMediaSortBy>('title')
  const [sortOrder, setSortOrder] = useState<SortOrder>('asc')
  const [openToolbarPopover, setOpenToolbarPopover] = useState<'filter' | 'sort' | null>(null)

  useEffect(() => {
    if (!Number.isFinite(libraryId)) {
      return
    }

    setPage(1)
    setQueryFilter('')
    setYearFilter(undefined)
    setCategory('all')
    setOpenToolbarPopover(null)
  }, [libraryId])

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
    queryKey: [
      'library-media',
      libraryId,
      page,
      queryFilter,
      yearFilter ?? null,
      category,
      sortBy,
      sortOrder,
    ],
    queryFn: () =>
      listLibraryMediaItems(libraryId, {
        page,
        pageSize: PAGE_SIZE,
        category: category === 'all' ? undefined : category,
        query: queryFilter || undefined,
        sortBy,
        sortOrder,
        year: yearFilter,
      }),
    refetchInterval: scanStatus === 'pending' || scanStatus === 'running' ? 3_000 : false,
  })

  const currentLibrary = libraryQuery.data
  const currentScanRuntime = Number.isFinite(libraryId)
    ? getLibraryScanRuntime(scanRuntimeByLibrary, libraryId)
    : null
  const mediaItems = mediaItemsQuery.data?.items ?? []
  const currentScan = getEffectiveScanJob(currentLibrary?.last_scan, currentScanRuntime)
  const scanItems =
    page === 1 && shouldShowScanPlaceholder(currentLibrary?.last_scan, currentScanRuntime)
      ? getScanRuntimeItems(currentScanRuntime).filter((item) => item.stage !== 'completed')
      : []
  const shouldShowMediaSkeleton = mediaItemsQuery.isLoading && mediaItems.length === 0
  const isScanning = isLibraryScanActive(currentScan, currentScanRuntime)
  const scanStatusCopy = isScanning
    ? formatScanJobStatusCopy(currentLibrary?.last_scan, currentScanRuntime)
    : null
  const scanProgressPercent = isScanning
    ? getScanJobProgressPercent(currentLibrary?.last_scan, currentScanRuntime)
    : 0
  const headerItemCount = currentLibrary?.media_count ?? null
  const totalPages = mediaItemsQuery.data
    ? Math.max(1, Math.ceil(mediaItemsQuery.data.total / mediaItemsQuery.data.page_size))
    : 1
  const hasActiveFilters = queryFilter.length > 0 || yearFilter !== undefined || category !== 'all'
  const activeFilterCount =
    Number(queryFilter.length > 0) + Number(yearFilter !== undefined) + Number(category !== 'all')
  const resultTitle =
    category === 'movie'
      ? l('Movies')
      : category === 'series'
        ? l('Series')
        : category === 'needs_review'
          ? l('Needs review')
          : l('All media')

  const updateQueryFilter = useCallback((query: string) => {
    setQueryFilter(query)
    setPage(1)
  }, [])

  const updateYearFilter = useCallback((year?: number) => {
    setYearFilter(year)
    setPage(1)
  }, [])

  const updateCategory = useCallback((nextCategory: LibraryMediaCategory) => {
    setCategory(nextCategory)
    setPage(1)
  }, [])

  const updateSort = useCallback((nextSortBy: LibraryMediaSortBy, nextSortOrder: SortOrder) => {
    setSortBy(nextSortBy)
    setSortOrder(nextSortOrder)
    setPage(1)
  }, [])

  const resetFilters = useCallback(() => {
    setQueryFilter('')
    setYearFilter(undefined)
    setCategory('all')
    setPage(1)
  }, [])
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
          <Link
            aria-label={l('Back')}
            className="home-dashboard-page-header__back"
            to={libraryDetailReturnPath()}
          >
            <HomeIcon name="arrowLeft" />
          </Link>
          <h2>{currentLibrary?.name ?? l('Loading…')}</h2>
          {headerItemCount !== null ? (
            <span className="home-dashboard-page-header__meta">
              {l('{{count}} items', { count: headerItemCount })}
            </span>
          ) : null}
        </DashboardPageHeader>

        {scanStatusCopy ? (
          <section className="library-detail-scan" role="status">
            <div className="library-detail-scan__row">
              <span>{l('Scanning library')}</span>
              <strong>{scanProgressPercent}%</strong>
            </div>
            <p>{scanStatusCopy}</p>
            <div aria-hidden="true" className="library-detail-scan__track">
              <span
                className="library-detail-scan__fill"
                style={{ width: `${scanProgressPercent}%` }}
              />
            </div>
          </section>
        ) : null}

        {libraryQuery.isError ? (
          <p className="callout callout--danger">
            {libraryQuery.error instanceof Error
              ? libraryQuery.error.message
              : l('Failed to load library')}
          </p>
        ) : null}

        <section className="catalog-shell library-detail-catalog">
          <div className="library-detail-results-header">
            <div className="library-detail-results-header__title">
              <h3>{resultTitle}</h3>
              {mediaItemsQuery.data ? (
                <span>{l('{{count}} results', { count: mediaItemsQuery.data.total })}</span>
              ) : null}
            </div>

            <div className="library-detail-results-header__actions">
              <LibrarySortMenu
                isOpen={openToolbarPopover === 'sort'}
                onChange={updateSort}
                onOpenChange={(isOpen) => setOpenToolbarPopover(isOpen ? 'sort' : null)}
                sortBy={sortBy}
                sortOrder={sortOrder}
              />
              <LibraryFilterPopover
                activeFilterCount={activeFilterCount}
                isOpen={openToolbarPopover === 'filter'}
                onCategoryChange={updateCategory}
                onOpenChange={(isOpen) => setOpenToolbarPopover(isOpen ? 'filter' : null)}
                onQueryChange={updateQueryFilter}
                onReset={resetFilters}
                onYearChange={updateYearFilter}
                value={{ category, query: queryFilter, year: yearFilter }}
              />
            </div>
          </div>

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
          !isScanning ? (
            hasActiveFilters ? (
              <EmptyState
                description={l('Try a different media type, title, or year, or clear the filters.')}
                title={l('No items match these filters')}
              />
            ) : (
              <EmptyState
                description={l('This library does not have any visible items yet.')}
                title={l('No items available yet')}
              />
            )
          ) : null}

          {shouldShowMediaSkeleton ? (
            <div className="catalog-stack library-detail-stack">
              <MediaSectionSkeleton placeholderLabel={resultTitle} />
            </div>
          ) : null}

          {!shouldShowMediaSkeleton && (mediaItems.length > 0 || scanItems.length > 0) ? (
            <div className="catalog-stack library-detail-stack">
              <ScanSection items={scanItems} />
              <MediaSection items={mediaItems} />
            </div>
          ) : null}

          {mediaItemsQuery.data && totalPages > 1 ? (
            <nav aria-label={l('Library pages')} className="library-detail-pagination">
              <button
                className="button"
                disabled={page <= 1 || mediaItemsQuery.isFetching}
                onClick={() => setPage((current) => Math.max(1, current - 1))}
                type="button"
              >
                {l('Previous')}
              </button>
              <span>{l('Page {{page}} of {{total}}', { page, total: totalPages })}</span>
              <button
                className="button"
                disabled={page >= totalPages || mediaItemsQuery.isFetching}
                onClick={() => setPage((current) => Math.min(totalPages, current + 1))}
                type="button"
              >
                {l('Next')}
              </button>
            </nav>
          ) : null}
        </section>
      </div>
    </HomeDashboardShell>
  )
}
