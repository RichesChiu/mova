import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useOutletContext } from 'react-router-dom'
import { changeOwnPassword } from '../../api/client'
import type { AppShellOutletContext } from '../../components/app-shell'
import { ChangePasswordModal } from '../../components/change-password-modal'
import { StatusPill } from '../../components/status-pill'

export const ProfilePage = () => {
  const { currentUser } = useOutletContext<AppShellOutletContext>()
  const queryClient = useQueryClient()
  const [isChangePasswordOpen, setIsChangePasswordOpen] = useState(false)
  const [successMessage, setSuccessMessage] = useState<string | null>(null)
  const roleLabel = currentUser.role === 'admin' ? 'Admin' : 'Viewer'

  const changePasswordMutation = useMutation({
    mutationFn: changeOwnPassword,
    onMutate: () => {
      setSuccessMessage(null)
    },
    onSuccess: async () => {
      // 服务端改密后会轮换 session cookie，这里顺手刷新当前用户查询，确保前端状态和新会话保持一致。
      await queryClient.invalidateQueries({ queryKey: ['current-user'] })
      setSuccessMessage('Password updated.')
    },
  })

  return (
    <div className="page-stack profile-page">
      <section className="catalog-block">
        <div className="catalog-block__header">
          <div>
            <p className="eyebrow">Profile</p>
            <h2>{currentUser.username}</h2>
          </div>
        </div>

        <div className="profile-page__identity">
          <span className="summary-card__label">Role</span>
          <StatusPill status={roleLabel} />
        </div>
      </section>

      <section className="catalog-block profile-page__password-block">
        <div className="catalog-block__header">
          <div>
            <p className="eyebrow">Security</p>
            <h3>Reset Password</h3>
          </div>
        </div>

        <div className="profile-page__password-actions">
          <button
            className="button button--primary"
            onClick={() => setIsChangePasswordOpen(true)}
            type="button"
          >
            Reset Password
          </button>
          {successMessage ? <p className="muted">{successMessage}</p> : null}
        </div>
      </section>

      <ChangePasswordModal
        error={
          changePasswordMutation.error instanceof Error
            ? changePasswordMutation.error.message
            : null
        }
        isOpen={isChangePasswordOpen}
        isSubmitting={changePasswordMutation.isPending}
        onClose={() => {
          setIsChangePasswordOpen(false)
          changePasswordMutation.reset()
        }}
        onSubmit={(input) => changePasswordMutation.mutateAsync(input)}
      />
    </div>
  )
}
