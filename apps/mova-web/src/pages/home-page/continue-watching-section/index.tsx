import { EpisodeCard } from '../../../components/episode-card'
import { ScrollableRail } from '../../../components/scrollable-rail'
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
      <div>
        <h3>Continue Watching</h3>
        <p className="muted">最近观看记录会优先出现在这里。</p>
      </div>
      <span className="counter-badge">{items.length}</span>
    </div>

    {isLoading ? <p className="muted">Loading recent watching…</p> : null}
    {errorMessage ? <p className="callout callout--danger">{errorMessage}</p> : null}

    {!isLoading && !errorMessage && items.length === 0 ? (
      <div className="catalog-block__empty">
        <p className="muted">还没有最近观看内容。开始播放后，这里会自动出现记录。</p>
      </div>
    ) : null}

    {items.length > 0 ? (
      <ScrollableRail hint="Drag or click arrows to scroll continue watching items horizontally.">
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
