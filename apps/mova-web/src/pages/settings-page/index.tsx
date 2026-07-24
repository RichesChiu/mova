import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useOutletContext } from 'react-router-dom'
import {
  createLibrary,
  createUser,
  deleteLibrary,
  deleteUser,
  getLibrary,
  listUsers,
  scanLibrary,
  updateLibrary,
  updateUser,
} from '../../api/client'
import type { CreateLibraryInput, Library, LibraryDetail, UserAccount } from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import { ConfirmActionModal } from '../../components/confirm-action-modal'
import { CreateLibraryModal } from '../../components/create-library-modal'
import { EmptyState } from '../../components/empty-state'
import { HoverTooltip } from '../../components/hover-tooltip'
import { LibraryActionsMenu } from '../../components/library-actions-menu'
import { LibraryEditorModal } from '../../components/library-editor-modal'
import { StatusPill } from '../../components/status-pill'
import { UserActionsMenu } from '../../components/user-actions-menu'
import { UserEditorModal } from '../../components/user-editor-modal'
import { useI18n } from '../../i18n'
import {
  buildCreatedLibraryCacheState,
  buildCreatedUserCacheState,
  buildDeletedLibraryCacheState,
  buildDeletedUserCacheState,
  buildDeleteLibraryConfirmationCopy,
  buildDeleteUserConfirmationCopy,
  buildTriggeredScanCacheState,
  buildUpdatedLibraryCacheState,
  buildUpdatedUserCacheState,
  getScanStatusLabel,
  getScanStatusTone,
} from '../../lib/settings-admin'
import { getUserInitial } from '../../lib/user-identity'
import { canManageUser, getUserRolePresentation } from '../../lib/user-role'
import { DashboardPageHeader } from '../home-page/dashboard-page-header'
import { HomeDashboardShell } from '../home-page/home-dashboard-shell'

const USER_SKELETON_COUNT = 1
const LIBRARY_SKELETON_COUNT = 3
const USER_SKELETON_KEYS = ['current-user'] as const
const LIBRARY_SKELETON_KEYS = ['library-a', 'library-b', 'library-c'] as const

type PendingSettingsConfirmation =
  | { kind: 'delete-library'; library: Library }
  | { kind: 'delete-user'; user: UserAccount }

const SettingsUserCardSkeleton = () => (
  <article aria-hidden="true" className="settings-user-card settings-user-card--loading">
    <div className="settings-user-card__avatar settings-user-card__avatar--loading skeleton-shimmer" />

    <div className="settings-user-card__body">
      <div className="settings-user-card__header">
        <div className="settings-user-card__copy">
          <span className="settings-user-card__line settings-user-card__line--title skeleton-shimmer" />
          <span className="settings-user-card__line settings-user-card__line--role skeleton-shimmer" />
        </div>

        <div className="settings-user-card__controls">
          <span className="settings-user-card__control settings-user-card__control--toggle skeleton-shimmer" />
          <span className="settings-user-card__control settings-user-card__control--icon skeleton-shimmer" />
        </div>
      </div>
    </div>
  </article>
)

const SettingsLibraryCardSkeleton = ({ rootPathLabel }: { rootPathLabel: string }) => (
  <article aria-hidden="true" className="settings-library-card settings-library-card--loading">
    <div aria-hidden="true" className="settings-library-card__backdrop">
      <span className="settings-library-card__backdrop-glow" />
    </div>

    <div className="settings-library-card__body">
      <div className="settings-library-card__header">
        <span className="settings-library-card__line settings-library-card__line--title skeleton-shimmer" />
        <div className="settings-library-card__header-actions">
          <span className="settings-library-card__line settings-library-card__line--status skeleton-shimmer" />
          <span className="settings-library-card__button settings-library-card__button--icon skeleton-shimmer" />
        </div>
      </div>
      <span className="settings-library-card__line settings-library-card__line--description skeleton-shimmer" />
      <span className="settings-library-card__line settings-library-card__line--description-alt skeleton-shimmer" />
      <span className="settings-library-card__line settings-library-card__line--meta skeleton-shimmer" />
      <div className="settings-library-card__path-block">
        <span className="settings-library-card__path-label">{rootPathLabel}</span>
        <span className="settings-library-card__path settings-library-card__path--loading skeleton-shimmer" />
      </div>
    </div>
  </article>
)

