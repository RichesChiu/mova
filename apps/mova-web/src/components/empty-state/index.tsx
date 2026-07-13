import type { ReactNode } from 'react'

interface EmptyStateProps {
  action?: ReactNode
  className?: string
  description: ReactNode
  title: ReactNode
}

export const EmptyState = ({ action, className, description, title }: EmptyStateProps) => {
  const classes = ['empty-state', className].filter(Boolean).join(' ')

  return (
    <section className={classes} role="status">
      <div className="empty-state__copy">
        <h3>{title}</h3>
        <p>{description}</p>
      </div>
      {action ? <div className="empty-state__action">{action}</div> : null}
    </section>
  )
}
