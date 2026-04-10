import { useMutation, useQueryClient } from '@tanstack/react-query'
import { type FormEvent, useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import { ApiError, applyMediaItemMetadataMatch, searchMediaItemMetadata } from '../../api/client'
import type { MetadataSearchResult } from '../../api/types'

interface MetadataMatchPanelProps {
  canOpen: boolean
  mediaItemId: number
  mediaType: string
  initialQuery: string
  initialYear: number | null
}

const renderResultTitle = (result: MetadataSearchResult) =>
  result.year ? `${result.title} · ${result.year}` : result.title

export const MetadataMatchPanel = ({
  canOpen,
  mediaItemId,
  mediaType,
  initialQuery,
  initialYear,
}: MetadataMatchPanelProps) => {
  const queryClient = useQueryClient()
  const [isOpen, setIsOpen] = useState(false)
  const [query, setQuery] = useState(initialQuery)
  const [yearInput, setYearInput] = useState(initialYear ? String(initialYear) : '')
  const [results, setResults] = useState<MetadataSearchResult[]>([])
  const [selectedProviderItemId, setSelectedProviderItemId] = useState<number | null>(null)
  const [statusMessage, setStatusMessage] = useState<string | null>(null)

  useEffect(() => {
    if (!isOpen) {
      return
    }

    setQuery(initialQuery)
    setYearInput(initialYear ? String(initialYear) : '')
    setResults([])
    setSelectedProviderItemId(null)
    setStatusMessage(null)
  }, [initialQuery, initialYear, isOpen])

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const previousOverflow = document.body.style.overflow
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setIsOpen(false)
      }
    }

    // 打开弹窗时锁定 body 滚动，避免背景页面跟着滚动。
    document.body.style.overflow = 'hidden'
    window.addEventListener('keydown', handleKeyDown)

    return () => {
      document.body.style.overflow = previousOverflow
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [isOpen])

  const searchMutation = useMutation({
    mutationFn: (payload: { query: string; year?: number }) =>
      searchMediaItemMetadata(mediaItemId, payload),
    onMutate: () => {
      setStatusMessage(null)
      setSelectedProviderItemId(null)
    },
    onSuccess: (searchResults) => {
      setResults(searchResults)
      setSelectedProviderItemId(searchResults[0]?.provider_item_id ?? null)
      setStatusMessage(searchResults.length === 0 ? 'No matches found.' : null)
    },
  })

  const matchMutation = useMutation({
    mutationFn: (providerItemId: number) =>
      applyMediaItemMetadataMatch(mediaItemId, providerItemId),
    onMutate: () => {
      setStatusMessage(null)
    },
    onSuccess: async () => {
      // 手动匹配会直接影响详情、演员和剧集 outline，所以这里把相关查询一起刷新。
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['media-item', mediaItemId] }),
        queryClient.invalidateQueries({ queryKey: ['media-episode-outline', mediaItemId] }),
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
      ])
      setIsOpen(false)
    },
  })

  const handleSearch = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    const trimmedQuery = query.trim()
    if (!trimmedQuery) {
      setStatusMessage('Enter a title to search.')
      return
    }

    const normalizedYear = yearInput.trim()
    const parsedYear =
      normalizedYear.length > 0 && Number.isFinite(Number(normalizedYear))
        ? Number(normalizedYear)
        : undefined

    await searchMutation.mutateAsync({
      query: trimmedQuery,
      year: parsedYear,
    })
  }

  const handleApply = async () => {
    if (!selectedProviderItemId) {
      setStatusMessage('Select a match before replacing metadata.')
      return
    }

    await matchMutation.mutateAsync(selectedProviderItemId)
  }

  if (!canOpen) {
    return null
  }

  return (
    <>
      <button
        className="button button--toolbar metadata-match-trigger"
        onClick={() => setIsOpen(true)}
        type="button"
      >
        <span>Search / Replace Metadata</span>
      </button>

      {isOpen
        ? createPortal(
            <div className="metadata-match-modal">
              <button
                aria-label="Close metadata match dialog"
                className="metadata-match-modal__backdrop"
                onClick={() => setIsOpen(false)}
                type="button"
              />
              <div aria-modal="true" className="metadata-match-modal__surface" role="dialog">
                <div className="metadata-match-modal__header">
                  <div>
                    <p className="eyebrow">Admin</p>
                    <h3>Search and Replace Metadata</h3>
                  </div>

                  <button
                    aria-label="Close metadata match dialog"
                    className="button button--icon metadata-match-modal__close"
                    onClick={() => setIsOpen(false)}
                    type="button"
                  >
                    <svg
                      aria-hidden="true"
                      className="button__icon"
                      fill="none"
                      viewBox="0 0 24 24"
                    >
                      <path
                        d="M6 6L18 18M18 6L6 18"
                        stroke="currentColor"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth="1.8"
                      />
                    </svg>
                  </button>
                </div>

                <form className="metadata-match-modal__form" onSubmit={handleSearch}>
                  <label className="field">
                    <span>Title</span>
                    <input
                      onChange={(event) => setQuery(event.target.value)}
                      placeholder="Search title"
                      type="text"
                      value={query}
                    />
                  </label>

                  <label className="field metadata-match-modal__year-field">
                    <span>Year</span>
                    <input
                      inputMode="numeric"
                      onChange={(event) => setYearInput(event.target.value)}
                      placeholder="Optional"
                      type="text"
                      value={yearInput}
                    />
                  </label>

                  <button
                    className="button button--primary"
                    disabled={searchMutation.isPending}
                    type="submit"
                  >
                    {searchMutation.isPending ? 'Searching…' : 'Search'}
                  </button>
                </form>

                {statusMessage ? <p className="muted">{statusMessage}</p> : null}
                {searchMutation.isError ? (
                  <p className="callout callout--danger">
                    {searchMutation.error instanceof Error
                      ? searchMutation.error.message
                      : 'Metadata search failed'}
                  </p>
                ) : null}
                {matchMutation.isError ? (
                  <p className="callout callout--danger">
                    {matchMutation.error instanceof ApiError
                      ? matchMutation.error.message
                      : matchMutation.error instanceof Error
                        ? matchMutation.error.message
                        : 'Metadata replacement failed'}
                  </p>
                ) : null}

                {results.length > 0 ? (
                  <div className="metadata-match-modal__results">
                    {results.map((result) => {
                      const isSelected = selectedProviderItemId === result.provider_item_id

                      return (
                        <button
                          className={
                            isSelected
                              ? 'metadata-match-card metadata-match-card--selected'
                              : 'metadata-match-card'
                          }
                          key={result.provider_item_id}
                          onClick={() => setSelectedProviderItemId(result.provider_item_id)}
                          type="button"
                        >
                          <div className="metadata-match-card__poster">
                            {result.poster_path ? (
                              <img alt={result.title} loading="lazy" src={result.poster_path} />
                            ) : (
                              <div className="media-card__placeholder">
                                <span>{mediaType}</span>
                              </div>
                            )}
                          </div>

                          <div className="metadata-match-card__body">
                            <div className="metadata-match-card__copy">
                              <p className="metadata-match-card__title">
                                {renderResultTitle(result)}
                              </p>
                              {result.original_title && result.original_title !== result.title ? (
                                <p className="metadata-match-card__original-title">
                                  {result.original_title}
                                </p>
                              ) : null}
                              <p className="metadata-match-card__overview">
                                {result.overview ?? 'No overview available.'}
                              </p>
                            </div>

                            <span className="metadata-match-card__badge">
                              {isSelected ? 'Selected' : 'Select'}
                            </span>
                          </div>
                        </button>
                      )
                    })}
                  </div>
                ) : null}

                <div className="metadata-match-modal__footer">
                  <button
                    className="button button--toolbar"
                    onClick={() => setIsOpen(false)}
                    type="button"
                  >
                    Cancel
                  </button>
                  <button
                    className="button button--primary"
                    disabled={matchMutation.isPending || selectedProviderItemId === null}
                    onClick={handleApply}
                    type="button"
                  >
                    {matchMutation.isPending ? 'Applying…' : 'Apply Selected Match'}
                  </button>
                </div>
              </div>
            </div>,
            document.body,
          )
        : null}
    </>
  )
}
