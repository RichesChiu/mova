import { type ReactNode, useLayoutEffect, useRef, useState } from 'react'

type ImageResult = {
  src: string
  state: 'loaded' | 'failed'
}

export const LibraryDetailTileArtwork = ({
  alt,
  children,
  placeholderLabel,
  src,
}: {
  alt: string
  children?: ReactNode
  placeholderLabel: string
  src: string | null
}) => {
  const imageRef = useRef<HTMLImageElement | null>(null)
  const [imageResult, setImageResult] = useState<ImageResult | null>(null)
  const imageState = src ? (imageResult?.src === src ? imageResult.state : 'loading') : 'idle'
  const shouldRenderImage = Boolean(src) && imageState !== 'failed'
  const shouldShowPlaceholder = !src || imageState !== 'loaded'

  const setResult = (state: ImageResult['state']) => {
    if (!src) {
      return
    }

    setImageResult((current) =>
      current?.src === src && current.state === state ? current : { src, state },
    )
  }

  useLayoutEffect(() => {
    const image = imageRef.current
    if (!src || !image?.complete) {
      return
    }

    const state = image.naturalWidth > 0 ? 'loaded' : 'failed'
    setImageResult((current) =>
      current?.src === src && current.state === state ? current : { src, state },
    )
  }, [src])

  return (
    <div className="library-detail-tile__poster">
      {shouldShowPlaceholder ? (
        <div className="library-detail-tile__placeholder">
          <span>{placeholderLabel}</span>
        </div>
      ) : null}
      {shouldRenderImage ? (
        <img
          alt={alt}
          className={
            imageState === 'loaded'
              ? 'library-detail-tile__image library-detail-tile__image--loaded'
              : 'library-detail-tile__image'
          }
          loading="lazy"
          onError={() => setResult('failed')}
          onLoad={() => setResult('loaded')}
          ref={imageRef}
          src={src ?? undefined}
        />
      ) : null}
      {children}
    </div>
  )
}
