import { Link } from 'react-router-dom'
import { ScrollableRail } from '../../../components/scrollable-rail'
import { useI18n } from '../../../i18n'
import { HomeIcon } from '../home-icons'
import type { ContinueWatchingCardData } from '../types'

interface ContinueWatchingSectionProps {
  errorMessage: string | null
  isLoading: boolean
  items: ContinueWatchingCardData[]
}

const HOME_CONTINUE_WATCHING_LIMIT = 6

const HomeContinueCard = ({ item }: { item: ContinueWatchingCardData }) => (
  <Link className="home-continue-card" to={item.href}>
    <div className="home-continue-card__artwork">
      {item.artworkSrc ? (
        <img alt={item.artworkAlt} loading="lazy" src={item.artworkSrc} />
      ) : (
        <span className="home-continue-card__placeholder">{item.placeholderLabel}</span>
      )}
      <span aria-hidden="true" className="home-continue-card__play">
        <HomeIcon name="play" />
      </span>
    </div>
    <div className="home-continue-card__copy">
      <strong title={item.title}>{item.title}</strong>
      {item.metaLabel ? <span>{item.metaLabel.replace(' · ', ' ')}</span> : null}
      <em>{item.progressPercent}%</em>
    </div>
    <div className="home-continue-card__progress" aria-hidden="true">
      <span style={{ width: `${item.progressPercent}%` }} />
    </div>
  </Link>
)

const HomeContinueSkeleton = ({ label }: { label: string }) => (
  <div aria-hidden="true" className="home-continue-card home-continue-card--loading">
    <div className="home-continue-card__artwork skeleton-shimmer">
      <span className="home-continue-card__placeholder">{label}</span>
    </div>
    <span className="home-continue-card__line home-continue-card__line--title skeleton-shimmer" />
    <span className="home-continue-card__line home-continue-card__line--meta skeleton-shimmer" />
  </div>
)

export const ContinueWatchingSection = ({
  errorMessage,
  isLoading,
  items,
}: ContinueWatchingSectionProps) => {
  const { l } = useI18n()
  const shouldShowSkeleton = isLoading && items.length === 0
  const visibleItems = items.slice(0, HOME_CONTINUE_WATCHING_LIMIT)
  const overflowCount = Math.max(0, items.length - visibleItems.length)

  // 没有继续观看数据时直接收起整个模块，首页只保留真正有内容的块。
  if (!isLoading && !errorMessage && items.length === 0) {
    return null
  }

  return (
    <section className="catalog-block continue-watching-section">
      <div className="catalog-block__header">
        <div className="catalog-block__title-row">
          <h3>{l('Continue Watching')}</h3>
        </div>
        {items.length > 0 ? (
          <span className="catalog-block__inline-action catalog-block__inline-action--static">
            {l('View all')}
          </span>
        ) : null}
      </div>

      {isLoading ? <p className="muted">{l('Loading…')}</p> : null}
      {errorMessage ? <p className="callout callout--danger">{errorMessage}</p> : null}

      {shouldShowSkeleton ? (
        <ScrollableRail
          hint={l('Scroll horizontally.')}
          viewportClassName="continue-watching-section__viewport"
        >
          <HomeContinueSkeleton label={l('Movies')} />
          <HomeContinueSkeleton label="S01 E03" />
          <HomeContinueSkeleton label="S02 E01" />
          <HomeContinueSkeleton label={l('Movies')} />
        </ScrollableRail>
      ) : null}

      {items.length > 0 ? (
        <ScrollableRail
          hint={l('Scroll horizontally.')}
          viewportClassName="continue-watching-section__viewport"
        >
          {visibleItems.map((item) => (
            <HomeContinueCard item={item} key={item.id} />
          ))}
          {overflowCount > 0 ? (
            <div className="continue-watching-section__overflow">+{overflowCount}</div>
          ) : null}
        </ScrollableRail>
      ) : null}
    </section>
  )
}
