import type { EpisodeOutlineSeason, MediaCastMember } from '../../api/types'
import {
  formatScanItemCardSummary,
  formatScanItemMeta,
  getScanItemCardProgressPercent,
  type ScanRuntimeItem,
} from '../../components/app-shell/scan-runtime'
import { EpisodeCard, EpisodeCardSkeleton } from '../../components/episode-card'
import { ScrollableRail } from '../../components/scrollable-rail'
import { useI18n } from '../../i18n'
import { mediaItemPlayPath } from '../../lib/media-routes'
import { playbackPercent, playbackStatus } from '../../lib/playback'

const EPISODE_SKELETONS = [
  { metaLabel: 'S01 · E01', placeholderLabel: '1-1' },
  { metaLabel: 'S01 · E02', placeholderLabel: '1-2' },
  { metaLabel: 'S01 · E03', placeholderLabel: '1-3' },
  { metaLabel: 'S01 · E04', placeholderLabel: '1-4' },
] as const

const SeasonBlock = ({
  scanItems,
  season,
}: {
  scanItems: ScanRuntimeItem[]
  season: EpisodeOutlineSeason
}) => {
  const { l } = useI18n()
  const entries = [
    ...scanItems.map((item) => ({
      key: `scan-${item.item_key}`,
      order: item.episode_number ?? Number.MAX_SAFE_INTEGER,
      render: () => {
        const metaLabel = formatScanItemMeta(item)

        return (
          <EpisodeCard
            artworkAlt={l('{{title}} artwork', { title: metaLabel })}
            artworkSrc={item.poster_path}
            description={formatScanItemCardSummary(item)}
            key={`scan-${item.item_key}`}
            metaLabel={metaLabel}
            placeholderLabel={metaLabel}
            progressPercent={getScanItemCardProgressPercent(item)}
            status="progress"
            title={item.title}
          />
        )
      },
    })),
    ...season.episodes.map((episode) => ({
      key: `${season.season_number}-${episode.episode_number}`,
      order: episode.episode_number,
      render: () => {
        const index = `S${String(season.season_number).padStart(2, '0')} · E${String(episode.episode_number).padStart(2, '0')}`
        const title =
          episode.title.trim() || l('Episode {{episode}}', { episode: episode.episode_number })
        const card = (
          <EpisodeCard
            artworkAlt={l('{{title}} artwork', { title: index })}
            artworkSrc={episode.poster_path}
            description={episode.overview}
            key={`${season.season_number}-${episode.episode_number}`}
            metaLabel={index}
            placeholderLabel={index}
            progressPercent={playbackPercent(episode.playback_progress)}
            status={playbackStatus(episode.playback_progress)}
            title={title}
            {...(episode.is_available && episode.media_item_id
              ? { href: mediaItemPlayPath(episode.media_item_id) }
              : {})}
          />
        )

        return card
      },
    })),
  ].sort((left, right) => left.order - right.order)

  return (
    <article className="season-card season-card--rail">
      <ScrollableRail
        hint={l('Use horizontal scrolling or click arrows to move through episodes.')}
        resetKey={season.season_number}
      >
        {entries.map((entry) => entry.render())}
      </ScrollableRail>
    </article>
  )
}

const SeasonBlockSkeleton = () => {
  const { l } = useI18n()

  return (
    <article aria-hidden="true" className="season-card season-card--rail">
      <ScrollableRail
        hint={l('Use horizontal scrolling or click arrows to move through episodes.')}
        resetKey="loading"
      >
        {EPISODE_SKELETONS.map((episode) => (
          <EpisodeCardSkeleton
            key={episode.metaLabel}
            metaLabel={episode.metaLabel}
            placeholderLabel={episode.placeholderLabel}
          />
        ))}
      </ScrollableRail>
    </article>
  )
}

interface MediaItemEpisodesSectionProps {
  availableSeasons: EpisodeOutlineSeason[]
  error: unknown
  isLoading: boolean
  scanItems: ScanRuntimeItem[]
  selectedSeason: EpisodeOutlineSeason | undefined
}

export const MediaItemEpisodesSection = ({
  availableSeasons,
  error,
  isLoading,
  scanItems,
  selectedSeason,
}: MediaItemEpisodesSectionProps) => {
  const { l } = useI18n()
  const shouldShowSkeleton = isLoading && !error && availableSeasons.length === 0

  return (
    <section className="page-stack">
      <div className="section-heading">
        <h3>{l('Episodes')}</h3>
      </div>

      {scanItems.length > 0 ? (
        <p className="muted">
          {scanItems.length === 1
            ? l(
                '{{count}} episode is still syncing in this season. Placeholder cards stay visible until the library write completes.',
                { count: scanItems.length },
              )
            : l(
                '{{count}} episodes are still syncing in this season. Placeholder cards stay visible until the library write completes.',
                { count: scanItems.length },
              )}
        </p>
      ) : null}

      {shouldShowSkeleton ? (
        <>
          <p className="muted">{l('Loading episodes…')}</p>
          <SeasonBlockSkeleton />
        </>
      ) : null}

      {error ? (
        <p className="callout callout--danger">
          {error instanceof Error ? error.message : l('Failed to load episodes')}
        </p>
      ) : null}

      {!shouldShowSkeleton && availableSeasons.length > 0 ? (
        selectedSeason ? (
          <SeasonBlock
            key={selectedSeason.season_number}
            scanItems={scanItems}
            season={selectedSeason}
          />
        ) : null
      ) : !shouldShowSkeleton ? (
        <p className="muted">{l('No local episodes available in this series yet.')}</p>
      ) : null}
    </section>
  )
}

const castInitials = (member: MediaCastMember) =>
  member.name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? '')
    .join('')

interface MediaItemCastSectionProps {
  error: unknown
  isLoading: boolean
  members: MediaCastMember[]
}

export const MediaItemCastSection = ({ error, isLoading, members }: MediaItemCastSectionProps) => {
  const { l } = useI18n()

  if (!isLoading && !error && members.length === 0) {
    return null
  }

  return (
    <section className="page-stack">
      <div className="section-heading">
        <div className="media-item-section-title-row">
          <h3>{l('Cast')}</h3>
        </div>
        {!isLoading ? <span className="counter-badge">{members.length}</span> : null}
      </div>

      <div className="season-card season-card--rail cast-panel">
        {isLoading ? (
          <p className="muted">{l('Loading cast…')}</p>
        ) : error ? (
          <p className="muted">{l('Cast details are unavailable right now.')}</p>
        ) : members.length > 0 ? (
          <ScrollableRail
            hint={l('Use horizontal scrolling or click arrows to move through the cast list.')}
            viewportClassName="cast-panel__viewport"
          >
            {members.map((member) => (
              <article
                className="cast-card"
                key={`${member.person_id ?? member.name}-${member.sort_order}`}
              >
                <div className="cast-card__portrait">
                  {member.profile_path ? (
                    <img alt={member.name} loading="lazy" src={member.profile_path} />
                  ) : (
                    <div className="cast-card__placeholder">
                      <span>{castInitials(member) || '??'}</span>
                    </div>
                  )}
                </div>
                <div className="cast-card__body">
                  <p className="cast-card__name">{member.name}</p>
                  <p className="cast-card__role">
                    {member.character_name
                      ? l('as {{character}}', { character: member.character_name })
                      : l('Actor')}
                  </p>
                </div>
              </article>
            ))}
          </ScrollableRail>
        ) : null}
      </div>
    </section>
  )
}
