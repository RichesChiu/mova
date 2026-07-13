import { useQuery } from '@tanstack/react-query'
import { type FormEvent, useEffect, useState } from 'react'
import { Link, useNavigate, useOutletContext, useSearchParams } from 'react-router-dom'
import { globalSearch } from '../../api/client'
import type { GlobalSearchResult, MediaType } from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import { type Translate, useI18n } from '../../i18n'
import { mediaTypePrimaryPath } from '../../lib/media-routes'
import { DashboardPageHeader } from '../home-page/dashboard-page-header'
import { HomeDashboardShell } from '../home-page/home-dashboard-shell'
import { HomeIcon } from '../home-page/home-icons'

const SEARCH_RESULTS_LIMIT = 48
const SEARCH_LOADING_PLACEHOLDER_KEYS = [
  'search-loading-1',
  'search-loading-2',
  'search-loading-3',
  'search-loading-4',
  'search-loading-5',
  'search-loading-6',
  'search-loading-7',
  'search-loading-8',
] as const

const formatEpisodeIndex = (seasonNumber: number | null, episodeNumber: number | null) => {
  if (typeof seasonNumber !== 'number' || typeof episodeNumber !== 'number') {
    return null
  }

  return `S${String(seasonNumber).padStart(2, '0')} · E${String(episodeNumber).padStart(2, '0')}`
}

const formatResultTypeLabel = (kind: string, mediaType: MediaType, l: Translate) => {
  if (kind === 'episode') {
    return l('Episode')
  }

  if (mediaType === 'movie') {
    return l('Movie')
  }

  if (mediaType === 'series') {
    return l('Series')
  }

  return l('Media')
}

const searchResultPath = (result: GlobalSearchResult) =>
  mediaTypePrimaryPath(result.media_item_id, result.media_type)

export const SearchPage = () => {
  const { formatNumber, l } = useI18n()
  const navigate = useNavigate()
  const { currentUser } = useOutletContext<AppShellOutletContext>()
  const [searchParams] = useSearchParams()
  const query = searchParams.get('q')?.trim() ?? ''
  const [searchTerm, setSearchTerm] = useState(query)
  const searchQuery = useQuery({
    enabled: query.length > 0,
    queryKey: ['global-search-page', query, SEARCH_RESULTS_LIMIT],
    queryFn: () => globalSearch(query, SEARCH_RESULTS_LIMIT),
    staleTime: 15_000,
  })
  const results = searchQuery.data ?? []

  useEffect(() => {
    setSearchTerm(query)
  }, [query])

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        document.querySelector<HTMLInputElement>('[data-search-page-input="true"]')?.focus()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  const submitSearch = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    const nextQuery = searchTerm.trim()
    if (!nextQuery) {
      navigate('/search')
      return
    }

    const params = new URLSearchParams({ q: nextQuery })
    navigate(`/search?${params.toString()}`)
  }

  return (
    <HomeDashboardShell ariaLabel={l('Search')} currentUser={currentUser}>
      <div className="home-dashboard__content home-dashboard__content--search">
        <DashboardPageHeader className="search-page__top">
          <form className="search-page__form" onSubmit={submitSearch}>
            <input
              aria-label={l('Search media in your libraries…')}
              autoComplete="off"
              data-search-page-input="true"
              onChange={(event) => setSearchTerm(event.target.value)}
              placeholder={l('Search media in your libraries…')}
              value={searchTerm}
            />
            <button aria-label={l('Search')} className="search-page__submit" type="submit">
              <HomeIcon name="search" />
            </button>
          </form>
        </DashboardPageHeader>

        <section className="catalog-block search-page">
          {query ? (
            <div className="catalog-block__header search-page__header">
              <div className="catalog-block__title-row">
                <h3>{l('Search results for "{{query}}"', { query })}</h3>
              </div>
              {!searchQuery.isLoading && !searchQuery.isError ? (
                <span className="search-page__count">
                  {l('{{count}} results', { count: formatNumber(results.length) })}
                </span>
              ) : null}
            </div>
          ) : null}

          {!query ? (
            <div className="search-page__empty">
              <p>{l('Start a search to see matching movies, series, and episodes.')}</p>
            </div>
          ) : searchQuery.isLoading ? (
            <div aria-label={l('Searching…')} className="search-page__grid" role="status">
              {SEARCH_LOADING_PLACEHOLDER_KEYS.map((key) => (
                <div aria-hidden="true" className="search-card search-card--loading" key={key}>
                  <span className="search-card__poster search-card__poster--loading skeleton-shimmer" />
                  <span className="search-card__line search-card__line--title skeleton-shimmer" />
                  <span className="search-card__line search-card__line--meta skeleton-shimmer" />
                </div>
              ))}
            </div>
          ) : searchQuery.isError ? (
            <p className="callout callout--danger">
              {searchQuery.error instanceof Error
                ? searchQuery.error.message
                : l('Failed to search media')}
            </p>
          ) : results.length === 0 ? (
            <div className="search-page__empty">
              <p>{l('No results found for "{{query}}".', { query })}</p>
            </div>
          ) : (
            <div className="search-page__grid">
              {results.map((result) => {
                const episodeIndex = formatEpisodeIndex(result.season_number, result.episode_number)
                const resultTypeLabel = formatResultTypeLabel(result.kind, result.media_type, l)
                const meta = [
                  resultTypeLabel,
                  result.subtitle,
                  episodeIndex,
                  result.year,
                  result.library_name,
                ].filter(Boolean)

                return (
                  <Link
                    className="search-card"
                    key={`${result.kind}-${result.media_item_id}-${episodeIndex ?? 'item'}`}
                    to={searchResultPath(result)}
                  >
                    <span className="search-card__poster">
                      {result.poster_path ? (
                        <img
                          alt={l('{{title}} poster', { title: result.title })}
                          loading="lazy"
                          src={result.poster_path}
                        />
                      ) : (
                        <span className="search-card__placeholder">{resultTypeLabel}</span>
                      )}
                    </span>

                    <span className="search-card__body">
                      <strong title={result.title}>{result.title}</strong>
                      <em title={meta.join(' · ')}>{meta.join(' · ')}</em>
                      {result.overview ? (
                        <small title={result.overview}>{result.overview}</small>
                      ) : null}
                    </span>
                  </Link>
                )
              })}
            </div>
          )}
        </section>
      </div>
    </HomeDashboardShell>
  )
}
