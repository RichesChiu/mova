import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useOutletContext } from 'react-router-dom'
import { createLibrary, createUser, deleteLibrary, listUsers, scanLibrary } from '../../api/client'
import type { AppShellOutletContext } from '../../components/app-shell'
import { CreateLibraryForm } from '../../components/create-library-form'
import { CreateUserForm } from '../../components/create-user-form'
import { SettingsGearIcon } from '../../components/settings-gear-icon'

export const SettingsPage = () => {
  const { currentUser, libraries } = useOutletContext<AppShellOutletContext>()
  const queryClient = useQueryClient()
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

  if (currentUser.role !== 'admin') {
    return <p className="callout callout--danger">Admin permission required.</p>
  }

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

      <section className="settings-grid">
        <article className="settings-card">
          <div className="settings-card__header">
            <span className="summary-card__label">User Management</span>
            <span className="chip">Live</span>
          </div>
          <h3>Users & Roles</h3>
          <p className="muted">管理员默认拥有全部媒体库访问权；普通用户只看被分配到的库。</p>
          <div className="settings-card__list">
            <span>{usersQuery.data?.length ?? 0} accounts</span>
            <span>Admin / Viewer</span>
            <span>Per-library access</span>
          </div>
        </article>

        <article className="settings-card">
          <div className="settings-card__header">
            <span className="summary-card__label">Metadata</span>
            <span className="chip">Planned</span>
          </div>
          <h3>Metadata Providers</h3>
          <p className="muted">
            配置 metadata provider、语言、图片缓存策略，以及后续的手动重绑定和匹配修正。
          </p>
          <div className="settings-card__list">
            <span>TMDB provider</span>
            <span>Image cache</span>
            <span>Fallback rules</span>
          </div>
        </article>

        <article className="settings-card">
          <div className="settings-card__header">
            <span className="summary-card__label">Operations</span>
            <span className="chip">Live</span>
          </div>
          <h3>Scanning & Jobs</h3>
          <p className="muted">
            扫描和建库操作已经从浏览页移到这里，后续 watcher、周期校准和后台任务也会在这里聚合。
          </p>
          <div className="settings-card__list">
            <span>Scan jobs</span>
            <span>Watcher health</span>
            <span>Background workers</span>
          </div>
        </article>
      </section>

      <section className="settings-section">
        <div className="section-heading">
          <div>
            <h3>User Management</h3>
            <p className="muted">创建管理员或普通用户。普通用户必须手动分配可见媒体库。</p>
          </div>
        </div>

        {usersQuery.isError ? (
          <p className="callout callout--danger">
            {usersQuery.error instanceof Error ? usersQuery.error.message : 'Failed to load users'}
          </p>
        ) : null}

        <div className="settings-user-grid">
          <div className="settings-create-block">
            <CreateUserForm
              error={
                createUserMutation.error instanceof Error ? createUserMutation.error.message : null
              }
              isSubmitting={createUserMutation.isPending}
              libraries={libraries}
              onSubmit={(input) => createUserMutation.mutateAsync(input)}
            />
          </div>

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
                <article className="settings-library-card" key={user.id}>
                  <div className="settings-library-card__body">
                    <span className="summary-card__label">{user.role}</span>
                    <strong>{user.username}</strong>
                    <p className="muted">
                      {user.is_enabled ? 'Account enabled' : 'Account disabled'}
                    </p>
                    <p className="muted">
                      Access:{' '}
                      {libraryNames.length > 0 ? libraryNames.join(', ') : 'No libraries assigned'}
                    </p>
                  </div>
                </article>
              )
            })}
          </div>
        </div>
      </section>

      <section className="settings-section">
        <div className="section-heading">
          <div>
            <h3>Library Management</h3>
            <p className="muted">建库和扫描统一集中到这里，普通浏览页不再直接提供这些管理操作。</p>
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
                <div className="settings-library-card__body">
                  <span className="summary-card__label">{library.library_type}</span>
                  <strong>{library.name}</strong>
                  <p className="muted">{library.description ?? 'No description'}</p>
                  <p className="muted">Path: {library.root_path}</p>
                </div>

                <div className="settings-library-card__meta">
                  <span
                    className={
                      library.is_enabled
                        ? 'status-pill status-pill--success'
                        : 'status-pill status-pill--neutral'
                    }
                  >
                    {library.is_enabled ? 'enabled' : 'disabled'}
                  </span>
                  <button
                    className="button"
                    disabled={scanMutation.isPending}
                    onClick={() => scanMutation.mutate(library.id)}
                    type="button"
                  >
                    {scanMutation.isPending ? 'Triggering…' : 'Scan Library'}
                  </button>
                  <button
                    className="button button--danger"
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
    </div>
  )
}
