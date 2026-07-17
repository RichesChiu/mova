import type { ReactNode } from 'react'
import { NotificationCenter } from './notification-center'

interface DashboardPageHeaderProps {
  children: ReactNode
  className?: string
}

export const DashboardPageHeader = ({ children, className }: DashboardPageHeaderProps) => {
  const headerClassName = ['home-dashboard-page-header', className].filter(Boolean).join(' ')

  return (
    <header className={headerClassName}>
      <div className="home-dashboard-page-header__content">{children}</div>
      <NotificationCenter />
    </header>
  )
}
