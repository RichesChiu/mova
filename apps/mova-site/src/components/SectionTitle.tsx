import type { ReactNode } from 'react'
import './SectionTitle.scss'

export function SectionTitle({ id, title }: { id?: string; title: ReactNode }) {
  return (
    <div className="section-title">
      <h2 id={id}>{title}</h2>
      <span aria-hidden="true" />
    </div>
  )
}
