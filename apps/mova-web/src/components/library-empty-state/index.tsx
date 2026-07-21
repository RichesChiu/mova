import { Link } from 'react-router-dom'
import { useI18n } from '../../i18n'
import { HomeIcon } from '../../pages/home-page/home-icons'
import { EmptyState } from '../empty-state'

interface LibraryEmptyStateProps {
  canManageLibraries: boolean
}

export const LibraryEmptyState = ({ canManageLibraries }: LibraryEmptyStateProps) => {
  const { l } = useI18n()

  return (
    <EmptyState
      action={
        canManageLibraries ? (
          <Link
            className="button button--primary button--toolbar library-empty-state__settings-link"
            to="/settings"
          >
            <HomeIcon className="button__icon" name="settings" />
            {l('Open Server Settings')}
          </Link>
        ) : null
      }
      className="library-empty-state"
      description={l('No media libraries are available to your account yet.')}
      title={l('No libraries yet.')}
    />
  )
}
