import { useState } from 'react'
import { Link } from 'react-router-dom'
import type { MediaItem, RecentlyAddedLibraryMediaItems } from '../../../api/types'
import { EmptyState } from '../../../components/empty-state'
import { HoverTooltip } from '../../../components/hover-tooltip'
import { useI18n } from '../../../i18n'
import { mediaItemPrimaryPath } from '../../../lib/media-routes'
import { formatLibraryMediaTypeLabel } from '../../../lib/media-type-label'

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
  'poster-g',
  'poster-h',
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
        <div className="recently-added-artwork__placeholder">
          <span>{placeholderLabel}</span>
        </div>
      )}
    </div>
  )
}

const RecentlyAddedMediaCard = ({ item }: { item: MediaItem }) => {
  const { l } = useI18n()
  const title = item.title.trim() || item.source_title.trim() || l('Untitled')
  const mediaTypeLabel = formatLibraryMediaTypeLabel(item.media_type, l)
  const metaLabel = item.year ? `${mediaTypeLabel} · ${item.year}` : mediaTypeLabel
  const overview = item.overview?.trim() ?? ''

  return (
    <Link className="recently-added-card" to={mediaItemPrimaryPath(item)}>
      <RecentlyAddedArtwork
        alt={l('{{title}} poster', { title })}
        className="recently-added-card__artwork"
        placeholderLabel={mediaTypeLabel}
        src={item.poster_path}
      />
      <div className="recently-added-card__body">
        <span>{metaLabel}</span>
        <strong title={title}>{title}</strong>
        {overview ? <p title={overview}>{overview}</p> : null}
      </div>
    </Link>
  )
}

const RecentlyAddedRowSkeleton = ({ index }: { index: number }) => {
  return (
    <section aria-hidden="true" className="recently-added-row">
      <div className="recently-added-row__header">
        <span className="recently-added-row__line recently-added-row__line--title skeleton-shimmer" />
      </div>
      <div className="recently-added-row__cards">
        {RECENTLY_ADDED_POSTER_SKELETON_KEYS.slice(0, index === 0 ? 8 : 4).map((key) => (
          <span className="recently-added-card recently-added-card--loading" key={key}>
            <span className="recently-added-card__artwork skeleton-shimmer" />
            <span className="recently-added-card__body">
              <span className="recently-added-card__line recently-added-card__line--title skeleton-shimmer" />
              <span className="recently-added-card__line recently-added-card__line--meta skeleton-shimmer" />
              <span className="recently-added-card__line recently-added-card__line--copy skeleton-shimmer" />
            </span>
          </span>
        ))}
      </div>
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
          <h3>{l('Recently Added')}</h3>
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
        <EmptyState
          description={l('Newly scanned media will appear here.')}
          title={l('No recently added media yet.')}
        />
      ) : null}

      {groups.length > 0 ? (
        <div className="recently-added-list">
          {groups.map((group) => {
            const sourceLabel = l('From “{{name}}” library', { name: group.library.name })

            return (
              <section className="recently-added-row" key={group.library.id}>
                <div className="recently-added-row__header">
                  <Link
                    aria-label={sourceLabel}
                    className="recently-added-row__source"
                    to={`/libraries/${group.library.id}`}
                  >
                    <span>{l('From')}</span>
                    <HoverTooltip
                      className="recently-added-row__source-tooltip"
                      content={group.library.name}
                    >
                      <span className="recently-added-row__source-name">
                        {l('“{{name}}”', { name: group.library.name })}
                      </span>
                    </HoverTooltip>
                    <span>{l('library')}</span>
                  </Link>
                </div>

                <div className="recently-added-row__cards">
                  {group.items.map((item) => (
                    <RecentlyAddedMediaCard item={item} key={item.id} />
                  ))}
                </div>
              </section>
            )
          })}
        </div>
      ) : null}
    </section>
  )
}
