import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { MemoryRouter } from 'react-router-dom'
import { describe, expect, it } from 'vitest'
import type { Library } from '../../api/types'
import { LibrariesSection } from './libraries-section'
import { LibraryContentSections } from './library-content-sections'
import type { HomeLibraryModuleData } from './types'

const library: Library = {
  id: 7,
  name: 'Movies',
  description: null,
  library_type: 'movie',
  metadata_language: 'zh-CN',
  root_path: '/media/movies',
  is_enabled: true,
  created_at: '2026-04-07T00:00:00Z',
  updated_at: '2026-04-07T00:00:00Z',
}

const renderWithRouter = (node: ReactNode) => render(<MemoryRouter>{node}</MemoryRouter>)

describe('home page scan state rendering', () => {
  it('shows a syncing library spotlight before library detail finishes loading', () => {
    const libraryModules: HomeLibraryModuleData[] = [
      {
        detail: null,
        detailLoading: true,
        library,
        scanRuntime: {
          items: [],
          scanJob: null,
        },
        shelfError: null,
        shelfItems: [],
        shelfLoading: true,
      },
    ]

    renderWithRouter(<LibrariesSection isLoading={false} libraryModules={libraryModules} />)

    expect(screen.getByText('正在同步媒体库状态')).toBeInTheDocument()
    expect(screen.getByText('10%')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /Movies/i })).toBeInTheDocument()
  })

  it('renders a pending scan placeholder in the library shelf before any item card appears', () => {
    const libraryModules: HomeLibraryModuleData[] = [
      {
        detail: {
          ...library,
          media_count: 0,
          movie_count: 0,
          series_count: 0,
          last_scan: {
            id: 88,
            library_id: 7,
            status: 'running',
            phase: 'discovering',
            total_files: 0,
            scanned_files: 0,
            created_at: '2026-04-07T00:00:00Z',
            started_at: '2026-04-07T00:00:05Z',
            finished_at: null,
            error_message: null,
          },
        },
        detailLoading: false,
        library,
        scanRuntime: {
          items: [],
          scanJob: {
            id: 88,
            library_id: 7,
            status: 'running',
            phase: 'discovering',
            total_files: 0,
            scanned_files: 0,
            created_at: '2026-04-07T00:00:00Z',
            started_at: '2026-04-07T00:00:05Z',
            finished_at: null,
            error_message: null,
          },
        },
        shelfError: null,
        shelfItems: [],
        shelfLoading: false,
      },
    ]

    renderWithRouter(<LibraryContentSections isLoading={false} libraryModules={libraryModules} />)

    expect(screen.getAllByText('Movies')).toHaveLength(2)
    expect(screen.getAllByText('正在发现文件 0').length).toBeGreaterThanOrEqual(2)
    expect(screen.getByText('12%')).toBeInTheDocument()
    expect(screen.getByText('library')).toBeInTheDocument()
  })
})
