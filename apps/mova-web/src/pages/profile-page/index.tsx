import { useOutletContext } from 'react-router-dom'
import type { AppShellOutletContext } from '../../components/app-shell'

export const ProfilePage = () => {
  const { currentUser, libraries } = useOutletContext<AppShellOutletContext>()

  const accessibleLibraryNames =
    currentUser.role === 'admin'
      ? ['All libraries']
      : libraries
          .filter((library) => currentUser.library_ids.includes(library.id))
          .map((library) => library.name)
  const accessibleLibraryCount =
    currentUser.role === 'admin' ? libraries.length : accessibleLibraryNames.length

  return (
    <div className="page-stack profile-page">
      <section className="catalog-block">
        <div className="catalog-block__header">
          <div>
            <p className="eyebrow">Profile</p>
            <h2>{currentUser.username}</h2>
          </div>
        </div>

        <div className="summary-grid">
          <article className="summary-card">
            <span className="summary-card__label">Account</span>
            <strong>{currentUser.is_enabled ? 'Active' : 'Disabled'}</strong>
          </article>

          <article className="summary-card">
            <span className="summary-card__label">Libraries</span>
            <strong>{accessibleLibraryCount}</strong>
          </article>
        </div>

        <p className="muted">
          当前个人页只保留账号相关内容。观看历史已经前置到首页，后续这里再补密码修改、语言和播放偏好。
        </p>

        <div className="catalog-block__empty">
          <p className="muted">Accessible: {accessibleLibraryNames.join(', ')}</p>
        </div>
      </section>
    </div>
  )
}
