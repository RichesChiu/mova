import { type FormEvent, useMemo, useState } from 'react'
import type { CreateUserInput, Library, UserRole } from '../../api/types'

interface CreateUserFormProps {
  error: string | null
  isSubmitting: boolean
  libraries: Library[]
  onSubmit: (input: CreateUserInput) => Promise<unknown>
}

export const CreateUserForm = ({
  error,
  isSubmitting,
  libraries,
  onSubmit,
}: CreateUserFormProps) => {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<UserRole>('viewer')
  const [isEnabled, setIsEnabled] = useState(true)
  const [selectedLibraryIds, setSelectedLibraryIds] = useState<number[]>([])

  const sortedLibraries = useMemo(
    () => [...libraries].sort((left, right) => left.name.localeCompare(right.name)),
    [libraries],
  )

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    try {
      await onSubmit({
        username,
        password,
        role,
        is_enabled: isEnabled,
        library_ids: role === 'admin' ? [] : selectedLibraryIds,
      })

      setUsername('')
      setPassword('')
      setRole('viewer')
      setIsEnabled(true)
      setSelectedLibraryIds([])
    } catch {
      // Mutation state already exposes the error message.
    }
  }

  const toggleLibrary = (libraryId: number) => {
    setSelectedLibraryIds((current) =>
      current.includes(libraryId)
        ? current.filter((value) => value !== libraryId)
        : [...current, libraryId],
    )
  }

  return (
    <form className="stack" onSubmit={handleSubmit}>
      <label className="field">
        <span>Username</span>
        <input
          autoComplete="username"
          onChange={(event) => setUsername(event.target.value)}
          placeholder="viewer01"
          type="text"
          value={username}
        />
      </label>

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

      <label className="field">
        <span>Role</span>
        <select onChange={(event) => setRole(event.target.value as UserRole)} value={role}>
          <option value="viewer">Viewer</option>
          <option value="admin">Admin</option>
        </select>
      </label>

      <label className="toggle">
        <input
          checked={isEnabled}
          onChange={(event) => setIsEnabled(event.target.checked)}
          type="checkbox"
        />
        <span>Account enabled</span>
      </label>

      {role === 'viewer' ? (
        <div className="field">
          <span>Library Access</span>
          {sortedLibraries.length === 0 ? (
            <p className="muted">请先创建至少一个媒体库，再给普通用户分配可见范围。</p>
          ) : (
            <div className="access-grid">
              {sortedLibraries.map((library) => {
                const checked = selectedLibraryIds.includes(library.id)

                return (
                  <label className="access-chip" key={library.id}>
                    <input
                      checked={checked}
                      onChange={() => toggleLibrary(library.id)}
                      type="checkbox"
                    />
                    <span>{library.name}</span>
                    <small>{library.library_type}</small>
                  </label>
                )
              })}
            </div>
          )}
        </div>
      ) : (
        <p className="muted">管理员默认拥有所有媒体库访问权，不需要单独分配。</p>
      )}

      {error ? <p className="callout callout--danger">{error}</p> : null}

      <button
        className="button button--primary"
        disabled={
          isSubmitting ||
          username.trim().length === 0 ||
          password.length < 8 ||
          (role === 'viewer' && selectedLibraryIds.length === 0)
        }
        type="submit"
      >
        {isSubmitting ? 'Creating…' : 'Create User'}
      </button>
    </form>
  )
}
