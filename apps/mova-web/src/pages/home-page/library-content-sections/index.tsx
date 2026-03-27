import { Link } from 'react-router-dom'
import { MediaCard } from '../../../components/media-card'
import { ScrollableRail } from '../../../components/scrollable-rail'
import { SectionHelp } from '../../../components/section-help'
import type { HomeLibraryModuleData } from '../types'

interface LibraryContentSectionsProps {
  libraryModules: HomeLibraryModuleData[]
}

export const LibraryContentSections = ({ libraryModules }: LibraryContentSectionsProps) => (
  <div className="home-library-sections">
    {libraryModules.map(({ library, shelfError, shelfItems, shelfLoading }) => (
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

        {shelfLoading ? <p className="muted">Loading library shelf…</p> : null}
        {shelfError ? <p className="callout callout--danger">{shelfError.message}</p> : null}

        {!shelfLoading && !shelfError && shelfItems.length === 0 ? (
          <div className="catalog-block__empty">
            <p className="muted">No items yet.</p>
          </div>
        ) : null}

        {shelfItems.length > 0 ? (
          // Reuse the shared rail so library shelves, continue watching, and episodes all expose
          // the same desktop scrolling affordances.
          <ScrollableRail
            hint="Scroll horizontally."
            viewportClassName="library-content-sections__viewport"
          >
            {shelfItems.map((item) => (
              <div className="library-content-sections__item" key={item.id}>
                <MediaCard item={item} />
              </div>
            ))}
          </ScrollableRail>
        ) : null}
      </section>
    ))}
  </div>
)
