import { useQuery } from '@tanstack/react-query'
import { useParams } from 'react-router-dom'
import { getLibrary, listLibraryMediaItems } from '../../api/client'
import type { MediaItem } from '../../api/types'
import { MediaCard } from '../../components/media-card'

const PAGE_SIZE = 500

const MediaSection = ({ items, title }: { items: MediaItem[]; title: string }) => {
  return (
    <section className="catalog-block">
      <div className="catalog-block__header">
        <h3>{title}</h3>
      </div>

      {items.length === 0 ? (
        <div className="catalog-block__empty">
          <p className="muted">No items in this section yet.</p>
        </div>
      ) : (
        <div className="media-grid">
          {items.map((item) => (
            <MediaCard item={item} key={item.id} />
          ))}
        </div>
      )}
    </section>
  )
}

export const LibraryPage = () => {
  const params = useParams()
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

  const mediaItemsQuery = useQuery({
    enabled: Number.isFinite(libraryId),
    queryKey: ['library-media', libraryId, 'full'],
    queryFn: () =>
      listLibraryMediaItems(libraryId, {
        page: 1,
        pageSize: PAGE_SIZE,
      }),
  })

  if (!Number.isFinite(libraryId)) {
    return <p className="callout callout--danger">Invalid library id.</p>
  }

  const currentLibrary = libraryQuery.data
  const mediaItems = mediaItemsQuery.data?.items ?? []
  const libraryDescription =
    currentLibrary?.description?.trim() || 'No library description provided yet.'
  const movieItems = mediaItems.filter((item) => item.media_type === 'movie')
  const seriesItems = mediaItems.filter((item) => item.media_type === 'series')
  const isMixedLibrary = currentLibrary?.library_type === 'mixed'

  return (
    <div className="page-stack">
      <section className="library-hero library-hero--compact">
        <div className="library-hero__content">
          <div className="library-hero__copy">
            <p className="eyebrow">Library</p>
            <h2>{currentLibrary?.name ?? 'Loading…'}</h2>
            <p className="muted">
              {isMixedLibrary
                ? 'Mixed library: movies and series are organized automatically.'
                : 'Browse this library content below.'}
            </p>
          </div>

          <div className="library-hero__meta">
            <div className="hero-stat">
              <span className="hero-stat__label">Library Name</span>
              <strong>{currentLibrary?.name ?? '—'}</strong>
            </div>
            <div className="hero-stat">
              <span className="hero-stat__label">Library Type</span>
              <strong>{currentLibrary?.library_type ?? '—'}</strong>
            </div>
            <div className="hero-stat">
              <span className="hero-stat__label">Resources</span>
              <strong>{currentLibrary?.media_count ?? mediaItemsQuery.data?.total ?? 0}</strong>
            </div>
          </div>

          <div className="library-hero__actions">
            <div className="hero-note">
              <strong>Description</strong>
              <span>{libraryDescription}</span>
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

      <section className="catalog-shell">
        {mediaItemsQuery.isLoading ? <p className="muted">Loading media items…</p> : null}

        {mediaItemsQuery.isError ? (
          <p className="callout callout--danger">
            {mediaItemsQuery.error instanceof Error
              ? mediaItemsQuery.error.message
              : 'Failed to load media items'}
          </p>
        ) : null}

        {mediaItemsQuery.data && mediaItems.length === 0 ? (
          <section className="empty-panel">
            <h3>No items available yet</h3>
            <p className="muted">这个媒体库当前还没有可展示的内容。</p>
          </section>
        ) : null}

        {mediaItems.length > 0 ? (
          isMixedLibrary ? (
            <div className="catalog-stack">
              <MediaSection items={movieItems} title="Movies" />
              <MediaSection items={seriesItems} title="Series" />
            </div>
          ) : (
            <div className="catalog-stack">
              <MediaSection
                items={mediaItems}
                title={currentLibrary?.library_type === 'series' ? 'Series' : 'Movies'}
              />
            </div>
          )
        ) : null}
      </section>
    </div>
  )
}
