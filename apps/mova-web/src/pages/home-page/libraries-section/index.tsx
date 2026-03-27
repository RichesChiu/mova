import { Link } from 'react-router-dom'
import { ScrollableRail } from '../../../components/scrollable-rail'
import { SectionHelp } from '../../../components/section-help'
import type { HomeLibraryModuleData } from '../types'

interface LibrariesSectionProps {
  libraryModules: HomeLibraryModuleData[]
}

export const LibrariesSection = ({ libraryModules }: LibrariesSectionProps) => (
  <section className="catalog-block libraries-section">
    <div className="catalog-block__header">
      <div className="catalog-block__title-row">
        <h3>Libraries</h3>
        <SectionHelp
          detail="Browse every library from here. Open one to see the full catalog view."
          title="About libraries"
        />
      </div>
      <span className="counter-badge">{libraryModules.length}</span>
    </div>

    {libraryModules.length === 0 ? (
      <div className="catalog-block__empty">
        <p className="muted">No libraries yet.</p>
      </div>
    ) : (
      <ScrollableRail hint="Scroll horizontally." viewportClassName="libraries-section__viewport">
        {libraryModules.map(({ detail, library, shelfItems }) => {
          // Use the first few posters as a lightweight library backdrop so a new library card still
          // feels alive before it gets custom artwork or richer metadata.
          const collagePosters = shelfItems
            .map((item) => item.poster_path ?? item.backdrop_path)
            .filter((value): value is string => Boolean(value))
            .slice(0, 4)

          return (
            <Link className="library-spotlight" key={library.id} to={`/libraries/${library.id}`}>
              <div className="library-spotlight__backdrop" aria-hidden="true">
                {collagePosters.length > 0 ? (
                  collagePosters.map((posterPath, posterIndex) => (
                    <span
                      className={`library-spotlight__poster library-spotlight__poster--${posterIndex + 1}`}
                      key={`${library.id}-${posterPath}`}
                      style={{ backgroundImage: `url(${posterPath})` }}
                    />
                  ))
                ) : (
                  <span className="library-spotlight__fallback" />
                )}
              </div>

              <div className="library-spotlight__content">
                <span className="library-spotlight__type">{library.library_type}</span>
                <strong className="library-spotlight__title">{library.name}</strong>

                <div className="library-spotlight__stats">
                  <span className="library-spotlight__stat">{detail?.media_count ?? 0} items</span>
                  {library.library_type === 'mixed' ? (
                    <>
                      <span className="library-spotlight__stat">
                        {detail?.movie_count ?? 0} movies
                      </span>
                      <span className="library-spotlight__stat">
                        {detail?.series_count ?? 0} series
                      </span>
                    </>
                  ) : null}
                </div>
              </div>
            </Link>
          )
        })}
      </ScrollableRail>
    )}
  </section>
)
