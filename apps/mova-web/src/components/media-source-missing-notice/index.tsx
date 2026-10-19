import { useI18n } from '../../i18n'
import './media-source-missing-notice.scss'

interface MediaSourceMissingNoticeProps {
  rescanQueued: boolean
  title: string
}

/**
 * Shown in place of a movie or episode whose files were not found on disk.
 * The entry itself is never removed here: the scan the server queued decides
 * whether it disappears, moves, or only loses some versions.
 */
export const MediaSourceMissingNotice = ({
  rescanQueued,
  title,
}: MediaSourceMissingNoticeProps) => {
  const { l } = useI18n()

  return (
    <section aria-live="polite" className="media-source-missing-notice" role="status">
      <h3 className="media-source-missing-notice__title">{l('Resource no longer exists')}</h3>
      <p className="media-source-missing-notice__text">
        {l('The files of {{title}} were not found on disk.', { title })}
      </p>
      <p className="media-source-missing-notice__text">
        {rescanQueued
          ? l(
              'The library is being rescanned. This entry is updated or removed when the scan finishes.',
            )
          : l('Rescan the library after restoring the files to bring this entry back.')}
      </p>
    </section>
  )
}
