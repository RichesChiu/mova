import { EpisodeCard, EpisodeCardSkeleton } from '../../../components/episode-card'
import { ScrollableRail } from '../../../components/scrollable-rail'
import { SectionHelp } from '../../../components/section-help'
import type { ContinueWatchingCardData } from '../types'

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
  const shouldShowSkeleton = isLoading && items.length === 0

  // 没有继续观看数据时直接收起整个模块，首页只保留真正有内容的块。
  if (!isLoading && !errorMessage && items.length === 0) {
    return null
  }

  return (
    <section className="catalog-block continue-watching-section">
      <div className="catalog-block__header">
        <div className="catalog-block__title-row">
          <h3>Continue Watching</h3>
          <SectionHelp
            detail="Resume unfinished movies and episodes. The latest in-progress item stays here."
            placement="bottom"
            title="About continue watching"
          />
        </div>
        <span className="counter-badge">{items.length}</span>
      </div>

      {isLoading ? <p className="muted">Loading…</p> : null}
      {errorMessage ? <p className="callout callout--danger">{errorMessage}</p> : null}

      {shouldShowSkeleton ? (
        <ScrollableRail hint="Scroll horizontally.">
          <EpisodeCardSkeleton metaLabel="MOVIE" placeholderLabel="MOVIE" />
          <EpisodeCardSkeleton metaLabel="S01 · E03" placeholderLabel="1-3" />
          <EpisodeCardSkeleton metaLabel="S02 · E01" placeholderLabel="2-1" />
          <EpisodeCardSkeleton metaLabel="MOVIE" placeholderLabel="MOVIE" />
        </ScrollableRail>
      ) : null}

      {items.length > 0 ? (
        <ScrollableRail hint="Scroll horizontally.">
          {items.map((item) => (
            <EpisodeCard
              artworkAlt={item.artworkAlt}
              artworkSrc={item.artworkSrc}
              description={item.description}
              href={item.href}
              key={item.id}
              metaLabel={item.metaLabel}
              placeholderLabel={item.placeholderLabel}
              progressPercent={item.progressPercent}
              status={item.status}
              title={item.title}
            />
          ))}
        </ScrollableRail>
      ) : null}
    </section>
  )
}
