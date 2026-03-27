import { EpisodeCard } from '../../../components/episode-card'
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
}: ContinueWatchingSectionProps) => (
  <section className="catalog-block continue-watching-section">
    <div className="catalog-block__header">
      <div className="catalog-block__title-row">
        <h3>Continue Watching</h3>
        <SectionHelp
          detail="Resume unfinished movies and episodes. The latest in-progress item stays here."
          title="About continue watching"
        />
      </div>
      <span className="counter-badge">{items.length}</span>
    </div>

    {isLoading ? <p className="muted">Loading…</p> : null}
    {errorMessage ? <p className="callout callout--danger">{errorMessage}</p> : null}

    {!isLoading && !errorMessage && items.length === 0 ? (
      <div className="catalog-block__empty">
        <p className="muted">Nothing here yet.</p>
      </div>
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
