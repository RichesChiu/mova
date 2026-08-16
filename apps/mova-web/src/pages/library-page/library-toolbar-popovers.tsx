import { type KeyboardEvent, useEffect, useRef, useState } from 'react'
import type { LibraryMediaCategory, LibraryMediaSortBy, SortOrder } from '../../api/types'
import { GlassMenu } from '../../components/glass-menu'
import { useI18n } from '../../i18n'
import { usePresenceTransition } from '../../lib/use-presence-transition'
import { HomeIcon } from '../home-page/home-icons'

export interface LibraryFilterValue {
  category: LibraryMediaCategory
  query: string
  year?: number
}

interface LibraryFilterPopoverProps {
  activeFilterCount: number
  isOpen: boolean
  onCategoryChange: (category: LibraryMediaCategory) => void
  onOpenChange: (isOpen: boolean) => void
  onQueryChange: (query: string) => void
  onReset: () => void
  onYearChange: (year?: number) => void
  value: LibraryFilterValue
}

interface LibrarySortMenuProps {
  isOpen: boolean
  onChange: (sortBy: LibraryMediaSortBy, sortOrder: SortOrder) => void
  onOpenChange: (isOpen: boolean) => void
  sortBy: LibraryMediaSortBy
  sortOrder: SortOrder
}

const QUERY_DEBOUNCE_MS = 280

const defaultSortOrder = (sortBy: LibraryMediaSortBy): SortOrder =>
  sortBy === 'title' ? 'asc' : 'desc'

const sortFieldOptions: LibraryMediaSortBy[] = ['title', 'year', 'rating']
const sortDirectionOptions: SortOrder[] = ['asc', 'desc']
const categoryOptions: LibraryMediaCategory[] = ['all', 'movie', 'series', 'needs_review']

export const LibrarySortMenu = ({
  isOpen,
  onChange,
  onOpenChange,
  sortBy,
  sortOrder,
}: LibrarySortMenuProps) => {
  const { l } = useI18n()
  const sortLabel = sortBy === 'title' ? l('Title') : sortBy === 'year' ? l('Year') : l('Rating')

  const fieldLabel = (value: LibraryMediaSortBy) =>
    value === 'title' ? l('Title') : value === 'year' ? l('Year') : l('Rating')

  return (
    <GlassMenu
      ariaLabel={l('Sort media')}
      id="library-sort-menu"
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      popoverClassName="library-toolbar-popover library-toolbar-popover--sort"
      rootClassName="library-toolbar-control library-toolbar-control--sort"
      trigger={(triggerProps) => (
        <button
          {...triggerProps}
          aria-label={l('Sort media')}
          className="button library-toolbar-trigger"
          type="button"
        >
          <HomeIcon className="library-toolbar-trigger__leading-icon" name="sort" />
          <span>{sortLabel}</span>
          <HomeIcon className="library-toolbar-trigger__caret" name="chevronDown" />
        </button>
      )}
    >
      {(closeMenu) => (
        <>
          <p className="library-toolbar-popover__label">{l('Sort method')}</p>
          <div className="library-toolbar-popover__options">
            {sortFieldOptions.map((option) => {
              const isSelected = sortBy === option
              return (
                <button
                  aria-checked={isSelected}
                  className={
                    isSelected
                      ? 'library-toolbar-popover__option library-toolbar-popover__option--selected'
                      : 'library-toolbar-popover__option'
                  }
                  key={option}
                  onClick={() => {
                    onChange(option, option === sortBy ? sortOrder : defaultSortOrder(option))
                    closeMenu()
                  }}
                  role="menuitemradio"
                  type="button"
                >
                  <span aria-hidden="true" className="library-toolbar-popover__check">
                    {isSelected ? <HomeIcon name="check" /> : null}
                  </span>
                  <span>{fieldLabel(option)}</span>
                </button>
              )
            })}
          </div>

          <hr className="library-toolbar-popover__divider" />

          <p className="library-toolbar-popover__label">{l('Sort direction')}</p>
          <div className="library-toolbar-popover__options">
            {sortDirectionOptions.map((option) => {
              const isSelected = sortOrder === option
              return (
                <button
                  aria-checked={isSelected}
                  className={
                    isSelected
                      ? 'library-toolbar-popover__option library-toolbar-popover__option--selected'
                      : 'library-toolbar-popover__option'
                  }
                  key={option}
                  onClick={() => {
                    onChange(sortBy, option)
                    closeMenu()
                  }}
                  role="menuitemradio"
                  type="button"
                >
                  <span aria-hidden="true" className="library-toolbar-popover__check">
                    {isSelected ? <HomeIcon name="check" /> : null}
                  </span>
                  <span>{option === 'asc' ? l('Ascending') : l('Descending')}</span>
                </button>
              )
            })}
          </div>
        </>
      )}
    </GlassMenu>
  )
}

