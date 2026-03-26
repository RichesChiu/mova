import { useMutation, useQueryClient } from '@tanstack/react-query'
import { type FormEvent, useEffect, useState } from 'react'
import { ApiError, applyMediaItemMetadataMatch, searchMediaItemMetadata } from '../../api/client'
import type { MetadataSearchResult } from '../../api/types'

interface MetadataMatchPanelProps {
  mediaItemId: number
  mediaType: string
  initialQuery: string
  initialYear: number | null
}

export const MetadataMatchPanel = ({
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
  const [statusMessage, setStatusMessage] = useState<string | null>(null)

  useEffect(() => {
    if (!isOpen) {
      return
    }

    setQuery(initialQuery)
    setYearInput(initialYear ? String(initialYear) : '')
    setStatusMessage(null)
  }, [initialQuery, initialYear, isOpen])

  const searchMutation = useMutation({
    mutationFn: (payload: { query: string; year?: number }) =>
      searchMediaItemMetadata(mediaItemId, payload),
    onMutate: () => {
      setStatusMessage(null)
    },
    onSuccess: (searchResults) => {
      setResults(searchResults)
      setStatusMessage(searchResults.length === 0 ? '未找到匹配结果。' : null)
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
      setResults([])
      setStatusMessage('元数据已替换为所选结果。')
    },
  })

  const handleSearch = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    const trimmedQuery = query.trim()
    if (!trimmedQuery) {
      setStatusMessage('请输入资源名称。')
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

  const renderResultTitle = (result: MetadataSearchResult) =>
    result.year ? `${result.title} · ${result.year}` : result.title

  return (
    <section className="season-card metadata-match-panel">
      <div className="metadata-match-panel__header">
        <div>
          <p className="eyebrow">Admin</p>
          <h3>Manual Metadata Match</h3>
          <p className="muted">
            手动输入{mediaType === 'series' ? '剧名' : '片名'}与年份，搜索后选中正确结果替换。
          </p>
        </div>

        <button
          className="button button--toolbar"
          onClick={() => setIsOpen((open) => !open)}
          type="button"
        >
          {isOpen ? 'Close' : 'Re-match'}
        </button>
      </div>

      {isOpen ? (
        <>
          <form className="metadata-match-panel__form" onSubmit={handleSearch}>
            <label className="field">
              <span>Title</span>
              <input
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search title"
                type="text"
                value={query}
              />
            </label>

            <label className="field metadata-match-panel__year-field">
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
                : '搜索元数据失败'}
            </p>
          ) : null}
          {matchMutation.isError ? (
            <p className="callout callout--danger">
              {matchMutation.error instanceof ApiError
                ? matchMutation.error.message
                : matchMutation.error instanceof Error
                  ? matchMutation.error.message
                  : '替换元数据失败'}
            </p>
          ) : null}

          {results.length > 0 ? (
            <div className="metadata-match-panel__results">
              {results.map((result) => (
                <article className="metadata-match-card" key={result.provider_item_id}>
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
                      <p className="metadata-match-card__title">{renderResultTitle(result)}</p>
                      {result.original_title && result.original_title !== result.title ? (
                        <p className="metadata-match-card__original-title">
                          {result.original_title}
                        </p>
                      ) : null}
                      <p className="metadata-match-card__overview">
                        {result.overview ?? 'No overview available.'}
                      </p>
                    </div>

                    <button
                      className="button button--toolbar"
                      disabled={matchMutation.isPending}
                      onClick={() => matchMutation.mutate(result.provider_item_id)}
                      type="button"
                    >
                      {matchMutation.isPending ? 'Applying…' : 'Use This Match'}
                    </button>
                  </div>
                </article>
              ))}
            </div>
          ) : null}
        </>
      ) : null}
    </section>
  )
}
