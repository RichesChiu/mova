import { Link } from 'react-router-dom'
import type { Library } from '../../../api/types'
import { EmptyState } from '../../../components/empty-state'
import {
  LibrarySpotlightCard,
  LibrarySpotlightCardSkeleton,
} from '../../../components/library-spotlight-card'
import { useI18n } from '../../../i18n'
import {
  getVisibleHomeLibraries,
  HOME_LIBRARY_LIMIT,
  shouldShowAllHomeLibraries,
} from '../../../lib/home-sections'
import type { HomeLibraryModuleData } from '../types'

interface LibrariesSectionProps {
  actionErrorMessage?: string | null
  canManageLibraries: boolean
  isLoading: boolean
  libraryModules: HomeLibraryModuleData[]
  pendingScanLibraryId?: number | null
  onDeleteLibrary: (library: Library) => void
  onEditLibrary: (library: Library) => void
  onScanLibrary: (library: Library) => void
  totalLibraryCount: number
}

const LIBRARY_SPOTLIGHT_SKELETON_KEYS = [
  'library-a',
  'library-b',
  'library-c',
  'library-d',
  'library-e',
] as const

export const LibrariesSection = ({
  actionErrorMessage,
  canManageLibraries,
  isLoading,
  libraryModules,
  pendingScanLibraryId = null,
  onDeleteLibrary,
  onEditLibrary,
  onScanLibrary,
  totalLibraryCount,
}: LibrariesSectionProps) => {
  const { l } = useI18n()
  const visibleLibraryModules = getVisibleHomeLibraries(libraryModules)

  return (
    <section className="catalog-block libraries-section">
      <div className="catalog-block__header">
        <div className="catalog-block__title-row">
          <h3>{l('Your Libraries')}</h3>
          {shouldShowAllHomeLibraries(totalLibraryCount) ? (
            <Link className="libraries-section__title-action" to="/libraries">
              {l('View all')}
            </Link>
          ) : null}
        </div>
      </div>

      {isLoading ? (
        <>
          <p className="muted">{l('Loading libraries…')}</p>
          <div className="libraries-section__grid">
            {LIBRARY_SPOTLIGHT_SKELETON_KEYS.slice(0, HOME_LIBRARY_LIMIT).map((key) => (
              <LibrarySpotlightCardSkeleton key={key} />
            ))}
          </div>
        </>
      ) : libraryModules.length === 0 ? (
        <EmptyState
          description={l('Create a library in Server Settings to start organizing your media.')}
          title={l('No libraries yet.')}
        />
      ) : (
        <>
          {actionErrorMessage ? (
            <p className="callout callout--danger">{actionErrorMessage}</p>
          ) : null}
          <div className="libraries-section__grid">
            {visibleLibraryModules.map(
              ({ detail, detailError, detailLoading, library, recentItems, scanRuntime }) => (
                <LibrarySpotlightCard
                  canManageLibraries={canManageLibraries}
                  detail={detail}
                  detailError={detailError}
                  detailLoading={detailLoading}
                  isScanPending={pendingScanLibraryId === library.id}
                  key={library.id}
                  library={library}
                  onDeleteLibrary={onDeleteLibrary}
                  onEditLibrary={onEditLibrary}
                  onScanLibrary={onScanLibrary}
                  recentItems={recentItems}
                  scanRuntime={scanRuntime}
                />
              ),
            )}
          </div>
        </>
      )}
    </section>
  )
}
