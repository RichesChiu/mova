import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { LibraryDetailTileArtwork } from './library-detail-tile-artwork'

describe('LibraryDetailTileArtwork', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('shows a cached poster immediately when remounting the library page', () => {
    vi.spyOn(HTMLImageElement.prototype, 'complete', 'get').mockReturnValue(true)
    vi.spyOn(HTMLImageElement.prototype, 'naturalWidth', 'get').mockReturnValue(320)

    render(
      <LibraryDetailTileArtwork
        alt="Cached poster"
        placeholderLabel="Movie"
        src="/api/artwork/poster.jpg"
      />,
    )

    expect(screen.getByRole('img', { name: 'Cached poster' })).toHaveClass(
      'library-detail-tile__image--loaded',
    )
    expect(screen.queryByText('Movie')).not.toBeInTheDocument()
  })

  it('falls back to the placeholder when a poster fails to load', () => {
    vi.spyOn(HTMLImageElement.prototype, 'complete', 'get').mockReturnValue(false)

    render(
      <LibraryDetailTileArtwork
        alt="Unavailable poster"
        placeholderLabel="Movie"
        src="/api/artwork/missing.jpg"
      />,
    )

    fireEvent.error(screen.getByRole('img', { name: 'Unavailable poster' }))

    expect(screen.queryByRole('img', { name: 'Unavailable poster' })).not.toBeInTheDocument()
    expect(screen.getByText('Movie')).toBeInTheDocument()
  })
})
