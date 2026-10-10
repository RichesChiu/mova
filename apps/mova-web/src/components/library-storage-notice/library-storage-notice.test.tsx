import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Library } from '../../api/types'
import { I18nProvider } from '../../i18n'
import { LibraryStorageNotice } from '.'

const unavailableLibrary: Library = {
  id: 7,
  name: 'TV',
  description: null,
  metadata_language: 'zh-CN',
  root_path: '/media/TV',
  storage_status: 'unavailable',
  storage_issue: {
    reason_code: 'storage_not_connected',
    mount_point: '/media',
    expected_fs_type: 'cifs',
    expected_source: '//192.168.50.3/media',
    actual_fs_type: 'ext4',
    actual_source: '/dev/sda2',
    diagnostic_message: null,
  },
  storage_unavailable_since: '2026-10-10T08:00:00Z',
  created_at: '2026-08-15T00:00:00Z',
  updated_at: '2026-08-15T00:00:00Z',
}

const renderNotice = (props: Partial<Parameters<typeof LibraryStorageNotice>[0]> = {}) =>
  render(
    <I18nProvider>
      <LibraryStorageNotice canManageLibraries library={unavailableLibrary} {...props} />
    </I18nProvider>,
  )

describe('LibraryStorageNotice', () => {
  beforeEach(() => {
    window.localStorage.setItem('mova.interfaceLanguage', 'en-US')
  })

  afterEach(() => {
    window.localStorage.clear()
  })

  it('shows admins what is connected and lets them delete the library', () => {
    const onDeleteLibrary = vi.fn()
    renderNotice({ onDeleteLibrary })

    expect(screen.getByRole('alert')).toHaveTextContent('Library storage unavailable')
    expect(screen.getByText('cifs //192.168.50.3/media')).toBeInTheDocument()
    expect(screen.getByText('ext4 /dev/sda2')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Delete Library' }))
    expect(onDeleteLibrary).toHaveBeenCalledWith(unavailableLibrary)
  })

  it('tells viewers only that the library is paused', () => {
    renderNotice({
      canManageLibraries: false,
      library: { ...unavailableLibrary, storage_issue: null },
      onDeleteLibrary: vi.fn(),
    })

    expect(screen.getByText('This library is temporarily unavailable.')).toBeInTheDocument()
    expect(screen.getByText('Existing media and playback history are kept.')).toBeInTheDocument()
    expect(screen.queryByText('Mount point')).not.toBeInTheDocument()
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })

  it('renders nothing while the storage is available', () => {
    const { container } = renderNotice({
      library: { ...unavailableLibrary, storage_status: 'available', storage_issue: null },
    })

    expect(container).toBeEmptyDOMElement()
  })
})
