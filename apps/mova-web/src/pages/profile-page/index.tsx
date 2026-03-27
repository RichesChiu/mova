import { useMutation, useQueryClient } from '@tanstack/react-query'
import { type FormEvent, useState } from 'react'
import { useOutletContext } from 'react-router-dom'
import { changeOwnPassword } from '../../api/client'
import type { AppShellOutletContext } from '../../components/app-shell'

export const ProfilePage = () => {
  const { currentUser, libraries } = useOutletContext<AppShellOutletContext>()
  const queryClient = useQueryClient()
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [successMessage, setSuccessMessage] = useState<string | null>(null)

  const accessibleLibraryNames =
    currentUser.role === 'admin'
      ? ['All libraries']
      : libraries
          .filter((library) => currentUser.library_ids.includes(library.id))
          .map((library) => library.name)
  const accessibleLibraryCount =
    currentUser.role === 'admin' ? libraries.length : accessibleLibraryNames.length

  const changePasswordMutation = useMutation({
    mutationFn: changeOwnPassword,
    onMutate: () => {
      setSuccessMessage(null)
    },
    onSuccess: async () => {
      // 服务端改密后会轮换 session cookie，这里顺手刷新当前用户查询，确保前端状态和新会话保持一致。
      await queryClient.invalidateQueries({ queryKey: ['current-user'] })
      setCurrentPassword('')
      setNewPassword('')
      setConfirmPassword('')
      setSuccessMessage('Password updated.')
    },
  })

  const handlePasswordSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    if (newPassword !== confirmPassword) {
      setSuccessMessage(null)
      return
    }

    await changePasswordMutation.mutateAsync({
      current_password: currentPassword,
      new_password: newPassword,
    })
  }

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

        <p className="muted">观看历史已经前置到首页；个人页现在只保留账号和密码修改相关内容。</p>

        <div className="catalog-block__empty">
          <p className="muted">Accessible: {accessibleLibraryNames.join(', ')}</p>
        </div>
      </section>

      <section className="catalog-block profile-page__password-block">
        <div className="catalog-block__header">
          <div>
            <p className="eyebrow">Security</p>
            <h3>Reset Password</h3>
            <p className="muted">修改当前账号密码。成功后服务端会轮换当前登录会话。</p>
          </div>
        </div>

        <form className="profile-page__password-form" onSubmit={handlePasswordSubmit}>
          <label className="field">
            <span>Current Password</span>
            <input
              autoComplete="current-password"
              onChange={(event) => setCurrentPassword(event.target.value)}
              type="password"
              value={currentPassword}
            />
          </label>

          <label className="field">
            <span>New Password</span>
            <input
              autoComplete="new-password"
              onChange={(event) => setNewPassword(event.target.value)}
              type="password"
              value={newPassword}
            />
          </label>

          <label className="field">
            <span>Confirm New Password</span>
            <input
              autoComplete="new-password"
              onChange={(event) => setConfirmPassword(event.target.value)}
              type="password"
              value={confirmPassword}
            />
          </label>

          {newPassword.length > 0 &&
          confirmPassword.length > 0 &&
          newPassword !== confirmPassword ? (
            <p className="callout callout--danger">The new passwords do not match.</p>
          ) : null}

          {changePasswordMutation.isError ? (
            <p className="callout callout--danger">
              {changePasswordMutation.error instanceof Error
                ? changePasswordMutation.error.message
                : 'Failed to update password'}
            </p>
          ) : null}

          {successMessage ? <p className="muted">{successMessage}</p> : null}

          <div className="profile-page__password-actions">
            <button
              className="button button--primary"
              disabled={
                changePasswordMutation.isPending ||
                currentPassword.length === 0 ||
                newPassword.length < 8 ||
                confirmPassword.length < 8 ||
                newPassword !== confirmPassword
              }
              type="submit"
            >
              {changePasswordMutation.isPending ? 'Updating…' : 'Update Password'}
            </button>
          </div>
        </form>
      </section>
    </div>
  )
}
