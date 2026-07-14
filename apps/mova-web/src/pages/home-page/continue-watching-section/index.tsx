import { Link } from 'react-router-dom'
import {
  ContinueWatchingCard,
  type ContinueWatchingCardData,
  ContinueWatchingCardSkeleton,
} from '../../../components/continue-watching-card'
import { ScrollableRail } from '../../../components/scrollable-rail'
import { useI18n } from '../../../i18n'
import { shouldRenderHomeContinueWatching } from '../../../lib/home-sections'

interface ContinueWatchingSectionProps {
  errorMessage: string | null
  isLoading: boolean
  items: ContinueWatchingCardData[]
}

const HOME_CONTINUE_WATCHING_LIMIT = 6

export const ContinueWatchingSection = ({
  errorMessage,
  isLoading,
  items,
}: ContinueWatchingSectionProps) => {
  const { l } = useI18n()
  const shouldShowSkeleton = isLoading && items.length === 0
  const visibleItems = items.slice(0, HOME_CONTINUE_WATCHING_LIMIT)
  const overflowCount = Math.max(0, items.length - visibleItems.length)

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
        </div>
        {items.length > 0 ? (
          <Link className="catalog-block__inline-action" to="/continue">
            {l('View all')}
          </Link>
        ) : null}
      </div>

      {isLoading ? <p className="muted">{l('Loading…')}</p> : null}
      {errorMessage ? <p className="callout callout--danger">{errorMessage}</p> : null}

      {shouldShowSkeleton ? (
        <ScrollableRail
          hint={l('Scroll horizontally.')}
          viewportClassName="continue-watching-section__viewport"
        >
          <ContinueWatchingCardSkeleton label={l('Movies')} />
          <ContinueWatchingCardSkeleton label="S01 E03" />
          <ContinueWatchingCardSkeleton label="S02 E01" />
          <ContinueWatchingCardSkeleton label={l('Movies')} />
        </ScrollableRail>
      ) : null}

      {items.length > 0 ? (
        <ScrollableRail
          hint={l('Scroll horizontally.')}
          viewportClassName="continue-watching-section__viewport"
        >
          {visibleItems.map((item) => (
            <ContinueWatchingCard item={item} key={item.id} />
          ))}
          {overflowCount > 0 ? (
            <div className="continue-watching-section__overflow">+{overflowCount}</div>
          ) : null}
        </ScrollableRail>
      ) : null}
    </section>
  )
}
