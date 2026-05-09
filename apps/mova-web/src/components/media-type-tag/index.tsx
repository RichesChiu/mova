import { translateCurrent } from '../../i18n'
import { formatMediaTypeLabel } from '../../lib/media-type-label'

interface MediaTypeTagProps {
  mediaType: string
}

export const MediaTypeTag = ({ mediaType }: MediaTypeTagProps) => {
  const normalizedMediaType = mediaType.trim().toLowerCase()
  const label = formatMediaTypeLabel(mediaType, translateCurrent)

  return <span className={`media-type-tag media-type-tag--${normalizedMediaType}`}>{label}</span>
}
