import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { useState } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { GlassDialog } from '.'

const DialogFixture = ({ onClose = vi.fn() }: { onClose?: () => void }) => {
  const [isOpen, setIsOpen] = useState(false)

  return (
    <>
      <button onClick={() => setIsOpen(true)} type="button">
        Open
      </button>
      <GlassDialog
        ariaLabel="Example dialog"
        closeLabel="Close example dialog"
        isOpen={isOpen}
        onClose={() => {
          onClose()
          setIsOpen(false)
        }}
      >
        <button type="button">First action</button>
        <button type="button">Last action</button>
      </GlassDialog>
    </>
  )
}

describe('GlassDialog', () => {
  it('locks document scrolling, focuses dialog content, and restores trigger focus', async () => {
    render(<DialogFixture />)
    const trigger = screen.getByRole('button', { name: 'Open' })

    trigger.focus()
    fireEvent.click(trigger)

    await waitFor(() => expect(screen.getByRole('button', { name: 'First action' })).toHaveFocus())
    expect(document.body.style.overflow).toBe('hidden')

    fireEvent.keyDown(window, { key: 'Escape' })

    await waitFor(() => expect(trigger).toHaveFocus())
    expect(document.body.style.overflow).toBe('')
  })

  it('keeps keyboard focus inside the active dialog', async () => {
    render(<DialogFixture />)
    fireEvent.click(screen.getByRole('button', { name: 'Open' }))
    const first = await screen.findByRole('button', { name: 'First action' })
    const last = screen.getByRole('button', { name: 'Last action' })

    last.focus()
    fireEvent.keyDown(window, { key: 'Tab' })
    expect(first).toHaveFocus()

    first.focus()
    fireEvent.keyDown(window, { key: 'Tab', shiftKey: true })
    expect(last).toHaveFocus()
  })

  it('does not reset focus when callback and disabled props change while open', async () => {
    const firstOnClose = vi.fn()
    const secondOnClose = vi.fn()
    const { rerender } = render(
      <GlassDialog
        ariaLabel="Example dialog"
        closeLabel="Close example dialog"
        isOpen
        onClose={firstOnClose}
      >
        <button type="button">First action</button>
        <button type="button">Last action</button>
      </GlassDialog>,
    )
    const last = screen.getByRole('button', { name: 'Last action' })

    await waitFor(() => expect(screen.getByRole('button', { name: 'First action' })).toHaveFocus())
    last.focus()

    rerender(
      <GlassDialog
        ariaLabel="Example dialog"
        closeLabel="Close example dialog"
        isCloseDisabled
        isOpen
        onClose={secondOnClose}
      >
        <button type="button">First action</button>
        <button type="button">Last action</button>
      </GlassDialog>,
    )

    await new Promise((resolve) => window.requestAnimationFrame(resolve))
    expect(last).toHaveFocus()

    fireEvent.keyDown(window, { key: 'Escape' })
    expect(firstOnClose).not.toHaveBeenCalled()
    expect(secondOnClose).not.toHaveBeenCalled()

    rerender(
      <GlassDialog
        ariaLabel="Example dialog"
        closeLabel="Close example dialog"
        isOpen
        onClose={secondOnClose}
      >
        <button type="button">First action</button>
        <button type="button">Last action</button>
      </GlassDialog>,
    )
    fireEvent.keyDown(window, { key: 'Escape' })

    expect(secondOnClose).toHaveBeenCalledTimes(1)
    expect(firstOnClose).not.toHaveBeenCalled()
  })
})
