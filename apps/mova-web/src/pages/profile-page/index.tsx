import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { useOutletContext } from 'react-router-dom'
import { changeOwnPassword, updateOwnProfile } from '../../api/client'
import type { AppShellOutletContext } from '../../components/app-shell'
import { ChangePasswordModal } from '../../components/change-password-modal'
import { GlassSelect, type GlassSelectOption } from '../../components/glass-select'
import { StatusPill } from '../../components/status-pill'
import { useI18n } from '../../i18n'
import {
  INTERFACE_LANGUAGES,
  readStoredThemePreference,
  setThemePreference,
} from '../../lib/preferences'
import { THEMES } from '../../lib/theme'
import { getUserDisplayName } from '../../lib/user-identity'

export const ProfilePage = () => {
  const { currentUser } = useOutletContext<AppShellOutletContext>()
  const queryClient = useQueryClient()
  const { language: interfaceLanguage, l, setLanguage } = useI18n()
  const [isChangePasswordOpen, setIsChangePasswordOpen] = useState(false)
  const [isEditingNickname, setIsEditingNickname] = useState(false)
  const [nicknameDraft, setNicknameDraft] = useState(currentUser.nickname)
  const [themePreference, setThemePreferenceState] = useState(() => readStoredThemePreference())
  const [successMessage, setSuccessMessage] = useState<string | null>(null)
  const nicknameInputRef = useRef<HTMLInputElement | null>(null)
  const roleLabel = currentUser.is_primary_admin
    ? l('Primary Admin')
    : currentUser.role === 'admin'
      ? l('Administrator')
      : l('Member')
  const nickname = getUserDisplayName(currentUser)
  const interfaceLanguageOptions: GlassSelectOption[] = [
    { label: l('English'), value: INTERFACE_LANGUAGES.english },
    { label: l('Chinese'), value: INTERFACE_LANGUAGES.chinese },
  ]
  const themeOptions: GlassSelectOption[] = [
    { label: l('Dark'), value: THEMES.noir },
    { label: l('Light'), value: THEMES.frost },
  ]

  useEffect(() => {
    setNicknameDraft(currentUser.nickname)
  }, [currentUser.nickname])

  useEffect(() => {
    if (!isEditingNickname) {
      return
    }

    nicknameInputRef.current?.focus()
    nicknameInputRef.current?.select()
  }, [isEditingNickname])

  const changePasswordMutation = useMutation({
    mutationFn: changeOwnPassword,
    onMutate: () => {
      setSuccessMessage(null)
    },
    onSuccess: async () => {
      // 服务端改密后会轮换 session cookie，这里顺手刷新当前用户查询，确保前端状态和新会话保持一致。
      await queryClient.invalidateQueries({ queryKey: ['current-user'] })
      setSuccessMessage(l('Password updated.'))
    },
  })

  const updateProfileMutation = useMutation({
    mutationFn: updateOwnProfile,
    onMutate: () => {
      setSuccessMessage(null)
    },
    onSuccess: async (updatedUser) => {
      queryClient.setQueryData(['current-user'], updatedUser)
      if (updatedUser.role === 'admin') {
        await queryClient.invalidateQueries({ queryKey: ['users'] })
      }
      setIsEditingNickname(false)
      setSuccessMessage(l('Nickname updated.'))
    },
  })

  const cancelNicknameEditing = () => {
    setNicknameDraft(currentUser.nickname)
    setIsEditingNickname(false)
    updateProfileMutation.reset()
  }

  const saveNickname = () => {
    if (updateProfileMutation.isPending || nicknameDraft.trim() === currentUser.nickname.trim()) {
      return
    }

    updateProfileMutation.mutate({
      nickname: nicknameDraft,
    })
  }

  return (
    <div className="page-stack profile-page">
      <section className="catalog-block profile-page__panel">
        <div>
          <p className="eyebrow">{l('Profile')}</p>
          <h2>{nickname}</h2>
        </div>

        <div className="profile-page__details">
          <div className="profile-page__row">
            <span className="profile-page__label">{l('Username:')}</span>
            <strong className="profile-page__value">{currentUser.username}</strong>
          </div>

          <div className="profile-page__row">
            <span className="profile-page__label">{l('Nickname:')}</span>
            {isEditingNickname ? (
              <div className="profile-page__inline-editor">
                <div className="profile-page__editor-surface">
                  <input
                    className="profile-page__input"
                    maxLength={128}
                    onChange={(event) => setNicknameDraft(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') {
                        event.preventDefault()
                        saveNickname()
                      }

                      if (event.key === 'Escape') {
                        event.preventDefault()
                        cancelNicknameEditing()
                      }
                    }}
                    placeholder={currentUser.username}
                    ref={nicknameInputRef}
                    type="text"
                    value={nicknameDraft}
                  />
                  <div className="profile-page__editor-actions">
                    <button
                      className="profile-page__action-link text-link"
                      disabled={
                        updateProfileMutation.isPending ||
                        nicknameDraft.trim() === currentUser.nickname.trim()
                      }
                      onClick={saveNickname}
                      type="button"
                    >
                      {updateProfileMutation.isPending ? l('Saving…') : l('Save')}
                    </button>
                    <button
                      className="profile-page__action-link text-link"
                      disabled={updateProfileMutation.isPending}
                      onClick={cancelNicknameEditing}
                      type="button"
                    >
                      {l('Cancel')}
                    </button>
                  </div>
                </div>
              </div>
            ) : (
              <>
                <strong className="profile-page__value">{nickname}</strong>
                <button
                  aria-label={l('Edit nickname')}
                  className="profile-page__icon-button"
                  onClick={() => {
                    setNicknameDraft(currentUser.nickname)
                    setIsEditingNickname(true)
                    updateProfileMutation.reset()
                  }}
                  type="button"
                >
                  <svg aria-hidden="true" fill="none" focusable="false" viewBox="0 0 24 24">
                    <path
                      d="M4 20H8.2L18.45 9.75C19.18 9.02 19.18 7.84 18.45 7.11L16.89 5.55C16.16 4.82 14.98 4.82 14.25 5.55L4 15.8V20Z"
                      stroke="currentColor"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth="1.7"
                    />
                    <path
                      d="M12.75 7.05L16.95 11.25"
                      stroke="currentColor"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth="1.7"
                    />
                  </svg>
                </button>
              </>
            )}
          </div>

          <div className="profile-page__row">
            <span className="profile-page__label">{l('Role:')}</span>
            <StatusPill status={roleLabel} />
          </div>

          <div className="profile-page__row profile-page__row--setting">
            <span className="profile-page__label">{l('Interface Language:')}</span>
            <div className="profile-page__setting">
              <div className="profile-page__select">
                <GlassSelect
                  ariaLabel={l('Interface Language:')}
                  onChange={(value) => {
                    setLanguage(value)
                  }}
                  options={interfaceLanguageOptions}
                  value={interfaceLanguage}
                />
              </div>
              <p className="profile-page__hint">
                {l('Stored locally on this browser for your interface preference.')}
              </p>
            </div>
          </div>

          <div className="profile-page__row profile-page__row--setting">
            <span className="profile-page__label">{l('Theme:')}</span>
            <div className="profile-page__setting">
              <div className="profile-page__select">
                <GlassSelect
                  ariaLabel={l('Theme:')}
                  onChange={(value) => {
                    const nextTheme = setThemePreference(value)
                    setThemePreferenceState(nextTheme)
                  }}
                  options={themeOptions}
                  value={themePreference}
                />
              </div>
              <p className="profile-page__hint">
                {l('Switch between the dark and light glass surfaces instantly.')}
              </p>
            </div>
          </div>

          <div className="profile-page__row">
            <span className="profile-page__label">{l('Password:')}</span>
            <button
              className="profile-page__action-link text-link"
              onClick={() => setIsChangePasswordOpen(true)}
              type="button"
            >
              {l('Reset Password')}
            </button>
          </div>
        </div>

        {updateProfileMutation.error instanceof Error ? (
          <p className="callout callout--danger">{updateProfileMutation.error.message}</p>
        ) : null}
        {successMessage ? <p className="muted">{successMessage}</p> : null}
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
