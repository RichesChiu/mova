import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { MemoryRouter, Outlet, Route, Routes } from 'react-router-dom'
import { beforeEach, describe, expect, it } from 'vitest'
import type { UserAccount } from '../../api/types'
import { I18nProvider } from '../../i18n'
import { TMDB_ATTRIBUTION_NOTICE, TMDB_LOGO_PATH } from '../../lib/tmdb-attribution'
import { AboutPage } from '.'

const currentUser: UserAccount = {
  id: 1,
  username: 'account',
  nickname: 'Account',
  role: 'viewer',
  is_enabled: true,
  library_ids: [],
  created_at: '2026-07-17T00:00:00Z',
  updated_at: '2026-07-17T00:00:00Z',
}

const renderAboutPage = (language: 'en-US' | 'zh-CN') => {
  window.localStorage.setItem('mova.interfaceLanguage', language)
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })

  return render(
    <I18nProvider>
      <QueryClientProvider client={queryClient}>
        <MemoryRouter initialEntries={['/about']}>
          <Routes>
            <Route
              element={
                <Outlet
                  context={{
                    currentUser,
                    libraries: [],
                    librariesLoading: false,
                    scanRuntimeByLibrary: {},
                  }}
                />
              }
            >
              <Route path="/about" element={<AboutPage />} />
            </Route>
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>
    </I18nProvider>,
  )
}

describe('AboutPage TMDB attribution', () => {
  beforeEach(() => {
    window.localStorage.clear()
  })

  it('shows the approved logo and required English notice', () => {
    renderAboutPage('en-US')

    expect(screen.getByText(TMDB_ATTRIBUTION_NOTICE)).toBeInTheDocument()
    const tmdbLink = screen
      .getAllByRole('link', { name: 'Visit TMDB' })
      .find((link) => link.querySelector('img'))
    expect(tmdbLink?.querySelector('img')).toHaveAttribute('src', TMDB_LOGO_PATH)
  })

  it('keeps the required notice verbatim alongside localized Chinese guidance', () => {
    renderAboutPage('zh-CN')

    expect(screen.getByText(TMDB_ATTRIBUTION_NOTICE)).toBeInTheDocument()
    expect(
      screen.getByText('MOVA 使用 TMDB 提供媒体元数据和图片。TMDB 不认可、认证或赞助 MOVA。'),
    ).toBeInTheDocument()
  })
})
