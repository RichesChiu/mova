import { useState } from 'react'
import { Link } from 'react-router-dom'
import type { MediaItem, RecentlyAddedLibraryMediaItems } from '../../../api/types'
import { useI18n } from '../../../i18n'
import { mediaItemPrimaryPath } from '../../../lib/media-routes'
import { formatLibraryMediaTypeLabel } from '../../../lib/media-type-label'
import { getLibraryArtworkSrc } from '../library-artwork'

interface LibraryContentSectionsProps {
  errorMessage: string | null
  groups: RecentlyAddedLibraryMediaItems[]
  isLoading: boolean
}

const RECENTLY_ADDED_SKELETON_KEYS = ['library-a', 'library-b', 'library-c'] as const
const RECENTLY_ADDED_POSTER_SKELETON_KEYS = [
  'poster-a',
  'poster-b',
  'poster-c',
  'poster-d',
  'poster-e',
  'poster-f',
] as const

const RecentlyAddedArtwork = ({
  alt,
  className,
  placeholderLabel,
  src,
}: {
  alt: string
  className: string
  placeholderLabel: string
  src: string | null
}) => {
  const [failedSrc, setFailedSrc] = useState<string | null>(null)
  const shouldRenderImage = Boolean(src) && failedSrc !== src

  return (
    <div className={className}>
      {shouldRenderImage ? (
        <img
          alt={alt}
          loading="lazy"
          onError={() => {
            setFailedSrc(src)
          }}
          src={src ?? undefined}
        />
      ) : (
        <div className="recently-added-poster__placeholder">
          <span>{placeholderLabel}</span>
        </div>
      )}
    </div>
  )
}

const RecentlyAddedPosterCard = ({ item }: { item: MediaItem }) => {
  const { l } = useI18n()
  const title = item.title.trim() || item.source_title.trim() || l('Untitled')
  const mediaTypeLabel = formatLibraryMediaTypeLabel(item.media_type, l)

  return (
    <Link className="recently-added-poster" to={mediaItemPrimaryPath(item)}>
      <RecentlyAddedArtwork
        alt={`${title} poster`}
        className="recently-added-poster__artwork"
        placeholderLabel={mediaTypeLabel}
        src={item.poster_path}
      />
      <div className="recently-added-poster__meta">
        <strong title={title}>{title}</strong>
        <span>{item.year ? `${mediaTypeLabel} · ${item.year}` : mediaTypeLabel}</span>
      </div>
    </Link>
  )
}

const RecentlyAddedRowSkeleton = ({ index }: { index: number }) => {
  return (
    <section aria-hidden="true" className="recently-added-row">
      <div className="recently-added-row__summary">
        <span className="recently-added-row__cover skeleton-shimmer" />
        <div className="recently-added-row__copy">
          <span className="recently-added-row__line recently-added-row__line--title skeleton-shimmer" />
          <span className="recently-added-row__line recently-added-row__line--meta skeleton-shimmer" />
        </div>
      </div>
      <div className="recently-added-row__posters">
        {RECENTLY_ADDED_POSTER_SKELETON_KEYS.slice(0, index === 0 ? 6 : 5).map((key) => (
          <span className="recently-added-poster recently-added-poster--loading" key={key}>
            <span className="recently-added-poster__artwork skeleton-shimmer" />
          </span>
        ))}
      </div>
      <span className="button recently-added-row__action skeleton-shimmer" />
    </section>
  )
}

export const LibraryContentSections = ({
  errorMessage,
  groups,
  isLoading,
}: LibraryContentSectionsProps) => {
  const { l } = useI18n()
  const shouldShowSkeleton = isLoading && groups.length === 0

  return (
    <section className="catalog-block library-content-sections__block">
      <div className="catalog-block__header">
        <div className="catalog-block__title-row">
          <h3>{l('Recently Added by Library')}</h3>
        </div>
      </div>

      {errorMessage ? <p className="callout callout--danger">{errorMessage}</p> : null}

      {shouldShowSkeleton ? (
        <div className="recently-added-list">
          {RECENTLY_ADDED_SKELETON_KEYS.map((key, index) => (
            <RecentlyAddedRowSkeleton index={index} key={key} />
          ))}
        </div>
      ) : null}

      {!isLoading && !errorMessage && groups.length === 0 ? (
        <div className="catalog-block__empty">
          <p className="muted">{l('No recently added media yet.')}</p>
        </div>
      ) : null}

      {groups.length > 0 ? (
        <div className="recently-added-list">
          {groups.map((group) => {
            const libraryArtworkSrc = getLibraryArtworkSrc(group.items)

            return (
              <section className="recently-added-row" key={group.library.id}>
                <div className="recently-added-row__summary">
                  <RecentlyAddedArtwork
                    alt={`${group.library.name} artwork`}
                    className="recently-added-row__cover"
                    placeholderLabel={l('Library')}
                    src={libraryArtworkSrc}
                  />
                  <div className="recently-added-row__copy">
                    <strong>{group.library.name}</strong>
                    <span>{l('{{count}} recently added', { count: group.items.length })}</span>
                  </div>
                </div>

                <div className="recently-added-row__posters">
                  {group.items.map((item) => (
                    <RecentlyAddedPosterCard item={item} key={item.id} />
                  ))}
                </div>

                <Link
                  className="button recently-added-row__action"
                  to={`/libraries/${group.library.id}`}
                >
                  {l('View Library')}
                </Link>
              </section>
            )
          })}
        </div>
      ) : null}
    </section>
  )
}
