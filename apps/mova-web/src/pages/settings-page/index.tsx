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
import type {
  CreateLibraryInput,
  Library,
  LibraryDetail,
  MediaItemListResponse,
  UserAccount,
} from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import { ConfirmActionModal } from '../../components/confirm-action-modal'
import { CreateLibraryModal } from '../../components/create-library-modal'
import { LibraryEditorModal } from '../../components/library-editor-modal'
import { SettingsGearIcon } from '../../components/settings-gear-icon'
import { StatusPill } from '../../components/status-pill'
import { UserEditorModal } from '../../components/user-editor-modal'
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
  getScanStatusSummary,
  getScanStatusTone,
} from '../../lib/settings-admin'
import { getUserDisplayName, getUserInitial } from '../../lib/user-identity'

const USER_SKELETON_COUNT = 4
const LIBRARY_SKELETON_COUNT = 3
const USER_SKELETON_KEYS = ['user-a', 'user-b', 'user-c', 'user-d'] as const
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
          <span className="settings-user-card__control settings-user-card__control--icon skeleton-shimmer" />
        </div>
      </div>
    </div>
  </article>
)

const SettingsLibraryCardSkeleton = () => (
  <article aria-hidden="true" className="settings-library-card settings-library-card--loading">
    <div aria-hidden="true" className="settings-library-card__backdrop">
      <span className="settings-library-card__backdrop-glow" />
    </div>

    <div className="settings-library-card__body">
      <div className="settings-library-card__header">
        <span className="settings-library-card__button settings-library-card__button--icon skeleton-shimmer" />
      </div>

      <span className="settings-library-card__line settings-library-card__line--title skeleton-shimmer" />
      <span className="settings-library-card__line settings-library-card__line--description skeleton-shimmer" />
      <span className="settings-library-card__line settings-library-card__line--description-alt skeleton-shimmer" />
      <span className="settings-library-card__line settings-library-card__line--meta skeleton-shimmer" />

      <div className="settings-library-card__path-block">
        <span className="settings-library-card__path-label">Root path</span>
        <span className="settings-library-card__path settings-library-card__path--loading skeleton-shimmer" />
      </div>
    </div>

    <div className="settings-library-card__actions">
      <span className="settings-library-card__button skeleton-shimmer" />
      <span className="settings-library-card__button skeleton-shimmer" />
    </div>
  </article>
)

