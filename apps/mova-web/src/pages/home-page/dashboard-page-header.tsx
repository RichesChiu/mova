import type { ReactNode } from 'react'
import { useI18n } from '../../i18n'
import { HomeIcon } from './home-icons'

interface DashboardPageHeaderProps {
  children: ReactNode
  className?: string
}

export const DashboardPageHeader = ({ children, className }: DashboardPageHeaderProps) => {
  const { l } = useI18n()
  const headerClassName = ['home-dashboard-page-header', className].filter(Boolean).join(' ')

  return (
    <header className={headerClassName}>
      <div className="home-dashboard-page-header__content">{children}</div>
      <button
        className="home-icon-button home-dashboard-page-header__notification"
        type="button"
        aria-label={l('Notifications')}
      >
        <HomeIcon name="bell" />
      </button>
    </header>
  )
}
