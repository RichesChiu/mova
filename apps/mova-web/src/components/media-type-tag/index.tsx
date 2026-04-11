const MEDIA_TYPE_LABELS: Record<string, string> = {
  episode: 'Episode',
  movie: 'Movie',
  series: 'Series',
}

interface MediaTypeTagProps {
  mediaType: string
}

export const MediaTypeTag = ({ mediaType }: MediaTypeTagProps) => {
  const normalizedMediaType = mediaType.trim().toLowerCase()
  const label =
    MEDIA_TYPE_LABELS[normalizedMediaType] ??
    (normalizedMediaType
      ? `${normalizedMediaType[0]?.toUpperCase() ?? ''}${normalizedMediaType.slice(1)}`
      : 'Media')

  return <span className={`media-type-tag media-type-tag--${normalizedMediaType}`}>{label}</span>
}
