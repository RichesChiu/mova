import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { GlassSelect } from './index'

describe('GlassSelect', () => {
  it('uses the shared select menu style for every instance', async () => {
    render(
      <GlassSelect
        ariaLabel="Select season"
        onChange={vi.fn()}
        options={[
          { label: 'Season 1', value: '1' },
          { label: 'Season 2', value: '2' },
        ]}
        value="1"
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Select season' }))

    await waitFor(() => {
      expect(screen.getByRole('listbox')).toHaveClass('glass-select__menu')
      expect(screen.getByRole('listbox')).not.toHaveClass('glass-select__menu--compact')
    })
  })

  it('toggles one centered caret without changing the trigger layout', () => {
    render(
      <GlassSelect
        ariaLabel="Select season"
        onChange={vi.fn()}
        options={[{ label: 'Season 1', value: '1' }]}
        value="1"
      />,
    )

    const trigger = screen.getByRole('button', { name: 'Select season' })
    const root = trigger.closest('.glass-select')

    expect(trigger.querySelectorAll('.glass-select__caret')).toHaveLength(1)
    expect(root).not.toHaveClass('glass-select--open')

    fireEvent.click(trigger)

    expect(root).toHaveClass('glass-select--open')
  })
})
