import { type FormEvent, useEffect, useMemo, useState } from 'react'
import { createPortal } from 'react-dom'
import type {
  CreateUserInput,
  Library,
  UpdateUserInput,
  UserAccount,
  UserRole,
} from '../../api/types'
import { GlassSelect } from '../glass-select'

interface UserEditorModalProps {
  currentUserId: number
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

const roleOptions = [
  { label: 'Member', value: 'viewer' },
  { label: 'Administrator', value: 'admin' },
]

const avatarInitial = (username: string) => username.trim().charAt(0).toUpperCase() || 'U'

export const UserEditorModal = ({
  currentUserId,
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
  const [username, setUsername] = useState('')
  const [nickname, setNickname] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<UserRole>('viewer')
  const [isEnabled, setIsEnabled] = useState(true)
  const [selectedLibraryIds, setSelectedLibraryIds] = useState<number[]>([])

  const sortedLibraries = useMemo(
    () => [...libraries].sort((left, right) => left.name.localeCompare(right.name)),
    [libraries],
  )
  const isCreateMode = mode === 'create'
  const isEditingAdmin = !isCreateMode && user?.role === 'admin'
  const isEditingSelf = !isCreateMode && user?.id === currentUserId

  useEffect(() => {
    if (!isOpen) {
      return
    }

    // 打开弹窗时总是把表单重置到当前模式对应的数据，避免上一次编辑残留到下一次创建。
    setUsername(user?.username ?? '')
    setNickname(user?.nickname ?? user?.username ?? '')
    setPassword('')
    setRole(user?.role ?? 'viewer')
    setIsEnabled(user?.is_enabled ?? true)
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
        nickname: nickname.trim(),
        password,
        role,
        is_enabled: role === 'admin' ? true : isEnabled,
        library_ids: role === 'admin' ? [] : selectedLibraryIds,
      })
      onClose()
      return
    }

    if (!user) {
      return
    }

    await onUpdate(user.id, {
      username: username.trim(),
      nickname: nickname.trim(),
      role,
      is_enabled: role === 'admin' ? true : isEnabled,
      library_ids: role === 'admin' ? [] : selectedLibraryIds,
    })
    onClose()
  }

  if (!isOpen) {
    return null
  }

  const title = isCreateMode ? 'Create User' : 'Edit User'
  const submitLabel = isCreateMode
    ? isSubmitting
      ? 'Creating…'
      : 'Create User'
    : isSubmitting
      ? 'Saving…'
      : 'Save Changes'
  const gridClassName =
    isCreateMode || !isEditingAdmin
      ? 'user-editor-modal__grid'
      : 'user-editor-modal__grid user-editor-modal__grid--single'

  return createPortal(
    <div className="user-editor-modal">
      <button
        aria-label="Close user editor dialog"
        className="user-editor-modal__backdrop"
        onClick={onClose}
        type="button"
      />

      <div aria-modal="true" className="user-editor-modal__surface" role="dialog">
        <div className="user-editor-modal__header">
          <div className="user-editor-modal__identity">
            <div className="user-editor-modal__avatar">
              {avatarInitial(nickname.trim() || username)}
            </div>
            <div>
              <p className="eyebrow">User Management</p>
              <h3>{title}</h3>
            </div>
          </div>

          <button
            aria-label="Close user editor dialog"
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
          <div className={gridClassName}>
            <label className="field">
              <span>Username</span>
              <input
                autoComplete="username"
                disabled={!isCreateMode}
                onChange={(event) => setUsername(event.target.value)}
                placeholder="viewer01"
                type="text"
                value={username}
              />
            </label>

            <label className="field">
              <span>Nickname</span>
              <input
                maxLength={128}
                onChange={(event) => setNickname(event.target.value)}
                placeholder="Shown in the app header"
                type="text"
                value={nickname}
              />
            </label>

            {isCreateMode ? (
              <label className="field">
                <span>Password</span>
                <input
                  autoComplete="new-password"
                  onChange={(event) => setPassword(event.target.value)}
                  placeholder="At least 8 characters"
                  type="password"
                  value={password}
                />
              </label>
            ) : null}

            {isCreateMode || !isEditingAdmin ? (
              <div className="field">
                <span>Role</span>
                <GlassSelect
                  ariaLabel="User role"
                  onChange={(value) => {
                    const nextRole = value as UserRole
                    setRole(nextRole)
                    if (nextRole === 'admin') {
                      setIsEnabled(true)
                      setSelectedLibraryIds([])
                    }
                  }}
                  options={roleOptions}
                  value={role}
                />
              </div>
            ) : null}
          </div>

          {role === 'viewer' ? (
            <>
              <label className={isEditingSelf ? 'toggle toggle--disabled' : 'toggle'}>
                <input
                  checked={isEnabled}
                  disabled={isEditingSelf}
                  onChange={(event) => setIsEnabled(event.target.checked)}
                  type="checkbox"
                />
                <span>Account enabled</span>
              </label>
              {isEditingSelf ? (
                <p className="muted">You cannot change your own enabled state here.</p>
              ) : null}
            </>
          ) : (
            <p className="muted">
              Admin accounts stay enabled and do not expose an enable or disable toggle here.
            </p>
          )}

          {role === 'viewer' ? (
            <div className="field">
              <span>Library Access</span>
              {sortedLibraries.length === 0 ? (
                <p className="muted">Create at least one library before assigning viewer access.</p>
              ) : (
                <div className="user-editor-modal__access-grid">
                  {sortedLibraries.map((library) => {
                    const checked = selectedLibraryIds.includes(library.id)

                    return (
                      <label className="user-editor-modal__access-chip" key={library.id}>
                        <span className="user-editor-modal__access-chip-title">{library.name}</span>
                        <span className="user-editor-modal__access-chip-footer">
                          <input
                            aria-label={`${checked ? 'Remove' : 'Grant'} access to ${library.name}`}
                            className="user-editor-modal__access-checkbox"
                            checked={checked}
                            onChange={() => toggleLibrary(library.id)}
                            type="checkbox"
                          />
                        </span>
                      </label>
                    )
                  })}
                </div>
              )}
            </div>
          ) : (
            <p className="muted">Admin accounts automatically have access to every library.</p>
          )}

          {error ? <p className="callout callout--danger">{error}</p> : null}

          <div className="user-editor-modal__footer">
            <button className="button" onClick={onClose} type="button">
              Cancel
            </button>
            <button
              className="button button--primary"
              disabled={
                isSubmitting ||
                username.trim().length === 0 ||
                (isCreateMode && password.length < 8) ||
                (role === 'viewer' && selectedLibraryIds.length === 0)
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
