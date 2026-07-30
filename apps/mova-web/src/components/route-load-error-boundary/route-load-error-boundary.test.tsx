import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { RouteLoadErrorBoundary } from '.'

const ThrowRoute = (): ReactNode => {
  throw new Error('chunk unavailable')
}

describe('RouteLoadErrorBoundary', () => {
  it('renders a reload action when a lazy route rejects', () => {
    const onReload = vi.fn()
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)

    render(
      <RouteLoadErrorBoundary
        description="Reload to continue."
        onReload={onReload}
        reloadLabel="Reload page"
        resetKey="/libraries"
        title="Page unavailable"
      >
        <ThrowRoute />
      </RouteLoadErrorBoundary>,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Reload page' }))
    expect(onReload).toHaveBeenCalledTimes(1)
    consoleError.mockRestore()
  })

  it('resets the captured failure when navigation changes the reset key', () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const { rerender } = render(
      <RouteLoadErrorBoundary
        description="Reload to continue."
        onReload={vi.fn()}
        reloadLabel="Reload page"
        resetKey="/libraries"
        title="Page unavailable"
      >
        <ThrowRoute />
      </RouteLoadErrorBoundary>,
    )

    rerender(
      <RouteLoadErrorBoundary
        description="Reload to continue."
        onReload={vi.fn()}
        reloadLabel="Reload page"
        resetKey="/"
        title="Page unavailable"
      >
        <p>Home route</p>
      </RouteLoadErrorBoundary>,
    )

    expect(screen.getByText('Home route')).toBeInTheDocument()
    consoleError.mockRestore()
  })
})
