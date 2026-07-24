import { type FormEvent, useEffect, useMemo, useState } from 'react'
import { createPortal } from 'react-dom'
import type {
  CreateUserInput,
  Library,
  UpdateUserInput,
  UserAccount,
  UserRole,
} from '../../api/types'
import { useI18n } from '../../i18n'
import { usePresenceTransition } from '../../lib/use-presence-transition'
import { USER_ACCOUNT_MAX_LENGTH } from '../../lib/user-account'
import { GlassSelect } from '../glass-select'
import { LibraryAccessOption } from './library-access-option'

interface UserEditorModalProps {
  currentUserIsPrimaryAdmin: boolean
  error: string | null
  isOpen: boolean
  isSubmitting: boolean
  libraries: Library[]
  mode: 'create' | 'edit'
  onClose: () => void
  onCreate: (input: CreateUserInput) => Promise<unknown>
  onUpdate: (userId: number, input: UpdateUserInput) => Promise<unknown>
  user?: UserAccount | null
}

export const UserEditorModal = ({
  currentUserIsPrimaryAdmin,
  error,
  isOpen,
  isSubmitting,
  libraries,
  mode,
  onClose,
  onCreate,
  onUpdate,
  user = null,
}: UserEditorModalProps) => {
  const { l } = useI18n()
  const modalPresence = usePresenceTransition(isOpen)
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<UserRole>('viewer')
  const [selectedLibraryIds, setSelectedLibraryIds] = useState<number[]>([])

  const roleOptions = useMemo(
    () =>
      currentUserIsPrimaryAdmin
        ? [
            { label: l('Standard User'), value: 'viewer' },
            { label: l('Administrator'), value: 'admin' },
          ]
        : [{ label: l('Standard User'), value: 'viewer' }],
    [currentUserIsPrimaryAdmin, l],
  )
  const sortedLibraries = useMemo(
    () => [...libraries].sort((left, right) => left.name.localeCompare(right.name)),
    [libraries],
  )
  const isCreateMode = mode === 'create'
  const shouldShowRoleField = isCreateMode || (currentUserIsPrimaryAdmin && !user?.is_primary_admin)

  useEffect(() => {
    if (!isOpen) {
      return
    }

    // 打开弹窗时总是把表单重置到当前模式对应的数据，避免上一次编辑残留到下一次创建。
    setUsername(user?.username ?? '')
    setPassword('')
    setRole(user?.role ?? 'viewer')
    setSelectedLibraryIds(user?.library_ids ?? [])
  }, [isOpen, user])

  useEffect(() => {
    if (!isOpen) {
      return
    }

    const previousOverflow = document.body.style.overflow
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose()
      }
    }

    document.body.style.overflow = 'hidden'
    window.addEventListener('keydown', handleKeyDown)

    return () => {
      document.body.style.overflow = previousOverflow
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [isOpen, onClose])

  const toggleLibrary = (libraryId: number) => {
    setSelectedLibraryIds((current) =>
      current.includes(libraryId)
        ? current.filter((value) => value !== libraryId)
        : [...current, libraryId],
    )
  }

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    if (mode === 'create') {
      await onCreate({
        username: username.trim(),
        password,
        role,
        is_enabled: true,
        library_ids: role === 'admin' ? [] : selectedLibraryIds,
      })
      onClose()
      return
    }

    if (!user) {
      return
    }

    await onUpdate(user.id, {
      role,
      library_ids: role === 'admin' ? [] : selectedLibraryIds,
    })
    onClose()
  }

  if (!modalPresence.shouldRender) {
    return null
  }

  const title = isCreateMode ? l('Create User') : l('Edit User')
  const submitLabel = isCreateMode
    ? isSubmitting
      ? l('Creating…')
      : l('Create User')
    : isSubmitting
      ? l('Saving…')
      : l('Save Changes')
  return createPortal(
    <div
      className="user-editor-modal overlay-transition"
      data-state={modalPresence.transitionState}
    >
      <button
        aria-label={l('Close user editor dialog')}
        className="user-editor-modal__backdrop glass-overlay-backdrop"
        onClick={onClose}
        type="button"
      />

      <div
        aria-modal="true"
        className="user-editor-modal__surface glass-modal-surface"
        role="dialog"
      >
        <div className="user-editor-modal__header">
          <h3>{title}</h3>

          <button
            aria-label={l('Close user editor dialog')}
            className="user-editor-modal__close"
            onClick={onClose}
            type="button"
          >
            <svg
              aria-hidden="true"
              className="user-editor-modal__close-icon"
              fill="none"
              viewBox="0 0 24 24"
            >
              <path
                d="M6 6L18 18M18 6L6 18"
                stroke="currentColor"
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="1.8"
              />
            </svg>
          </button>
        </div>

        <form className="stack" onSubmit={handleSubmit}>
          {isCreateMode || shouldShowRoleField ? (
            <div
              className={
                isCreateMode
                  ? 'user-editor-modal__grid'
                  : 'user-editor-modal__grid user-editor-modal__grid--single'
              }
            >
              {isCreateMode ? (
                <>
                  <label className="field">
                    <span>{l('Account')}</span>
                    <input
                      autoComplete="username"
                      maxLength={USER_ACCOUNT_MAX_LENGTH}
                      onChange={(event) => setUsername(event.target.value)}
                      placeholder={l('Enter the account used to sign in')}
                      spellCheck={false}
                      type="text"
                      value={username}
                    />
                  </label>

                  <label className="field">
                    <span>{l('Password')}</span>
                    <input
                      autoComplete="new-password"
                      onChange={(event) => setPassword(event.target.value)}
                      placeholder={l('At least 8 characters')}
                      type="password"
                      value={password}
                    />
                  </label>
                </>
              ) : null}

              {shouldShowRoleField ? (
                <div className="field">
                  <span>{l('Role')}</span>
                  <GlassSelect
                    ariaLabel={l('User role')}
                    onChange={(value) => {
                      const nextRole = value as UserRole
                      setRole(nextRole)
                      if (nextRole === 'admin') {
                        setSelectedLibraryIds([])
                      }
                    }}
                    options={roleOptions}
                    value={role}
                  />
                </div>
              ) : null}
            </div>
          ) : null}

          {role === 'viewer' ? (
            <div className="field">
              <span>{l('Library Access')}</span>
              {sortedLibraries.length === 0 ? (
                <p className="muted">
                  {l('No libraries assigned yet. You can save this user first.')}
                </p>
              ) : (
                <div className="user-editor-modal__access-grid">
                  {sortedLibraries.map((library) => {
                    const checked = selectedLibraryIds.includes(library.id)

                    return (
                      <LibraryAccessOption
                        checked={checked}
                        key={library.id}
                        library={library}
                        onToggle={() => toggleLibrary(library.id)}
                      />
                    )
                  })}
                </div>
              )}
            </div>
          ) : (
            <p className="muted">
              {l('Admin accounts automatically have access to every library.')}
            </p>
          )}

          {error ? <p className="callout callout--danger">{error}</p> : null}

          <div className="user-editor-modal__footer">
            <button className="button" onClick={onClose} type="button">
              {l('Cancel')}
            </button>
            <button
              className="button button--primary"
              disabled={
                isSubmitting ||
                (isCreateMode && (username.trim().length === 0 || password.length < 8))
              }
              type="submit"
            >
              {submitLabel}
            </button>
          </div>
        </form>
      </div>
    </div>,
    document.body,
  )
}
