import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useOutletContext } from 'react-router-dom'
import {
  deleteLibrary,
  getLibrary,
  listRecentlyAddedByLibrary,
  scanLibrary,
  updateLibrary,
} from '../../api/client'
import type { Library, LibraryDetail } from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import { getLibraryScanRuntime } from '../../components/app-shell/scan-runtime'
import { ConfirmActionModal } from '../../components/confirm-action-modal'
import { EmptyState } from '../../components/empty-state'
import { LibraryEditorModal } from '../../components/library-editor-modal'
import {
  LibrarySpotlightCard,
  LibrarySpotlightCardSkeleton,
} from '../../components/library-spotlight-card'
import { useI18n } from '../../i18n'
import {
  buildDeletedLibraryCacheState,
  buildDeleteLibraryConfirmationCopy,
  buildTriggeredScanCacheState,
  buildUpdatedLibraryCacheState,
  mergeTriggeredScanLibraryDetail,
  mergeUpdatedLibraryDetail,
} from '../../lib/settings-admin'
import { canManageServer } from '../../lib/viewer'
import { DashboardPageHeader } from '../home-page/dashboard-page-header'
import { HomeDashboardShell } from '../home-page/home-dashboard-shell'

const LIBRARY_SKELETON_KEYS = ['library-a', 'library-b', 'library-c', 'library-d'] as const

