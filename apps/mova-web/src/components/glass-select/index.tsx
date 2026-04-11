import { useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'

export interface GlassSelectOption {
  disabled?: boolean
  label: string
  value: string
}

interface GlassSelectProps {
  ariaLabel?: string
  compact?: boolean
  disabled?: boolean
  onChange: (value: string) => void
  options: GlassSelectOption[]
  value: string
}

export const GlassSelect = ({
  ariaLabel,
  compact = false,
  disabled = false,
  onChange,
  options,
  value,
}: GlassSelectProps) => {
  const [isOpen, setIsOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement | null>(null)
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const menuRef = useRef<HTMLDivElement | null>(null)
  const [menuStyle, setMenuStyle] = useState<{
    left: number
    maxHeight: number
    top: number
    width: number
  } | null>(null)

  const selectedOption = useMemo(
    () => options.find((option) => option.value === value) ?? options[0],
    [options, value],
  )

  useEffect(() => {
    const handleOutsidePointer = (event: MouseEvent) => {
      const rootElement = rootRef.current
      if (!rootElement) {
        return
      }

      if (!(event.target instanceof Node)) {
        return
      }

      const menuElement = menuRef.current

      if (!rootElement.contains(event.target) && !menuElement?.contains(event.target)) {
        setIsOpen(false)
      }
    }

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setIsOpen(false)
      }
    }

    window.addEventListener('mousedown', handleOutsidePointer)
    window.addEventListener('keydown', handleEscape)

    return () => {
      window.removeEventListener('mousedown', handleOutsidePointer)
      window.removeEventListener('keydown', handleEscape)
    }
  }, [])

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const updateMenuPosition = () => {
      const triggerElement = triggerRef.current
      if (!triggerElement) {
        return
      }

      const rect = triggerElement.getBoundingClientRect()
      const viewportHeight = window.innerHeight
      const preferredTop = rect.bottom + 8
      const availableBottom = viewportHeight - preferredTop - 16
      const availableTop = rect.top - 16
      const shouldOpenUpward = availableBottom < 180 && availableTop > availableBottom
      const maxHeight = Math.max(
        160,
        shouldOpenUpward ? availableTop - 8 : viewportHeight - preferredTop - 16,
      )

      setMenuStyle({
        left: rect.left,
        maxHeight,
        top: shouldOpenUpward ? Math.max(12, rect.top - maxHeight - 8) : preferredTop,
        width: rect.width,
      })
    }

    updateMenuPosition()
    window.addEventListener('resize', updateMenuPosition)
    window.addEventListener('scroll', updateMenuPosition, true)

    return () => {
      window.removeEventListener('resize', updateMenuPosition)
      window.removeEventListener('scroll', updateMenuPosition, true)
    }
  }, [isOpen])

  return (
    <div
      className={
        compact
          ? isOpen
            ? 'glass-select glass-select--compact glass-select--open'
            : 'glass-select glass-select--compact'
          : isOpen
            ? 'glass-select glass-select--open'
            : 'glass-select'
      }
      ref={rootRef}
    >
      <button
        aria-expanded={isOpen}
        aria-haspopup="listbox"
        aria-label={ariaLabel}
        className="glass-select__trigger"
        disabled={disabled}
        onClick={() => setIsOpen((open) => !open)}
        ref={triggerRef}
        type="button"
      >
        <span className="glass-select__value">{selectedOption?.label ?? ''}</span>
        <span aria-hidden className="glass-select__caret" />
      </button>

      {isOpen && menuStyle
        ? createPortal(
            <div
              className="glass-select__menu glass-select__menu--portal"
              ref={menuRef}
              role="listbox"
              style={{
                left: menuStyle.left,
                maxHeight: menuStyle.maxHeight,
                top: menuStyle.top,
                width: menuStyle.width,
              }}
            >
              {options.map((option) => {
                const isSelected = option.value === value
                const optionClassName = option.disabled
                  ? 'glass-select__option glass-select__option--disabled'
                  : isSelected
                    ? 'glass-select__option glass-select__option--selected'
                    : 'glass-select__option'

                return (
                  <button
                    aria-selected={isSelected}
                    className={optionClassName}
                    disabled={option.disabled}
                    key={option.value}
                    onClick={() => {
                      if (option.disabled) {
                        return
                      }
                      onChange(option.value)
                      setIsOpen(false)
                    }}
                    role="option"
                    type="button"
                  >
                    <span className="glass-select__option-label">{option.label}</span>
                  </button>
                )
              })}
            </div>,
            document.body,
          )
        : null}
    </div>
  )
}
