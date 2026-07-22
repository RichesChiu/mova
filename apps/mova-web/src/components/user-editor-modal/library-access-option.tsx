import type { Library } from '../../api/types'
import { useI18n } from '../../i18n'
import { HoverTooltip } from '../hover-tooltip'

interface LibraryAccessOptionProps {
  checked: boolean
  library: Library
  onToggle: () => void
}

export const LibraryAccessOption = ({ checked, library, onToggle }: LibraryAccessOptionProps) => {
  const { l } = useI18n()

  return (
    <label className="user-editor-modal__access-chip">
      <input
        aria-label={
          checked
            ? l('Remove access to {{name}}', { name: library.name })
            : l('Grant access to {{name}}', { name: library.name })
        }
        className="user-editor-modal__access-checkbox"
        checked={checked}
        onChange={onToggle}
        type="checkbox"
      />

      <HoverTooltip className="user-editor-modal__access-chip-text-wrap" content={library.name}>
        <span className="user-editor-modal__access-chip-title">{library.name}</span>
      </HoverTooltip>
    </label>
  )
}
