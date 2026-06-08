import type { ReactNode } from 'react'

interface GlassPopoverProps {
  children: ReactNode
  className?: string
  description?: string | null
  footer?: ReactNode
  role?: 'dialog' | 'menu' | 'tooltip'
  title?: string | null
}

export const GlassPopover = ({
  children,
  className = '',
  description = null,
  footer = null,
  role = 'dialog',
  title = null,
}: GlassPopoverProps) => {
  const combinedClassName = ['glass-popover', className].filter(Boolean).join(' ')

  return (
    <div className={combinedClassName} role={role}>
      {title || description ? (
        <div className="glass-popover__header">
          {title ? <strong>{title}</strong> : null}
          {description ? <span className="muted">{description}</span> : null}
        </div>
      ) : null}

      <div className="glass-popover__body">{children}</div>

      {footer ? <div className="glass-popover__footer">{footer}</div> : null}
    </div>
  )
}
