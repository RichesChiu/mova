import { Link } from 'react-router-dom'
import {
  formatScanItemMeta,
  formatScanItemProgressCopy,
  getScanRuntimeItems,
  shouldShowScanPlaceholder,
} from '../../../components/app-shell/scan-runtime'
import {
  MediaCard,
  MediaCardScanPlaceholder,
  MediaCardSkeleton,
} from '../../../components/media-card'
import { ScrollableRail } from '../../../components/scrollable-rail'
import { SectionHelp } from '../../../components/section-help'
import type { HomeLibraryModuleData } from '../types'

interface LibraryContentSectionsProps {
  isLoading: boolean
  libraryModules: HomeLibraryModuleData[]
}

const SHELF_SKELETON_COUNT = 4
const SHELF_SKELETON_KEYS = ['slot-a', 'slot-b', 'slot-c', 'slot-d'] as const

const LibraryContentSectionSkeleton = ({ title }: { title: string }) => (
  <section className="catalog-block library-content-sections__block">
    <div className="catalog-block__header">
      <div className="catalog-block__title-row">
        <h3>{title}</h3>
      </div>
    </div>

    <p className="muted">Loading library shelf…</p>

    <ScrollableRail
      hint="Scroll horizontally."
      viewportClassName="library-content-sections__viewport"
    >
      {SHELF_SKELETON_KEYS.slice(0, SHELF_SKELETON_COUNT).map((key) => (
        <div className="library-content-sections__item" key={`${title}-${key}`}>
          <MediaCardSkeleton />
        </div>
      ))}
    </ScrollableRail>
  </section>
)

export const LibraryContentSections = ({
  isLoading,
  libraryModules,
}: LibraryContentSectionsProps) => (
  <div className="home-library-sections">
    {isLoading
      ? ['Loading library A', 'Loading library B'].map((title) => (
          <LibraryContentSectionSkeleton key={title} title={title} />
        ))
      : libraryModules.map(
          ({ detail, library, scanRuntime, shelfError, shelfItems, shelfLoading }) => {
            const showScanPlaceholder = shouldShowScanPlaceholder(detail?.last_scan, scanRuntime)
            const scanItems = showScanPlaceholder ? getScanRuntimeItems(scanRuntime) : []
            const currentScanItem = scanItems[0] ?? null

            return (
              <section className="catalog-block library-content-sections__block" key={library.id}>
                <div className="catalog-block__header">
                  <div className="catalog-block__title-row">
                    <h3>{library.name}</h3>
                    <SectionHelp
                      detail="This shelf shows a quick preview from the library. Open it for the full list."
                      title={`About ${library.name}`}
                    />
                  </div>
                  <Link className="library-content-sections__link" to={`/libraries/${library.id}`}>
                    <span>Open</span>
                    <span aria-hidden="true" className="library-content-sections__link-icon">
                      <svg
                        aria-hidden="true"
                        fill="none"
                        focusable="false"
                        height="14"
                        viewBox="0 0 14 14"
                        width="14"
                      >
                        <path
                          d="M4.25 2.5 8.75 7l-4.5 4.5"
                          stroke="currentColor"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          strokeWidth="1.5"
                        />
                      </svg>
                    </span>
                  </Link>
                </div>

                {currentScanItem ? (
                  <p className="muted">
                    {formatScanItemProgressCopy(currentScanItem)}
                    {scanItems.length > 1 ? ` · 已发现 ${scanItems.length} 个新条目` : null}
                  </p>
                ) : null}
                {shelfLoading ? <p className="muted">Loading library shelf…</p> : null}
                {shelfError ? (
                  <p className="callout callout--danger">{shelfError.message}</p>
                ) : null}

                {!shelfLoading && !shelfError && shelfItems.length === 0 && !showScanPlaceholder ? (
                  <div className="catalog-block__empty">
                    <p className="muted">No items yet.</p>
                  </div>
                ) : null}

                {shelfLoading && shelfItems.length === 0 && !showScanPlaceholder ? (
                  <ScrollableRail
                    hint="Scroll horizontally."
                    viewportClassName="library-content-sections__viewport"
                  >
                    {SHELF_SKELETON_KEYS.slice(0, SHELF_SKELETON_COUNT).map((key) => (
                      <div className="library-content-sections__item" key={`${library.id}-${key}`}>
                        <MediaCardSkeleton />
                      </div>
                    ))}
                  </ScrollableRail>
                ) : null}

                {shelfItems.length > 0 || showScanPlaceholder ? (
                  // Reuse the shared rail so library shelves, continue watching, and episodes all expose
                  // the same desktop scrolling affordances.
                  <ScrollableRail
                    hint="Scroll horizontally."
                    viewportClassName="library-content-sections__viewport"
                  >
                    {scanItems.map((item) => (
                      <div
                        className="library-content-sections__item"
                        key={`scan-${library.id}-${item.item_key}`}
                      >
                        <MediaCardScanPlaceholder
                          placeholderLabel={item.media_type.toUpperCase()}
                          progressPercent={item.progress_percent}
                          progressText={formatScanItemProgressCopy(item)}
                          subtitle={formatScanItemMeta(item)}
                          title={item.title}
                        />
                      </div>
                    ))}
                    {shelfItems.map((item) => (
                      <div className="library-content-sections__item" key={item.id}>
                        <MediaCard item={item} />
                      </div>
                    ))}
                  </ScrollableRail>
                ) : null}
              </section>
            )
          },
        )}
  </div>
)
