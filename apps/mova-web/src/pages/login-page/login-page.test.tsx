import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../../i18n'
import { LoginPage } from '.'

const unauthorizedResponse = () =>
  new Response(
    JSON.stringify({
      code: 401,
      data: null,
      error_code: 'unauthorized',
      message: 'authentication required',
    }),
    {
      headers: { 'Content-Type': 'application/json' },
      status: 401,
      statusText: 'Unauthorized',
    },
  )

const successResponse = (data: unknown) =>
  new Response(JSON.stringify({ code: 200, data, message: 'ok' }), {
    headers: { 'Content-Type': 'application/json' },
    status: 200,
  })

const renderLoginPage = (bootstrapRequired: boolean) => {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const path = String(input)
    if (path === '/api/auth/me') {
      return unauthorizedResponse()
    }
    if (path === '/api/auth/bootstrap-status') {
      return successResponse({ bootstrap_required: bootstrapRequired })
    }

    throw new Error(`Unexpected request: ${path}`)
  })
  vi.stubGlobal('fetch', fetchMock)

  const queryClient = new QueryClient({
    defaultOptions: {
      mutations: { retry: false },
      queries: { retry: false },
    },
  })

  render(
    <I18nProvider>
      <QueryClientProvider client={queryClient}>
        <MemoryRouter>
          <LoginPage />
        </MemoryRouter>
      </QueryClientProvider>
    </I18nProvider>,
  )
}

describe('LoginPage account default', () => {
  beforeEach(() => {
    window.localStorage.setItem('mova.interfaceLanguage', 'en-US')
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('leaves the account empty for regular sign in', async () => {
    renderLoginPage(false)

    expect(await screen.findByRole('heading', { name: 'Sign in to Mova' })).toBeInTheDocument()
    expect(screen.getByRole('textbox', { name: 'Account' })).toHaveValue('')
  })

  it('defaults the first administrator account to admin during bootstrap', async () => {
    renderLoginPage(true)

    expect(
      await screen.findByRole('heading', { name: 'Create the first admin account' }),
    ).toBeInTheDocument()
    expect(screen.getByRole('textbox', { name: 'Account' })).toHaveValue('admin')
  })
})