export const SettingsPage = () => {
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
      queryClient.setQueryData<MediaItemListResponse>(
        ['home-library-shelf', createdLibrary.id],
        nextLibraryCache.homeLibraryShelf,
      )

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['library', createdLibrary.id] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-detail', createdLibrary.id] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-shelf', createdLibrary.id] }),
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
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['library', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['library-media', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-detail', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-shelf', libraryId] }),
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
      queryClient.removeQueries({ queryKey: ['home-library-shelf', libraryId] })

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['library', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['library-media', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-shelf'] }),
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
        queryClient.invalidateQueries({ queryKey: ['home-library-shelf', libraryId] }),
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
    return <p className="callout callout--danger">Admin permission required.</p>
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
  const canManageAdminAccounts = currentUser.is_primary_admin

  return (
    <div className="settings-shell">
      <section className="settings-hero">
        <div className="settings-hero__badge">
          <SettingsGearIcon className="settings-hero__icon" />
        </div>
        <div className="settings-hero__copy">
          <p className="eyebrow">Admin Settings</p>
          <h2>Server Settings</h2>
        </div>
      </section>

      <section className="settings-section settings-section--users">
        <div className="section-heading">
          <div>
            <h3>User Management</h3>
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
          {shouldShowUserSkeleton ? <p className="muted">Loading users…</p> : null}

          {shouldShowUserSkeleton
            ? USER_SKELETON_KEYS.slice(0, USER_SKELETON_COUNT).map((key) => (
                <SettingsUserCardSkeleton key={key} />
              ))
            : null}

          {!shouldShowUserSkeleton
            ? users.map((user) => {
                const displayName = getUserDisplayName(user)
                const showUsername = displayName !== user.username
                const roleLabel = user.is_primary_admin
                  ? 'Primary Admin'
                  : user.role === 'admin'
                    ? 'Administrator'
                    : 'Member'
                const canManageThisUser = user.role === 'viewer' || canManageAdminAccounts
                const canEditUser =
                  canManageThisUser && user.id !== currentUser.id && !user.is_primary_admin
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
                          <strong>{displayName}</strong>
                          {showUsername ? <p className="muted">@{user.username}</p> : null}
                          <div className="settings-user-card__identity-meta">
                            <StatusPill status={roleLabel} />
                            {user.id === currentUser.id ? (
                              <span className="settings-user-card__self-badge">You</span>
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
                                  Off
                                </span>
                                <span className="settings-user-card__switch-copy settings-user-card__switch-copy--on">
                                  On
                                </span>
                              </span>
                            </label>
                          ) : null}

                          {canEditUser ? (
                            <button
                              aria-label={`Edit ${user.username}`}
                              className="settings-user-card__edit-icon"
                              onClick={() => setEditingUser(user)}
                              type="button"
                            >
                              <svg
                                aria-hidden="true"
                                fill="none"
                                focusable="false"
                                viewBox="0 0 24 24"
                              >
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
                          ) : null}

                          {canDeleteUser ? (
                            <button
                              aria-label={`Delete ${user.username}`}
                              className="settings-user-card__delete-icon"
                              disabled={deleteUserMutation.isPending}
                              onClick={() => {
                                deleteUserMutation.reset()
                                setPendingConfirmation({
                                  kind: 'delete-user',
                                  user,
                                })
                              }}
                              type="button"
                            >
                              <svg
                                aria-hidden="true"
                                fill="none"
                                focusable="false"
                                viewBox="0 0 24 24"
                              >
                                <path
                                  d="M9 4.5H15M5.5 7H18.5M8 7V18.5C8 19.05 8.45 19.5 9 19.5H15C15.55 19.5 16 19.05 16 18.5V7M10.5 10.5V16M13.5 10.5V16"
                                  stroke="currentColor"
                                  strokeLinecap="round"
                                  strokeLinejoin="round"
                                  strokeWidth="1.7"
                                />
                              </svg>
                            </button>
                          ) : null}
                        </div>
                      </div>
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
            <h3>Library Management</h3>
          </div>
          <button
            className="button button--primary button--toolbar"
            onClick={() => setIsCreateLibraryOpen(true)}
            type="button"
          >
            <span>Create Library</span>
          </button>
        </div>

        {deleteLibraryMutation.isError ? (
          <p className="callout callout--danger">
            {deleteLibraryMutation.error instanceof Error
              ? deleteLibraryMutation.error.message
              : 'Failed to delete library'}
          </p>
        ) : null}

        {shouldShowLibrarySkeleton || libraries.length > 0 ? (
          <div className="settings-library-list">
            {shouldShowLibrarySkeleton ? <p className="muted">Loading libraries…</p> : null}

            {shouldShowLibrarySkeleton
              ? LIBRARY_SKELETON_KEYS.slice(0, LIBRARY_SKELETON_COUNT).map((key) => (
                  <SettingsLibraryCardSkeleton key={key} />
                ))
              : null}

            {!shouldShowLibrarySkeleton
              ? libraries.map((library) => {
                  const libraryDetail = libraryDetailsById.get(library.id)
                  const lastScan = libraryDetail?.last_scan ?? null
                  const lastScanStatusLabel = getScanStatusLabel(lastScan)
                  const lastScanStatusTone = getScanStatusTone(lastScan)

                  return (
                    <article className="settings-library-card" key={library.id}>
                      <div aria-hidden="true" className="settings-library-card__backdrop">
                        <span className="settings-library-card__backdrop-glow" />
                      </div>

                      <div className="settings-library-card__body">
                        <div className="settings-library-card__header">
                          <button
                            aria-label={`Edit ${library.name}`}
                            className="settings-library-card__edit-icon"
                            onClick={() => setEditingLibrary(library)}
                            type="button"
                          >
                            <svg
                              aria-hidden="true"
                              fill="none"
                              focusable="false"
                              viewBox="0 0 24 24"
                            >
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

                        <strong className="settings-library-card__title">{library.name}</strong>
                        <p className="settings-library-card__description">
                          {library.description ?? 'No description'}
                        </p>
                        <p className="settings-library-card__language-note">
                          Metadata language: {library.metadata_language}
                        </p>

                        <div className="settings-library-card__scan">
                          <div className="settings-library-card__scan-header">
                            <span className="settings-library-card__path-label">Latest Scan</span>
                            <span
                              className={`settings-library-card__scan-badge settings-library-card__scan-badge--${lastScanStatusTone}`}
                            >
                              {lastScanStatusLabel}
                            </span>
                          </div>
                          <p className="settings-library-card__scan-copy">
                            {getScanStatusSummary(lastScan)}
                          </p>
                        </div>

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
                            deleteLibraryMutation.reset()
                            setPendingConfirmation({
                              kind: 'delete-library',
                              library,
                            })
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
                  )
                })
              : null}
          </div>
        ) : null}
      </section>

      <UserEditorModal
        currentUserId={currentUser.id}
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
        confirmLabel={confirmationCopy?.confirmLabel ?? 'Confirm'}
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
        title={confirmationCopy?.title ?? 'Confirm action'}
      />
    </div>
  )
}
