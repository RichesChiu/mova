import { useEffect, useState } from 'react'
import type { Library } from '../../api/types'
import { useI18n } from '../../i18n'
import './library-actions-menu.scss'

type LibraryActionIconName = 'edit' | 'scan' | 'trash'

interface LibraryActionsMenuProps {
  className?: string
  isDeleteDisabled?: boolean
  isDeletePending?: boolean
  isScanDisabled?: boolean
  isScanPending?: boolean
  library: Library
  onDeleteLibrary: (library: Library) => void
  onEditLibrary: (library: Library) => void
  onScanLibrary: (library: Library) => void
}

const LibraryActionIcon = ({ name }: { name: LibraryActionIconName }) => (
  <svg
    aria-hidden="true"
    fill="none"
    focusable="false"
    stroke="currentColor"
    strokeLinecap="round"
    strokeLinejoin="round"
    strokeWidth="1.8"
    viewBox="0 0 24 24"
  >
    {name === 'edit' ? (
      <>
        <path d="M12 20h8" />
        <path d="m16.5 3.5 4 4L8 20l-4.5.5L4 16 16.5 3.5Z" />
      </>
    ) : name === 'scan' ? (
      <>
        <path d="M4 7V5.5A1.5 1.5 0 0 1 5.5 4H7" />
        <path d="M17 4h1.5A1.5 1.5 0 0 1 20 5.5V7" />
        <path d="M20 17v1.5a1.5 1.5 0 0 1-1.5 1.5H17" />
        <path d="M7 20H5.5A1.5 1.5 0 0 1 4 18.5V17" />
        <path d="M7 12h10" />
      </>
    ) : (
      <>
        <path d="M4 6h16" />
        <path d="M9 6V4h6v2" />
        <path d="m6.5 6 .8 14h9.4l.8-14" />
        <path d="M10 10v6" />
        <path d="M14 10v6" />
      </>
    )}
  </svg>
)

export const LibraryActionsMenu = ({
  className,
  isDeleteDisabled = false,
  isDeletePending = false,
  isScanDisabled = false,
  isScanPending = false,
  library,
  onDeleteLibrary,
  onEditLibrary,
  onScanLibrary,
}: LibraryActionsMenuProps) => {
  const { l } = useI18n()
  const [isOpen, setIsOpen] = useState(false)

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const handlePointerDown = (event: MouseEvent) => {
      if (
        event.target instanceof Element &&
        event.target.closest(`[data-library-actions-menu="${library.id}"]`)
      ) {
        return
      }

      setIsOpen(false)
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setIsOpen(false)
      }
    }

    document.addEventListener('mousedown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)

    return () => {
      document.removeEventListener('mousedown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [isOpen, library.id])

  const closeMenu = () => setIsOpen(false)

  return (
    <div
      className={['library-actions-menu', className].filter(Boolean).join(' ')}
      data-library-actions-menu={library.id}
      data-state={isOpen ? 'open' : 'closed'}
    >
      <button
        aria-expanded={isOpen}
        aria-haspopup="menu"
        aria-label={l('Open library actions menu')}
        className="library-actions-menu__trigger"
        onClick={() => setIsOpen((current) => !current)}
        type="button"
      >
        <span />
        <span />
        <span />
      </button>

      {isOpen ? (
        <div
          aria-label={l('Library actions')}
          className="library-actions-menu__popover glass-popover-surface floating-transition"
          data-state="open"
          role="menu"
        >
          <button
            className="library-actions-menu__item"
            onClick={() => {
              closeMenu()
              onEditLibrary(library)
            }}
            role="menuitem"
            type="button"
          >
            <LibraryActionIcon name="edit" />
            <span>{l('Edit Library')}</span>
          </button>
          <button
            className="library-actions-menu__item"
            disabled={isScanDisabled}
            onClick={() => {
              closeMenu()
              onScanLibrary(library)
            }}
            role="menuitem"
            type="button"
          >
            <LibraryActionIcon name="scan" />
            <span>{isScanPending ? l('Triggering…') : l('Scan Library')}</span>
          </button>
          <button
            className="library-actions-menu__item library-actions-menu__item--danger"
            disabled={isDeleteDisabled}
            onClick={() => {
              closeMenu()
              onDeleteLibrary(library)
            }}
            role="menuitem"
            type="button"
          >
            <LibraryActionIcon name="trash" />
            <span>{isDeletePending ? l('Deleting…') : l('Delete Library')}</span>
          </button>
        </div>
      ) : null}
    </div>
  )
}
