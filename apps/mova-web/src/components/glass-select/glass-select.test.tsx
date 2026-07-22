import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { GlassSelect } from './index'

describe('GlassSelect', () => {
  it('keeps compact sizing on the portaled option menu', async () => {
    render(
      <GlassSelect
        ariaLabel="Select season"
        compact
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
      expect(screen.getByRole('listbox')).toHaveClass('glass-select__menu--compact')
    })
  })
})
