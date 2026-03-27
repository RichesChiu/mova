import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useOutletContext } from 'react-router-dom'
import {
  createLibrary,
  createUser,
  deleteLibrary,
  deleteUser,
  listUsers,
  scanLibrary,
  updateUser,
} from '../../api/client'
import type { UserAccount } from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import { CreateLibraryForm } from '../../components/create-library-form'
import { SettingsGearIcon } from '../../components/settings-gear-icon'
import { UserEditorModal } from '../../components/user-editor-modal'

const userAvatarInitial = (username: string) => username.trim().charAt(0).toUpperCase() || 'U'

export const SettingsPage = () => {
  const { currentUser, libraries } = useOutletContext<AppShellOutletContext>()
  const queryClient = useQueryClient()
  const [isCreateUserOpen, setIsCreateUserOpen] = useState(false)
  const [editingUser, setEditingUser] = useState<UserAccount | null>(null)
  const usersQuery = useQuery({
    enabled: currentUser.role === 'admin',
    queryKey: ['users'],
    queryFn: listUsers,
  })

  const createLibraryMutation = useMutation({
    mutationFn: createLibrary,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['libraries'] })
    },
  })

  const scanMutation = useMutation({
    mutationFn: (libraryId: number) => scanLibrary(libraryId),
    onSuccess: async (_scanJob, libraryId) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['library', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['library-media', libraryId] }),
      ])
    },
  })

  const deleteLibraryMutation = useMutation({
    mutationFn: (libraryId: number) => deleteLibrary(libraryId),
    onSuccess: async (_result, libraryId) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['library', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['library-media', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-shelf'] }),
      ])
    },
  })

  const createUserMutation = useMutation({
    mutationFn: createUser,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['users'] })
    },
  })

  const updateUserMutation = useMutation({
    mutationFn: ({ userId, input }: { userId: number; input: Parameters<typeof updateUser>[1] }) =>
      updateUser(userId, input),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['users'] }),
        queryClient.invalidateQueries({ queryKey: ['current-user'] }),
      ])
    },
  })

  const deleteUserMutation = useMutation({
    mutationFn: (userId: number) => deleteUser(userId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['users'] })
    },
  })

  if (currentUser.role !== 'admin') {
    return <p className="callout callout--danger">Admin permission required.</p>
  }

  const activeUserModalError = isCreateUserOpen
    ? createUserMutation.error instanceof Error
      ? createUserMutation.error.message
      : null
    : editingUser && updateUserMutation.error instanceof Error
      ? updateUserMutation.error.message
      : null

  return (
    <div className="settings-shell">
      <section className="settings-hero">
        <div className="settings-hero__badge">
          <SettingsGearIcon className="settings-hero__icon" />
        </div>
        <div className="settings-hero__copy">
          <p className="eyebrow">Admin Settings</p>
          <h2>Server Settings</h2>
          <p className="muted">
            当前以管理员视角登录：`{currentUser.username}
            `。这里统一承接用户、媒体库和扫描相关的管理动作。
          </p>
        </div>
      </section>

      <section className="settings-section">
        <div className="section-heading">
          <div>
            <h3>User Management</h3>
            <p className="muted">用头像卡片管理用户，创建和编辑都在弹窗里完成。</p>
          </div>
          <button
            className="button button--primary button--toolbar"
            onClick={() => setIsCreateUserOpen(true)}
            type="button"
          >
            <span>Create User</span>
          </button>
        </div>

        {usersQuery.isError ? (
          <p className="callout callout--danger">
            {usersQuery.error instanceof Error ? usersQuery.error.message : 'Failed to load users'}
          </p>
        ) : null}
        {updateUserMutation.isError && !editingUser ? (
          <p className="callout callout--danger">
            {updateUserMutation.error instanceof Error
              ? updateUserMutation.error.message
              : 'Failed to update user'}
          </p>
        ) : null}
        {deleteUserMutation.isError ? (
          <p className="callout callout--danger">
            {deleteUserMutation.error instanceof Error
              ? deleteUserMutation.error.message
              : 'Failed to delete user'}
          </p>
        ) : null}

        <div className="settings-user-list">
          {usersQuery.isLoading ? <p className="muted">Loading users…</p> : null}

          {usersQuery.data?.map((user) => {
            const libraryNames =
              user.role === 'admin'
                ? ['All libraries']
                : libraries
                    .filter((library) => user.library_ids.includes(library.id))
                    .map((library) => library.name)

            return (
              <article className="settings-user-card" key={user.id}>
                <button
                  aria-label={`Edit ${user.username}`}
                  className="settings-user-card__avatar"
                  onClick={() => setEditingUser(user)}
                  type="button"
                >
                  <span>{userAvatarInitial(user.username)}</span>
                </button>

                <div className="settings-user-card__body">
                  <div className="settings-user-card__header">
                    <div>
                      <strong>{user.username}</strong>
                      <p className="muted">{user.role === 'admin' ? 'Administrator' : 'Viewer'}</p>
                    </div>

                    <div className="settings-user-card__controls">
                      {user.role === 'viewer' ? (
                        <label className="settings-user-card__switch">
                          <input
                            checked={user.is_enabled}
                            disabled={updateUserMutation.isPending}
                            onChange={(event) =>
                              updateUserMutation.mutate({
                                userId: user.id,
                                input: { is_enabled: event.target.checked },
                              })
                            }
                            type="checkbox"
                          />
                          <span className="settings-user-card__switch-track" />
                        </label>
                      ) : null}

                      <button
                        aria-label={`Edit ${user.username}`}
                        className="settings-user-card__edit-icon"
                        onClick={() => setEditingUser(user)}
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
                    </div>
                  </div>

                  <p className="settings-user-card__access">
                    {user.role === 'admin'
                      ? 'Access: All libraries'
                      : libraryNames.length > 0
                        ? `Access: ${libraryNames.join(', ')}`
                        : 'Access: No libraries assigned'}
                  </p>
                </div>

                <div className="settings-user-card__actions">
                  {user.id !== currentUser.id ? (
                    <button
                      className="button button--danger settings-user-card__delete"
                      disabled={deleteUserMutation.isPending}
                      onClick={() => {
                        const confirmed = window.confirm(
                          `Delete user "${user.username}"? This removes their access, sessions, and playback records.`,
                        )

                        if (!confirmed) {
                          return
                        }

                        deleteUserMutation.mutate(user.id)
                      }}
                      type="button"
                    >
                      {deleteUserMutation.isPending && deleteUserMutation.variables === user.id
                        ? 'Deleting…'
                        : 'Delete'}
                    </button>
                  ) : null}
                </div>
              </article>
            )
          })}
        </div>
      </section>

      <section className="settings-section">
        <div className="section-heading">
          <div>
            <h3>Library Management</h3>
            <p className="muted">用卡片方式管理媒体库，扫描和删除都集中在这里处理。</p>
          </div>
        </div>

        {deleteLibraryMutation.isError ? (
          <p className="callout callout--danger">
            {deleteLibraryMutation.error instanceof Error
              ? deleteLibraryMutation.error.message
              : 'Failed to delete library'}
          </p>
        ) : null}

        <div className="settings-library-list">
          {libraries.length === 0 ? (
            <article className="settings-library-card">
              <div className="settings-library-card__body">
                <span className="summary-card__label">No libraries yet</span>
                <strong>Create the first library</strong>
                <p className="muted">下方表单会直接创建媒体库，创建完成后库列表会出现在这里。</p>
              </div>
            </article>
          ) : (
            libraries.map((library) => (
              <article className="settings-library-card" key={library.id}>
                <div aria-hidden="true" className="settings-library-card__backdrop">
                  <span className="settings-library-card__backdrop-glow" />
                </div>

                <div className="settings-library-card__body">
                  <div className="settings-library-card__header">
                    <span className="settings-library-card__type">{library.library_type}</span>
                    <span className="settings-library-card__language">
                      {library.metadata_language}
                    </span>
                  </div>

                  <strong className="settings-library-card__title">{library.name}</strong>
                  <p className="settings-library-card__description">
                    {library.description ?? 'No description'}
                  </p>

                  <div className="settings-library-card__path-block">
                    <span className="settings-library-card__path-label">Root path</span>
                    <code className="settings-library-card__path">{library.root_path}</code>
                  </div>
                </div>

                <div className="settings-library-card__actions">
                  <button
                    className="button"
                    disabled={scanMutation.isPending}
                    onClick={() => scanMutation.mutate(library.id)}
                    type="button"
                  >
                    {scanMutation.isPending ? 'Triggering…' : 'Scan Library'}
                  </button>
                  <button
                    className="button button--danger settings-library-card__delete"
                    disabled={deleteLibraryMutation.isPending || scanMutation.isPending}
                    onClick={() => {
                      const confirmed = window.confirm(
                        `Delete library "${library.name}"? This removes library records and scan history.`,
                      )
                      if (!confirmed) {
                        return
                      }
                      deleteLibraryMutation.mutate(library.id)
                    }}
                    type="button"
                  >
                    {deleteLibraryMutation.isPending &&
                    deleteLibraryMutation.variables === library.id
                      ? 'Deleting…'
                      : 'Delete Library'}
                  </button>
                </div>
              </article>
            ))
          )}
        </div>
      </section>

      <section className="settings-section">
        <div className="section-heading">
          <div>
            <h3>Create Library</h3>
            <p className="muted">默认仍然支持 `mixed / movie / series` 三种建库模式。</p>
          </div>
        </div>

        <div className="settings-create-block">
          <CreateLibraryForm
            error={
              createLibraryMutation.error instanceof Error
                ? createLibraryMutation.error.message
                : null
            }
            isSubmitting={createLibraryMutation.isPending}
            onSubmit={(input) => createLibraryMutation.mutateAsync(input)}
          />
        </div>
      </section>

      <UserEditorModal
        currentUserId={currentUser.id}
        error={activeUserModalError}
        isOpen={isCreateUserOpen || editingUser !== null}
        isSubmitting={createUserMutation.isPending || updateUserMutation.isPending}
        libraries={libraries}
        mode={isCreateUserOpen ? 'create' : 'edit'}
        onClose={() => {
          setIsCreateUserOpen(false)
          setEditingUser(null)
          createUserMutation.reset()
          updateUserMutation.reset()
        }}
        onCreate={(input) => createUserMutation.mutateAsync(input)}
        onUpdate={(userId, input) => updateUserMutation.mutateAsync({ userId, input })}
        user={editingUser}
      />
    </div>
  )
}
