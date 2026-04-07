import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { Library } from '../../api/types'
import { LibraryEditorModal } from './index'

const library: Library = {
  id: 7,
  name: 'Movies',
  description: '家庭电影库',
  library_type: 'movie',
  metadata_language: 'zh-CN',
  root_path: '/media/movies',
  is_enabled: true,
  created_at: '2026-04-07T00:00:00Z',
  updated_at: '2026-04-07T00:00:00Z',
}

describe('LibraryEditorModal', () => {
  it('submits the expanded library configuration payload', async () => {
    const onClose = vi.fn()
    const onUpdate = vi.fn().mockResolvedValue(undefined)

    render(
      <LibraryEditorModal
        error={null}
        isOpen
        isSubmitting={false}
        library={library}
        onClose={onClose}
        onUpdate={onUpdate}
      />,
    )

    expect(screen.getByRole('button', { name: 'Save Changes' })).toBeDisabled()

    fireEvent.change(screen.getByLabelText('Library Name'), {
      target: { value: 'Cinema' },
    })
    fireEvent.change(screen.getByLabelText('Description'), {
      target: { value: '' },
    })

    fireEvent.click(screen.getByRole('button', { name: 'Library metadata language' }))
    fireEvent.click(screen.getByRole('option', { name: 'English (en-US)' }))

    fireEvent.click(screen.getByLabelText('Enable watcher and automatic background sync'))
    fireEvent.click(screen.getByRole('button', { name: 'Save Changes' }))

    await waitFor(() => {
      expect(onUpdate).toHaveBeenCalledWith(7, {
        name: 'Cinema',
        description: null,
        metadata_language: 'en-US',
        is_enabled: false,
      })
      expect(onClose).toHaveBeenCalled()
    })
  })
})
