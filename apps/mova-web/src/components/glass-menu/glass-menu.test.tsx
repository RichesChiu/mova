import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { useState } from 'react'
import { describe, expect, it } from 'vitest'
import { GlassMenu } from '.'

const MenuFixture = () => {
  const [isOpen, setIsOpen] = useState(false)

  return (
    <GlassMenu
      ariaLabel="Actions"
      isOpen={isOpen}
      onOpenChange={setIsOpen}
      trigger={(triggerProps) => (
        <button {...triggerProps} type="button">
          Open actions
        </button>
      )}
    >
      {(closeMenu) => (
        <>
          <button onClick={closeMenu} role="menuitem" type="button">
            Edit
          </button>
          <button role="menuitem" type="button">
            Delete
          </button>
        </>
      )}
    </GlassMenu>
  )
}

describe('GlassMenu', () => {
  it('focuses the first item, dismisses on Escape, and restores focus to the trigger', async () => {
    render(<MenuFixture />)
    const trigger = screen.getByRole('button', { name: 'Open actions' })
    fireEvent.click(trigger)
    await waitFor(() => expect(screen.getByRole('menuitem', { name: 'Edit' })).toHaveFocus())

    fireEvent.keyDown(document, { key: 'Escape' })

    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
    expect(trigger).toHaveFocus()
  })

  it('supports arrow-key navigation between menu items', () => {
    render(<MenuFixture />)
    fireEvent.click(screen.getByRole('button', { name: 'Open actions' }))
    const edit = screen.getByRole('menuitem', { name: 'Edit' })
    const remove = screen.getByRole('menuitem', { name: 'Delete' })
    edit.focus()

    fireEvent.keyDown(document, { key: 'ArrowDown' })
    expect(remove).toHaveFocus()
    fireEvent.keyDown(document, { key: 'ArrowDown' })
    expect(edit).toHaveFocus()
  })

  it.each([
    { shiftKey: false },
    { shiftKey: true },
  ])('closes when keyboard focus leaves with Tab (shift: $shiftKey)', async ({ shiftKey }) => {
    render(<MenuFixture />)
    fireEvent.click(screen.getByRole('button', { name: 'Open actions' }))
    await waitFor(() => expect(screen.getByRole('menuitem', { name: 'Edit' })).toHaveFocus())

    fireEvent.keyDown(document, { key: 'Tab', shiftKey })

    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
  })
})
