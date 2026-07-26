import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { describe, expect, it, vi } from 'vitest'
import type { Library, LibraryDetail } from '../../api/types'
import { I18nProvider } from '../../i18n'
import { EMPTY_LIBRARY_SCAN_RUNTIME } from '../app-shell/scan-runtime'
import { LibrarySpotlightCard } from '.'

const library: Library = {
  id: 7,
  name: 'Main Library',
  description: 'A curated collection of films and series.',
  metadata_language: 'zh-CN',
  root_path: '/media',
  created_at: '2026-07-26T00:00:00Z',
  updated_at: '2026-07-26T00:00:00Z',
}

const detail: LibraryDetail = {
  ...library,
  media_count: 18,
  movie_count: 12,
  series_count: 6,
  last_scan: null,
}

describe('LibrarySpotlightCard', () => {
  it('shows the library description on the shared card', () => {
    render(
      <I18nProvider>
        <MemoryRouter>
          <LibrarySpotlightCard
            canManageLibraries={false}
            detail={detail}
            detailError={null}
            detailLoading={false}
            library={library}
            onDeleteLibrary={vi.fn()}
            onEditLibrary={vi.fn()}
            onScanLibrary={vi.fn()}
            recentItems={[]}
            scanRuntime={EMPTY_LIBRARY_SCAN_RUNTIME}
          />
        </MemoryRouter>
      </I18nProvider>,
    )

    const description = screen.getByText(library.description ?? '')

    expect(description).toHaveClass('library-spotlight__description')
    expect(description).toHaveAttribute('title', library.description)
  })
})
