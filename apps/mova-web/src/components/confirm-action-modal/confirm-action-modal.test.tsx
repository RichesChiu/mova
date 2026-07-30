import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../../i18n'
import { ConfirmActionModal } from '.'

describe('ConfirmActionModal', () => {
  beforeEach(() => {
    window.localStorage.setItem('mova.interfaceLanguage', 'en-US')
  })

  it('keeps long confirmation copy and both actions inside the shared dialog structure', () => {
    const onClose = vi.fn()
    const onConfirm = vi.fn()
    const description = `Delete "${'movies'.repeat(
      24,
    )}"? This removes the library record and scan history.`

    render(
      <I18nProvider>
        <ConfirmActionModal
          confirmLabel="Delete Library"
          description={description}
          error={null}
          isOpen
          isSubmitting={false}
          onClose={onClose}
          onConfirm={onConfirm}
          title="Delete library"
        />
      </I18nProvider>,
    )

    expect(screen.getByRole('dialog', { name: 'Delete library' })).toBeInTheDocument()
    expect(screen.getByText(description)).toHaveClass('confirm-action-modal__description')

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    fireEvent.click(screen.getByRole('button', { name: 'Delete Library' }))

    expect(onClose).toHaveBeenCalledTimes(1)
    expect(onConfirm).toHaveBeenCalledTimes(1)
  })
})