export const SettingsPage = () => {
  const { l } = useI18n()
  const { currentUser, libraries, librariesLoading } = useOutletContext<AppShellOutletContext>()
  const queryClient = useQueryClient()
  const [isCreateLibraryOpen, setIsCreateLibraryOpen] = useState(false)
  const [isCreateUserOpen, setIsCreateUserOpen] = useState(false)
  const [editingUser, setEditingUser] = useState<UserAccount | null>(null)
  const [editingLibrary, setEditingLibrary] = useState<Library | null>(null)
  const [pendingConfirmation, setPendingConfirmation] =
    useState<PendingSettingsConfirmation | null>(null)
  const usersQuery = useQuery<UserAccount[]>({
    enabled: currentUser.role === 'admin',
    queryKey: ['users'],
    queryFn: listUsers,
  })
  const libraryDetailQueries = useQueries({
    queries: libraries.map((library) => ({
      enabled: currentUser.role === 'admin',
      queryKey: ['library', library.id],
      queryFn: () => getLibrary(library.id),
    })),
  })

  const createLibraryMutation = useMutation({
    mutationFn: createLibrary,
    onSuccess: async (createdLibrary, _input: CreateLibraryInput) => {
      const nextLibraryCache = buildCreatedLibraryCacheState(
        queryClient.getQueryData<Library[]>(['libraries']),
        createdLibrary,
      )

      // Show the new library immediately so home/settings can render a scanning placeholder while
      // the real list/detail requests catch up in the background.
      queryClient.setQueryData<Library[]>(['libraries'], nextLibraryCache.libraries)
      queryClient.setQueryData<LibraryDetail>(
        ['library', createdLibrary.id],
        nextLibraryCache.libraryDetail,
      )
      queryClient.setQueryData<LibraryDetail>(
        ['home-library-detail', createdLibrary.id],
        nextLibraryCache.homeLibraryDetail,
      )

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['library', createdLibrary.id] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-detail', createdLibrary.id] }),
        queryClient.invalidateQueries({ queryKey: ['recently-added-by-library'] }),
      ])

      setIsCreateLibraryOpen(false)
    },
  })

  const scanMutation = useMutation({
    mutationFn: (libraryId: number) => scanLibrary(libraryId),
    onSuccess: async (scanJob, libraryId) => {
      const fallbackLibrary = libraries.find((library) => library.id === libraryId)

      if (fallbackLibrary) {
        const nextScanCache = buildTriggeredScanCacheState({
          fallbackLibrary,
          currentLibraryDetail: queryClient.getQueryData<LibraryDetail>(['library', libraryId]),
          currentHomeLibraryDetail: queryClient.getQueryData<LibraryDetail>([
            'home-library-detail',
            libraryId,
          ]),
          scanJob,
        })

        queryClient.setQueryData<LibraryDetail>(['library', libraryId], nextScanCache.libraryDetail)
        queryClient.setQueryData<LibraryDetail>(
          ['home-library-detail', libraryId],
          nextScanCache.homeLibraryDetail,
        )
      }

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['library', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['library-media', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-detail', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['recently-added-by-library'] }),
      ])
    },
  })

  const deleteLibraryMutation = useMutation({
    mutationFn: (libraryId: number) => deleteLibrary(libraryId),
    onSuccess: async (_result, libraryId) => {
      const nextLibraryCache = buildDeletedLibraryCacheState(
        queryClient.getQueryData<Library[]>(['libraries']),
        libraryId,
      )

      queryClient.setQueryData<Library[]>(['libraries'], nextLibraryCache.libraries)
      queryClient.removeQueries({ queryKey: ['library', libraryId] })
      queryClient.removeQueries({ queryKey: ['library-media', libraryId] })
      queryClient.removeQueries({ queryKey: ['home-library-detail', libraryId] })

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['library', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['library-media', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['recently-added-by-library'] }),
      ])
    },
  })

  const updateLibraryMutation = useMutation({
    mutationFn: ({
      libraryId,
      input,
    }: {
      libraryId: number
      input: Parameters<typeof updateLibrary>[1]
    }) => updateLibrary(libraryId, input),
    onSuccess: async (updatedLibrary, { libraryId }) => {
      const currentLibraries = queryClient.getQueryData<Library[]>(['libraries'])
      const nextLibraryCache = buildUpdatedLibraryCacheState({
        currentLibraries,
        updatedLibrary,
        currentLibraryDetail: queryClient.getQueryData<LibraryDetail>(['library', libraryId]),
        currentHomeLibraryDetail: queryClient.getQueryData<LibraryDetail>([
          'home-library-detail',
          libraryId,
        ]),
      })

      // Keep the settings list and detail caches in sync immediately so the modal edits feel
      // local-first, then let background refetches reconcile scan state and counts.
      queryClient.setQueryData<Library[]>(['libraries'], nextLibraryCache.libraries)
      queryClient.setQueryData<LibraryDetail>(
        ['library', libraryId],
        nextLibraryCache.libraryDetail,
      )
      queryClient.setQueryData<LibraryDetail>(
        ['home-library-detail', libraryId],
        nextLibraryCache.homeLibraryDetail,
      )

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['library', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['library-media', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-detail', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['recently-added-by-library'] }),
      ])
    },
  })

  const createUserMutation = useMutation({
    mutationFn: createUser,
    onSuccess: async (createdUser) => {
      const nextUserCache = buildCreatedUserCacheState(
        queryClient.getQueryData<UserAccount[]>(['users']),
        createdUser,
      )

      queryClient.setQueryData<UserAccount[]>(['users'], nextUserCache.users)
      await queryClient.invalidateQueries({ queryKey: ['users'] })
    },
  })

  const updateUserMutation = useMutation({
    mutationFn: ({ userId, input }: { userId: number; input: Parameters<typeof updateUser>[1] }) =>
      updateUser(userId, input),
    onSuccess: async (updatedUser) => {
      const nextUserCache = buildUpdatedUserCacheState(
        queryClient.getQueryData<UserAccount[]>(['users']),
        currentUser.id,
        updatedUser,
      )

      queryClient.setQueryData<UserAccount[]>(['users'], nextUserCache.users)
      if (nextUserCache.currentUser) {
        queryClient.setQueryData<UserAccount>(['current-user'], nextUserCache.currentUser)
      }
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['users'] }),
        queryClient.invalidateQueries({ queryKey: ['current-user'] }),
      ])
    },
  })

  const deleteUserMutation = useMutation({
    mutationFn: (userId: number) => deleteUser(userId),
    onSuccess: async (_result, userId) => {
      const nextUserCache = buildDeletedUserCacheState(
        queryClient.getQueryData<UserAccount[]>(['users']),
        userId,
      )

      queryClient.setQueryData<UserAccount[]>(['users'], nextUserCache.users)
      await queryClient.invalidateQueries({ queryKey: ['users'] })
    },
  })

  if (currentUser.role !== 'admin') {
    return (
      <HomeDashboardShell ariaLabel={l('Server Settings')} currentUser={currentUser}>
        <div className="home-dashboard__content home-dashboard__content--settings">
          <DashboardPageHeader>
            <h2>{l('Server Settings')}</h2>
          </DashboardPageHeader>

          <p className="callout callout--danger">{l('Admin permission required.')}</p>
        </div>
      </HomeDashboardShell>
    )
  }

  const activeUserModalError = isCreateUserOpen
    ? createUserMutation.error instanceof Error
      ? createUserMutation.error.message
      : null
    : editingUser && updateUserMutation.error instanceof Error
      ? updateUserMutation.error.message
      : null
  const activeLibraryModalError =
    editingLibrary && updateLibraryMutation.error instanceof Error
      ? updateLibraryMutation.error.message
      : null
  const confirmationCopy =
    pendingConfirmation?.kind === 'delete-user'
      ? buildDeleteUserConfirmationCopy(pendingConfirmation.user)
      : pendingConfirmation?.kind === 'delete-library'
        ? buildDeleteLibraryConfirmationCopy(pendingConfirmation.library)
        : null
  const confirmationError =
    pendingConfirmation?.kind === 'delete-user'
      ? deleteUserMutation.error instanceof Error
        ? deleteUserMutation.error.message
        : null
      : pendingConfirmation?.kind === 'delete-library'
        ? deleteLibraryMutation.error instanceof Error
          ? deleteLibraryMutation.error.message
          : null
        : null
  const isConfirmationSubmitting =
    pendingConfirmation?.kind === 'delete-user'
      ? deleteUserMutation.isPending
      : pendingConfirmation?.kind === 'delete-library'
        ? deleteLibraryMutation.isPending
        : false
  const users = usersQuery.data ?? []
  const shouldShowUserSkeleton = usersQuery.isLoading && users.length === 0
  const shouldShowLibrarySkeleton = librariesLoading && libraries.length === 0
  const libraryDetailsById = new Map(
    libraries.map((library, index) => [library.id, libraryDetailQueries[index]?.data ?? null]),
  )
  return (
    <>
      <HomeDashboardShell ariaLabel={l('Server Settings')} currentUser={currentUser}>
        <div className="home-dashboard__content home-dashboard__content--settings">
          <DashboardPageHeader>
            <h2>{l('Server Settings')}</h2>
          </DashboardPageHeader>

          <section className="settings-section settings-section--users">
            <div className="section-heading">
              <div>
                <h3>{l('User Management')}</h3>
              </div>
              <button
                className="button button--toolbar settings-section__create-button"
                onClick={() => setIsCreateUserOpen(true)}
                type="button"
              >
                <span>{l('Create User')}</span>
              </button>
            </div>

            {usersQuery.isError ? (
              <p className="callout callout--danger">
                {usersQuery.error instanceof Error
                  ? usersQuery.error.message
                  : l('Failed to load users')}
              </p>
            ) : null}
            {updateUserMutation.isError && !editingUser ? (
              <p className="callout callout--danger">
                {updateUserMutation.error instanceof Error
                  ? updateUserMutation.error.message
                  : l('Failed to update user')}
              </p>
            ) : null}
            {deleteUserMutation.isError ? (
              <p className="callout callout--danger">
                {deleteUserMutation.error instanceof Error
                  ? deleteUserMutation.error.message
                  : l('Failed to delete user')}
              </p>
            ) : null}

            <div className="settings-user-list">
              {shouldShowUserSkeleton ? <p className="muted">{l('Loading users…')}</p> : null}

              {!shouldShowUserSkeleton && !usersQuery.isError && users.length === 0 ? (
                <EmptyState
                  description={l('Create a user to grant access to this server.')}
                  title={l('No users yet.')}
                />
              ) : null}

              {shouldShowUserSkeleton
                ? USER_SKELETON_KEYS.slice(0, USER_SKELETON_COUNT).map((key) => (
                    <SettingsUserCardSkeleton key={key} />
                  ))
                : null}

              {!shouldShowUserSkeleton
                ? users.map((user) => {
                    const nickname = user.nickname.trim()
                    const rolePresentation = getUserRolePresentation(user)
                    const canEditUser = canManageUser(currentUser, user)
                    const canDeleteUser = canEditUser
                    const canToggleUser = canEditUser

                    return (
                      <article className="settings-user-card" key={user.id}>
                        <div aria-hidden="true" className="settings-user-card__avatar">
                          <span>{getUserInitial(user)}</span>
                        </div>

                        <div className="settings-user-card__body">
                          <div className="settings-user-card__header">
                            <div className="settings-user-card__identity">
                              <HoverTooltip
                                className="settings-user-card__nickname-wrap"
                                content={nickname || '—'}
                              >
                                <strong
                                  className={
                                    nickname
                                      ? 'settings-user-card__nickname'
                                      : 'settings-user-card__nickname settings-user-card__nickname--empty'
                                  }
                                >
                                  {nickname || '—'}
                                </strong>
                              </HoverTooltip>
                              <div className="settings-user-card__identity-meta">
                                <StatusPill
                                  size="compact"
                                  status={l(rolePresentation.label)}
                                  tone={rolePresentation.tone}
                                />
                                {user.id === currentUser.id ? (
                                  <span className="settings-user-card__self-badge">{l('You')}</span>
                                ) : null}
                              </div>
                            </div>

                            <div className="settings-user-card__toolbar">
                              {canToggleUser ? (
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
                                  <span className="settings-user-card__switch-track">
                                    <span className="settings-user-card__switch-copy settings-user-card__switch-copy--off">
                                      {l('Off')}
                                    </span>
                                    <span className="settings-user-card__switch-copy settings-user-card__switch-copy--on">
                                      {l('On')}
                                    </span>
                                  </span>
                                </label>
                              ) : null}

                              {canEditUser ? (
                                <UserActionsMenu
                                  isDeleteDisabled={!canDeleteUser || deleteUserMutation.isPending}
                                  isDeletePending={
                                    deleteUserMutation.isPending &&
                                    deleteUserMutation.variables === user.id
                                  }
                                  onDeleteUser={(selectedUser) => {
                                    deleteUserMutation.reset()
                                    setPendingConfirmation({
                                      kind: 'delete-user',
                                      user: selectedUser,
                                    })
                                  }}
                                  onEditUser={setEditingUser}
                                  user={user}
                                />
                              ) : null}
                            </div>
                          </div>
                          <p className="settings-user-card__account" title={user.username}>
                            {user.username}
                          </p>
                        </div>
                      </article>
                    )
                  })
                : null}
            </div>
          </section>

          <section className="settings-section settings-section--libraries">
            <div className="section-heading">
              <div>
                <h3>{l('Library Management')}</h3>
              </div>
              <button
                className="button button--toolbar settings-section__create-button"
                onClick={() => setIsCreateLibraryOpen(true)}
                type="button"
              >
                <span>{l('Create Library')}</span>
              </button>
            </div>

            {deleteLibraryMutation.isError ? (
              <p className="callout callout--danger">
                {deleteLibraryMutation.error instanceof Error
                  ? deleteLibraryMutation.error.message
                  : l('Failed to delete library')}
              </p>
            ) : null}

            {shouldShowLibrarySkeleton || libraries.length > 0 ? (
              <div className="settings-library-list">
                {shouldShowLibrarySkeleton
                  ? LIBRARY_SKELETON_KEYS.slice(0, LIBRARY_SKELETON_COUNT).map((key) => (
                      <SettingsLibraryCardSkeleton key={key} rootPathLabel={l('Root path')} />
                    ))
                  : libraries.map((library) => {
                      const libraryDescription = library.description ?? l('No description')
                      const libraryDetail = libraryDetailsById.get(library.id)
                      const lastScan = libraryDetail?.last_scan ?? null
                      const lastScanStatusLabel = getScanStatusLabel(lastScan)
                      const lastScanStatusTone = getScanStatusTone(lastScan)
                      const isTriggeringScan =
                        scanMutation.isPending && scanMutation.variables === library.id
                      const isDeletingLibrary =
                        deleteLibraryMutation.isPending &&
                        deleteLibraryMutation.variables === library.id
                      const isScanActive =
                        lastScan?.status === 'pending' || lastScan?.status === 'running'
                      return (
                        <article className="settings-library-card" key={library.id}>
                          <div aria-hidden="true" className="settings-library-card__backdrop">
                            <span className="settings-library-card__backdrop-glow" />
                          </div>

                          <div className="settings-library-card__body">
                            <div className="settings-library-card__header">
                              <HoverTooltip
                                className="settings-library-card__title-wrap"
                                content={library.name}
                              >
                                <strong className="settings-library-card__title">
                                  {library.name}
                                </strong>
                              </HoverTooltip>
                              <div className="settings-library-card__header-actions">
                                <span
                                  className={`settings-library-card__scan-badge settings-library-card__scan-badge--${lastScanStatusTone}`}
                                >
                                  <span
                                    aria-hidden="true"
                                    className="settings-library-card__scan-dot"
                                  />
                                  {lastScanStatusLabel}
                                </span>
                                <LibraryActionsMenu
                                  className="settings-library-card__menu"
                                  isDeleteDisabled={
                                    deleteLibraryMutation.isPending ||
                                    isTriggeringScan ||
                                    isScanActive
                                  }
                                  isDeletePending={isDeletingLibrary}
                                  isScanDisabled={isTriggeringScan || isScanActive}
                                  isScanPending={isTriggeringScan}
                                  library={library}
                                  onDeleteLibrary={(selectedLibrary) => {
                                    deleteLibraryMutation.reset()
                                    setPendingConfirmation({
                                      kind: 'delete-library',
                                      library: selectedLibrary,
                                    })
                                  }}
                                  onEditLibrary={setEditingLibrary}
                                  onScanLibrary={(selectedLibrary) =>
                                    scanMutation.mutate(selectedLibrary.id)
                                  }
                                />
                              </div>
                            </div>
                            <HoverTooltip
                              className="settings-library-card__description-wrap"
                              content={libraryDescription}
                            >
                              <p className="settings-library-card__description">
                                {libraryDescription}
                              </p>
                            </HoverTooltip>
                            <p className="settings-library-card__language-note">
                              {l('Metadata language: {{language}}', {
                                language: library.metadata_language,
                              })}
                            </p>

                            <div className="settings-library-card__path-block">
                              <span className="settings-library-card__path-label">
                                {l('Root path')}
                              </span>
                              <HoverTooltip
                                className="settings-library-card__path-value"
                                content={library.root_path}
                              >
                                <code className="settings-library-card__path">
                                  {library.root_path}
                                </code>
                              </HoverTooltip>
                            </div>
                          </div>
                        </article>
                      )
                    })}
              </div>
            ) : (
              <EmptyState
                description={l('Create a library to start scanning and organizing media.')}
                title={l('No libraries yet.')}
              />
            )}
          </section>
        </div>
      </HomeDashboardShell>

      <UserEditorModal
        currentUserIsPrimaryAdmin={currentUser.is_primary_admin}
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

      <LibraryEditorModal
        error={activeLibraryModalError}
        isOpen={editingLibrary !== null}
        isSubmitting={updateLibraryMutation.isPending}
        library={editingLibrary}
        onClose={() => {
          setEditingLibrary(null)
          updateLibraryMutation.reset()
        }}
        onUpdate={(libraryId, input) => updateLibraryMutation.mutateAsync({ libraryId, input })}
      />

      <CreateLibraryModal
        error={
          createLibraryMutation.error instanceof Error ? createLibraryMutation.error.message : null
        }
        isOpen={isCreateLibraryOpen}
        isSubmitting={createLibraryMutation.isPending}
        onClose={() => {
          setIsCreateLibraryOpen(false)
          createLibraryMutation.reset()
        }}
        onSubmit={(input) => createLibraryMutation.mutateAsync(input)}
      />

      <ConfirmActionModal
        confirmLabel={confirmationCopy?.confirmLabel ?? l('Confirm')}
        description={confirmationCopy?.description ?? ''}
        error={confirmationError}
        isOpen={pendingConfirmation !== null}
        isSubmitting={isConfirmationSubmitting}
        onClose={() => {
          setPendingConfirmation(null)
          deleteUserMutation.reset()
          deleteLibraryMutation.reset()
        }}
        onConfirm={() => {
          if (!pendingConfirmation) {
            return
          }

          if (pendingConfirmation.kind === 'delete-user') {
            deleteUserMutation.mutate(pendingConfirmation.user.id, {
              onSuccess: () => setPendingConfirmation(null),
            })
            return
          }

          deleteLibraryMutation.mutate(pendingConfirmation.library.id, {
            onSuccess: () => setPendingConfirmation(null),
          })
        }}
        title={confirmationCopy?.title ?? l('Confirm action')}
      />
    </>
  )
}
