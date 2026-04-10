import { useEffect, useMemo, useRef, useState } from 'react'

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

      if (!rootElement.contains(event.target)) {
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
        type="button"
      >
        <span className="glass-select__value">{selectedOption?.label ?? ''}</span>
        <span aria-hidden className="glass-select__caret" />
      </button>

      {isOpen ? (
        <div className="glass-select__menu" role="listbox">
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
        </div>
      ) : null}
    </div>
  )
}
