import {
  formatPendingScanPlaceholderCopy,
  formatScanItemMeta,
  formatScanItemProgressCopy,
  formatScanJobStatusCopy,
  getEffectiveScanJob,
  getScanJobProgressPercent,
  getScanRuntimeItems,
  isLibraryScanActive,
  shouldShowScanPlaceholder,
} from '../../../components/app-shell/scan-runtime'
import {
  MediaCard,
  MediaCardScanPlaceholder,
  MediaCardSkeleton,
} from '../../../components/media-card'
import { useI18n } from '../../../i18n'
import { ScrollableRail } from '../../../components/scrollable-rail'
import { SectionHelp } from '../../../components/section-help'
import type { HomeLibraryModuleData } from '../types'

interface LibraryContentSectionsProps {
  isLoading: boolean
  libraryModules: HomeLibraryModuleData[]
}

const SHELF_SKELETON_COUNT = 4
const SHELF_SKELETON_KEYS = ['slot-a', 'slot-b', 'slot-c', 'slot-d'] as const

const LibraryContentSectionSkeleton = ({ title }: { title: string }) => {
  const { l } = useI18n()

  return (
    <section className="catalog-block library-content-sections__block">
      <div className="catalog-block__header">
        <div className="catalog-block__title-row">
          <h3>{title}</h3>
        </div>
      </div>

      <p className="muted">{l('Loading library shelf…')}</p>

      <ScrollableRail
        hint={l('Scroll horizontally.')}
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
}

export const LibraryContentSections = ({
  isLoading,
  libraryModules,
}: LibraryContentSectionsProps) => (
  <LibraryContentSectionsBody isLoading={isLoading} libraryModules={libraryModules} />
)

const LibraryContentSectionsBody = ({
  isLoading,
  libraryModules,
}: LibraryContentSectionsProps) => {
  const { l } = useI18n()

  return (
    <div className="home-library-sections">
      {isLoading
        ? [l('Loading library A'), l('Loading library B')].map((title) => (
            <LibraryContentSectionSkeleton key={title} title={title} />
          ))
        : libraryModules.map(
            ({
              detail,
              detailLoading,
              library,
              scanRuntime,
              shelfError,
              shelfItems,
              shelfLoading,
            }) => {
            const effectiveScanJob = getEffectiveScanJob(detail?.last_scan, scanRuntime)
            const isScanning = isLibraryScanActive(effectiveScanJob, scanRuntime)
            const showScanPlaceholder = shouldShowScanPlaceholder(detail?.last_scan, scanRuntime)
            const scanItems = showScanPlaceholder ? getScanRuntimeItems(scanRuntime) : []
            const currentScanItem = scanItems[0] ?? null
            const scanCopy =
              formatScanJobStatusCopy(effectiveScanJob, scanRuntime) ??
              (detailLoading ? l('Syncing library state') : null)
            const scanProgressPercent = getScanJobProgressPercent(effectiveScanJob, scanRuntime)

            return (
              <section className="catalog-block library-content-sections__block" key={library.id}>
                <div className="catalog-block__header">
                  <div className="catalog-block__title-row">
                    <h3>{library.name}</h3>
                    <SectionHelp
                      detail={l(
                        'This shelf shows a quick preview from the library.',
                      )}
                      placement="bottom"
                      title={l('About {{name}}', { name: library.name })}
                    />
                  </div>
                </div>

                {currentScanItem ? (
                  <p className="muted">
                    {formatScanItemProgressCopy(currentScanItem)}
                    {scanItems.length > 1
                      ? ` · ${l('{{count}} new items discovered', { count: scanItems.length })}`
                      : null}
                  </p>
                ) : scanCopy ? (
                  <p className="muted">{scanCopy}</p>
                ) : null}
                {shelfLoading ? <p className="muted">{l('Loading library shelf…')}</p> : null}
                {shelfError ? (
                  <p className="callout callout--danger">{shelfError.message}</p>
                ) : null}

                {!shelfLoading && !shelfError && shelfItems.length === 0 && !showScanPlaceholder ? (
                  <div className="catalog-block__empty">
                    <p className="muted">{l('No items yet.')}</p>
                  </div>
                ) : null}

                {shelfLoading && shelfItems.length === 0 && !showScanPlaceholder ? (
                  <ScrollableRail
                    hint={l('Scroll horizontally.')}
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
                    hint={l('Scroll horizontally.')}
                    viewportClassName="library-content-sections__viewport"
                  >
                    {isScanning && scanItems.length === 0 ? (
                      <div
                        className="library-content-sections__item"
                        key={`scan-pending-${library.id}`}
                      >
                        <MediaCardScanPlaceholder
                          placeholderLabel={l('Media')}
                          progressPercent={scanProgressPercent}
                          progressText={formatPendingScanPlaceholderCopy(
                            effectiveScanJob,
                            scanRuntime,
                            library.name,
                          )}
                          subtitle={l('Library')}
                          title={library.name}
                        />
                      </div>
                    ) : null}
                    {scanItems.map((item) => (
                      <div
                        className="library-content-sections__item"
                        key={`scan-${library.id}-${item.item_key}`}
                      >
                        <MediaCardScanPlaceholder
                          placeholderLabel={
                            item.media_type === 'movie' ? l('Movies') : l('Series')
                          }
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
}