export const LibrariesPage = () => {
  const { formatNumber, l } = useI18n()
  const { currentUser, libraries, librariesLoading, scanRuntimeByLibrary } =
    useOutletContext<AppShellOutletContext>()
  const queryClient = useQueryClient()
  const [editingLibrary, setEditingLibrary] = useState<Library | null>(null)
  const [pendingDeleteLibrary, setPendingDeleteLibrary] = useState<Library | null>(null)
  const canManageLibraries = canManageServer(currentUser)
  const libraryDetailQueries = useQueries({
    queries: libraries.map((library) => ({
      queryKey: ['libraries-page-detail', library.id],
      queryFn: () => getLibrary(library.id),
    })),
  })
  const recentPreviewQuery = useQuery({
    enabled: libraries.length > 0,
    queryKey: ['libraries-page-recently-added', 8],
    queryFn: () => listRecentlyAddedByLibrary({ limit: 8 }),
  })
  const recentPreviewByLibraryId = new Map(
    (recentPreviewQuery.data ?? []).map((group) => [group.library.id, group.items]),
  )

  const scanMutation = useMutation({
    mutationFn: (libraryId: number) => scanLibrary(libraryId),
    onSuccess: async (scanJob, libraryId) => {
      const fallbackLibrary = libraries.find((library) => library.id === libraryId)

      if (fallbackLibrary) {
        const nextScanCache = buildTriggeredScanCacheState({
          fallbackLibrary,
          currentLibraryDetail: queryClient.getQueryData<LibraryDetail>([
            'libraries-page-detail',
            libraryId,
          ]),
          currentHomeLibraryDetail: queryClient.getQueryData<LibraryDetail>([
            'home-library-detail',
            libraryId,
          ]),
          scanJob,
        })

        queryClient.setQueryData<LibraryDetail>(
          ['libraries-page-detail', libraryId],
          nextScanCache.libraryDetail,
        )
        queryClient.setQueryData<LibraryDetail>(
          ['home-library-detail', libraryId],
          nextScanCache.homeLibraryDetail,
        )
        queryClient.setQueryData<LibraryDetail>(['library', libraryId], (current) =>
          mergeTriggeredScanLibraryDetail(current, fallbackLibrary, scanJob),
        )
      }

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['libraries-page-detail', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['library', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['library-media', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-detail', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['home'] }),
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
      const nextLibraryCache = buildUpdatedLibraryCacheState({
        currentLibraries: queryClient.getQueryData<Library[]>(['libraries']),
        updatedLibrary,
        currentLibraryDetail: queryClient.getQueryData<LibraryDetail>([
          'libraries-page-detail',
          libraryId,
        ]),
        currentHomeLibraryDetail: queryClient.getQueryData<LibraryDetail>([
          'home-library-detail',
          libraryId,
        ]),
      })

      queryClient.setQueryData<Library[]>(['libraries'], nextLibraryCache.libraries)
      queryClient.setQueryData<LibraryDetail>(
        ['libraries-page-detail', libraryId],
        nextLibraryCache.libraryDetail,
      )
      queryClient.setQueryData<LibraryDetail>(
        ['home-library-detail', libraryId],
        nextLibraryCache.homeLibraryDetail,
      )
      queryClient.setQueryData<LibraryDetail>(['library', libraryId], (current) =>
        mergeUpdatedLibraryDetail(current, updatedLibrary),
      )

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['libraries-page-detail', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['library', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['library-media', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['home-library-detail', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['home'] }),
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
      queryClient.removeQueries({ queryKey: ['libraries-page-detail', libraryId] })
      queryClient.removeQueries({ queryKey: ['library', libraryId] })
      queryClient.removeQueries({ queryKey: ['library-media', libraryId] })
      queryClient.removeQueries({ queryKey: ['home-library-detail', libraryId] })

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['libraries-page-recently-added'] }),
        queryClient.invalidateQueries({ queryKey: ['recently-added-by-library'] }),
        queryClient.invalidateQueries({ queryKey: ['home'] }),
      ])
    },
  })

  const activeLibraryModalError =
    editingLibrary && updateLibraryMutation.error instanceof Error
      ? updateLibraryMutation.error.message
      : null
  const deleteLibraryConfirmationCopy = pendingDeleteLibrary
    ? buildDeleteLibraryConfirmationCopy(pendingDeleteLibrary)
    : null
  const deleteLibraryConfirmationError =
    pendingDeleteLibrary && deleteLibraryMutation.error instanceof Error
      ? deleteLibraryMutation.error.message
      : null
  const scanErrorMessage = scanMutation.error instanceof Error ? scanMutation.error.message : null

  return (
    <>
      <HomeDashboardShell ariaLabel={l('Libraries')} currentUser={currentUser}>
        <div className="home-dashboard__content home-dashboard__content--libraries">
          <DashboardPageHeader>
            <h2>{l('All Libraries')}</h2>
            <span className="home-dashboard-page-header__meta">
              {formatNumber(libraries.length)} {l('Libraries')}
            </span>
          </DashboardPageHeader>

          <section className="catalog-block libraries-page">
            {recentPreviewQuery.isError ? (
              <p className="callout callout--danger">
                {recentPreviewQuery.error instanceof Error
                  ? recentPreviewQuery.error.message
                  : l('Failed to load recently added media')}
              </p>
            ) : null}
            {scanErrorMessage ? (
              <p className="callout callout--danger">{scanErrorMessage}</p>
            ) : null}

            {librariesLoading ? (
              <div className="libraries-page__grid">
                {LIBRARY_SKELETON_KEYS.map((key) => (
                  <LibrarySpotlightCardSkeleton className="libraries-page__card" key={key} />
                ))}
              </div>
            ) : libraries.length === 0 ? (
              <EmptyState
                description={l(
                  'Create a library in Server Settings to start organizing your media.',
                )}
                title={l('No libraries yet.')}
              />
            ) : (
              <div className="libraries-page__grid">
                {libraries.map((library, index) => {
                  const detail = libraryDetailQueries[index]?.data ?? null
                  const detailError =
                    libraryDetailQueries[index]?.error instanceof Error
                      ? libraryDetailQueries[index].error
                      : null
                  const detailLoading = libraryDetailQueries[index]?.isLoading ?? false

                  return (
                    <LibrarySpotlightCard
                      canManageLibraries={canManageLibraries}
                      className="libraries-page__card"
                      detail={detail}
                      detailError={detailError}
                      detailLoading={detailLoading}
                      isScanPending={
                        scanMutation.isPending && scanMutation.variables === library.id
                      }
                      key={library.id}
                      library={library}
                      onDeleteLibrary={(selectedLibrary) => {
                        deleteLibraryMutation.reset()
                        setPendingDeleteLibrary(selectedLibrary)
                      }}
                      onEditLibrary={(selectedLibrary) => {
                        updateLibraryMutation.reset()
                        setEditingLibrary(selectedLibrary)
                      }}
                      onScanLibrary={(selectedLibrary) => {
                        scanMutation.reset()
                        scanMutation.mutate(selectedLibrary.id)
                      }}
                      recentItems={recentPreviewByLibraryId.get(library.id) ?? []}
                      scanRuntime={getLibraryScanRuntime(scanRuntimeByLibrary, library.id)}
                    />
                  )
                })}
              </div>
            )}
          </section>
        </div>
      </HomeDashboardShell>

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

      <ConfirmActionModal
        confirmLabel={deleteLibraryConfirmationCopy?.confirmLabel ?? l('Confirm')}
        description={deleteLibraryConfirmationCopy?.description ?? ''}
        error={deleteLibraryConfirmationError}
        isOpen={pendingDeleteLibrary !== null}
        isSubmitting={deleteLibraryMutation.isPending}
        onClose={() => {
          setPendingDeleteLibrary(null)
          deleteLibraryMutation.reset()
        }}
        onConfirm={() => {
          if (!pendingDeleteLibrary) {
            return
          }

          deleteLibraryMutation.mutate(pendingDeleteLibrary.id, {
            onSuccess: () => setPendingDeleteLibrary(null),
          })
        }}
        title={deleteLibraryConfirmationCopy?.title ?? l('Confirm action')}
      />
    </>
  )
}
