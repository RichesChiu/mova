import { Link } from 'react-router-dom'
import {
  ContinueWatchingCard,
  type ContinueWatchingCardData,
  ContinueWatchingCardSkeleton,
} from '../../../components/continue-watching-card'
import { useI18n } from '../../../i18n'
import {
  getVisibleHomeContinueWatching,
  shouldRenderHomeContinueWatching,
  shouldShowAllHomeContinueWatching,
} from '../../../lib/home-sections'

interface ContinueWatchingSectionProps {
  errorMessage: string | null
  isLoading: boolean
  items: ContinueWatchingCardData[]
}

export const ContinueWatchingSection = ({
  errorMessage,
  isLoading,
  items,
}: ContinueWatchingSectionProps) => {
  const { l } = useI18n()
  const shouldShowSkeleton = isLoading && items.length === 0
  const visibleItems = getVisibleHomeContinueWatching(items)

  if (
    !shouldRenderHomeContinueWatching({
      hasError: Boolean(errorMessage),
      isLoading,
      itemCount: items.length,
    })
  ) {
    return null
  }

  return (
    <section className="catalog-block continue-watching-section">
      <div className="catalog-block__header">
        <div className="catalog-block__title-row">
          <h3>{l('Continue Watching')}</h3>
          {shouldShowAllHomeContinueWatching(items.length) ? (
            <Link className="catalog-block__title-action" to="/continue">
              {l('View all')}
            </Link>
          ) : null}
        </div>
      </div>

      {errorMessage ? <p className="callout callout--danger">{errorMessage}</p> : null}

      {shouldShowSkeleton ? (
        <div className="continue-watching-section__grid">
          <ContinueWatchingCardSkeleton label={l('Movies')} />
          <ContinueWatchingCardSkeleton label="S01 E03" />
          <ContinueWatchingCardSkeleton label="S02 E01" />
          <ContinueWatchingCardSkeleton label={l('Movies')} />
        </div>
      ) : null}

      {items.length > 0 ? (
        <div className="continue-watching-section__grid">
          {visibleItems.map((item) => (
            <ContinueWatchingCard item={item} key={item.id} />
          ))}
        </div>
      ) : null}
    </section>
  )
}