export const LibraryFilterPopover = ({
  activeFilterCount,
  isOpen,
  onCategoryChange,
  onOpenChange,
  onQueryChange,
  onReset,
  onYearChange,
  value,
}: LibraryFilterPopoverProps) => {
  const { l } = useI18n()
  const rootRef = useRef<HTMLDivElement | null>(null)
  const searchInputRef = useRef<HTMLInputElement | null>(null)
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const wasOpenRef = useRef(isOpen)
  const [queryDraft, setQueryDraft] = useState(value.query)
  const [yearDraft, setYearDraft] = useState(value.year?.toString() ?? '')
  const [yearError, setYearError] = useState(false)
  const presence = usePresenceTransition(isOpen)

  useEffect(() => {
    if (!isOpen || wasOpenRef.current) {
      return
    }

    setQueryDraft(value.query)
    setYearDraft(value.year?.toString() ?? '')
    setYearError(false)
    const focusFrame = window.requestAnimationFrame(() => searchInputRef.current?.focus())
    return () => window.cancelAnimationFrame(focusFrame)
  }, [isOpen, value.query, value.year])

  useEffect(() => {
    if (!isOpen || queryDraft.trim() === value.query) {
      return
    }

    const timeoutId = window.setTimeout(() => onQueryChange(queryDraft.trim()), QUERY_DEBOUNCE_MS)
    return () => window.clearTimeout(timeoutId)
  }, [isOpen, onQueryChange, queryDraft, value.query])

  useEffect(() => {
    const wasOpen = wasOpenRef.current
    wasOpenRef.current = isOpen
    if (!wasOpen || isOpen) {
      return
    }

    const nextQuery = queryDraft.trim()
    if (nextQuery !== value.query) {
      onQueryChange(nextQuery)
    }

    const normalizedYear = yearDraft.trim()
    if (!normalizedYear) {
      if (value.year !== undefined) {
        onYearChange(undefined)
      }
      return
    }

    const nextYear = Number(normalizedYear)
    if (Number.isInteger(nextYear) && nextYear > 0 && nextYear !== value.year) {
      onYearChange(nextYear)
    }
  }, [isOpen, onQueryChange, onYearChange, queryDraft, value.query, value.year, yearDraft])

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const handlePointerDown = (event: MouseEvent) => {
      if (event.target instanceof Node && rootRef.current?.contains(event.target)) {
        return
      }
      onOpenChange(false)
    }
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key !== 'Escape') {
        return
      }
      event.preventDefault()
      onOpenChange(false)
      triggerRef.current?.focus()
    }

    document.addEventListener('mousedown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)
    return () => {
      document.removeEventListener('mousedown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [isOpen, onOpenChange])

  const commitQuery = () => {
    const nextQuery = queryDraft.trim()
    if (nextQuery !== value.query) {
      onQueryChange(nextQuery)
    }
  }

  const commitYear = () => {
    const normalizedDraft = yearDraft.trim()
    if (!normalizedDraft) {
      setYearError(false)
      onYearChange(undefined)
      return
    }

    const nextYear = Number(normalizedDraft)
    if (!Number.isInteger(nextYear) || nextYear <= 0) {
      setYearError(true)
      return
    }

    setYearError(false)
    onYearChange(nextYear)
  }

  const handleInputKeyDown = (event: KeyboardEvent<HTMLInputElement>, commit: () => void) => {
    if (event.key !== 'Enter') {
      return
    }
    event.preventDefault()
    commit()
  }

  const categoryLabel = (category: LibraryMediaCategory) =>
    category === 'movie'
      ? l('Movies')
      : category === 'series'
        ? l('Series')
        : category === 'needs_review'
          ? l('Needs review')
          : l('All')

  return (
    <div
      className="library-toolbar-control library-toolbar-control--filter"
      data-state={isOpen ? 'open' : 'closed'}
      ref={rootRef}
    >
      <button
        aria-controls="library-filter-popover"
        aria-expanded={isOpen}
        aria-haspopup="dialog"
        aria-label={l('Filter media')}
        className="button library-toolbar-trigger"
        onClick={() => onOpenChange(!isOpen)}
        ref={triggerRef}
        type="button"
      >
        <HomeIcon className="library-toolbar-trigger__leading-icon" name="filter" />
        <span>{l('Filter')}</span>
        {activeFilterCount > 0 ? (
          <strong title={l('{{count}} active filters', { count: activeFilterCount })}>
            {activeFilterCount}
          </strong>
        ) : null}
      </button>

      {presence.shouldRender ? (
        <section
          aria-hidden={!isOpen}
          aria-labelledby="library-filter-popover-title"
          className="library-toolbar-popover library-toolbar-popover--filter glass-popover-surface floating-transition"
          data-state={presence.transitionState}
          id="library-filter-popover"
          inert={!isOpen}
          role="dialog"
        >
          <div className="library-filter-popover__header">
            <h4 id="library-filter-popover-title">{l('Filter')}</h4>
            <button
              className="library-filter-popover__reset"
              disabled={activeFilterCount === 0}
              onClick={() => {
                setQueryDraft('')
                setYearDraft('')
                setYearError(false)
                onReset()
              }}
              type="button"
            >
              {l('Reset')}
            </button>
          </div>

          <label className="library-filter-popover__search">
            <HomeIcon name="search" />
            <input
              onBlur={commitQuery}
              onChange={(event) => setQueryDraft(event.target.value)}
              onKeyDown={(event) => handleInputKeyDown(event, commitQuery)}
              placeholder={l('Search title or original title')}
              ref={searchInputRef}
              type="search"
              value={queryDraft}
            />
          </label>

          <div className="library-filter-popover__field">
            <span>{l('Category')}</span>
            <fieldset
              aria-label={l('Filter by category')}
              className="library-filter-popover__segments"
            >
              {categoryOptions.map((option) => (
                <button
                  aria-pressed={value.category === option}
                  key={option}
                  onClick={() => onCategoryChange(option)}
                  type="button"
                >
                  {categoryLabel(option)}
                </button>
              ))}
            </fieldset>
          </div>

          <label className="library-filter-popover__field">
            <span>{l('Release year')}</span>
            <span className="library-filter-popover__year-input">
              <HomeIcon name="calendar" />
              <input
                aria-describedby={yearError ? 'library-filter-year-error' : undefined}
                aria-invalid={yearError}
                inputMode="numeric"
                min="1"
                onBlur={commitYear}
                onChange={(event) => {
                  setYearDraft(event.target.value)
                  setYearError(false)
                }}
                onKeyDown={(event) => handleInputKeyDown(event, commitYear)}
                placeholder={l('All years')}
                type="number"
                value={yearDraft}
              />
            </span>
            {yearError ? (
              <small id="library-filter-year-error" role="alert">
                {l('Enter a valid year.')}
              </small>
            ) : null}
          </label>
        </section>
      ) : null}
    </div>
  )
}
