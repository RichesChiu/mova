import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useOutletContext } from 'react-router-dom'
import { deleteLibrary, getHome, scanLibrary, updateLibrary } from '../../api/client'
import type { Library, LibraryDetail } from '../../api/types'
import type { AppShellOutletContext } from '../../components/app-shell'
import { getLibraryScanRuntime } from '../../components/app-shell/scan-runtime'
import { ConfirmActionModal } from '../../components/confirm-action-modal'
import type { ContinueWatchingCardData } from '../../components/continue-watching-card'
import { LibraryEditorModal } from '../../components/library-editor-modal'
import { useI18n } from '../../i18n'
import { getVisibleHomeLibraries, shouldRenderHomeRecentlyAdded } from '../../lib/home-sections'
import { mediaItemDetailPath, mediaItemPrimaryPath } from '../../lib/media-routes'
import {
  buildDeletedLibraryCacheState,
  buildDeleteLibraryConfirmationCopy,
  buildTriggeredScanCacheState,
  buildUpdatedLibraryCacheState,
} from '../../lib/settings-admin'
import { canManageServer } from '../../lib/viewer'
import { ContinueWatchingSection } from './continue-watching-section'
import { DashboardPageHeader } from './dashboard-page-header'
import { HomeDashboardShell } from './home-dashboard-shell'
import { LibrariesSection } from './libraries-section'
import { LibraryContentSections } from './library-content-sections'
import type { HomeLibraryModuleData } from './types'

// Playback progress is stored in seconds, but the card UI needs a clamped percentage.
const progressPercent = (position: number, duration: number | null) => {
  if (!duration || duration <= 0) {
    return 0
  }

  return Math.max(0, Math.min(100, Math.round((position / duration) * 100)))
}

export const HomePage = () => {
  const { l } = useI18n()
  const { currentUser, scanRuntimeByLibrary } = useOutletContext<AppShellOutletContext>()
  const queryClient = useQueryClient()
  const [editingLibrary, setEditingLibrary] = useState<Library | null>(null)
  const [pendingDeleteLibrary, setPendingDeleteLibrary] = useState<Library | null>(null)
  const homeQuery = useQuery({
    queryKey: ['home'],
    queryFn: getHome,
  })
  const libraries = homeQuery.data?.libraries.map((entry) => entry.library) ?? []
  const librariesLoading = homeQuery.isLoading
  const isAdmin = canManageServer(currentUser)

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
        currentLibraryDetail: queryClient.getQueryData<LibraryDetail>(['library', libraryId]),
        currentHomeLibraryDetail: queryClient.getQueryData<LibraryDetail>([
          'home-library-detail',
          libraryId,
        ]),
      })

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
      queryClient.removeQueries({ queryKey: ['library', libraryId] })
      queryClient.removeQueries({ queryKey: ['library-media', libraryId] })
      queryClient.removeQueries({ queryKey: ['home-library-detail', libraryId] })

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['libraries'] }),
        queryClient.invalidateQueries({ queryKey: ['library', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['library-media', libraryId] }),
        queryClient.invalidateQueries({ queryKey: ['recently-added-by-library'] }),
        queryClient.invalidateQueries({ queryKey: ['home'] }),
      ])
    },
  })

  const continueWatchingItems = homeQuery.data?.continue_watching ?? []
  const homeLibraries = getVisibleHomeLibraries(libraries)
  const recentlyAddedGroups = homeQuery.data?.recently_added ?? []
  const shouldShowRecentlyAdded = shouldRenderHomeRecentlyAdded({
    hasError: homeQuery.isError,
    isLoading: homeQuery.isLoading,
    libraryCount: libraries.length,
  })
  const recentlyAddedByLibraryId = new Map(
    recentlyAddedGroups.map((group) => [group.library.id, group.items]),
  )
  const homeLibraryById = new Map(
    (homeQuery.data?.libraries ?? []).map((entry) => [entry.library.id, entry]),
  )
  const libraryModules: HomeLibraryModuleData[] = homeLibraries.map((library) => ({
    detail: homeLibraryById.get(library.id)?.library ?? null,
    detailError: homeQuery.error instanceof Error ? homeQuery.error : null,
    detailLoading: homeQuery.isLoading,
    library,
    recentItems:
      recentlyAddedByLibraryId.get(library.id) ??
      homeLibraryById.get(library.id)?.preview_items ??
      [],
    scanRuntime: getLibraryScanRuntime(scanRuntimeByLibrary, library.id),
  }))
  const continueWatchingCards: ContinueWatchingCardData[] = continueWatchingItems.map((entry) => {
    const percent = progressPercent(
      entry.playback_progress.position_seconds,
      entry.playback_progress.duration_seconds,
    )
    const seasonNumber = typeof entry.season_number === 'number' ? entry.season_number : null
    const episodeNumber = typeof entry.episode_number === 'number' ? entry.episode_number : null
    const hasEpisodeContext = seasonNumber !== null && episodeNumber !== null
    const episodeLabel = hasEpisodeContext
      ? `S${String(seasonNumber).padStart(2, '0')} · E${String(episodeNumber).padStart(2, '0')}`
      : null
    const continuePath = hasEpisodeContext
      ? `${mediaItemDetailPath(entry.media_item.id)}?season=${seasonNumber}`
      : mediaItemPrimaryPath(entry.media_item)
    const artwork = hasEpisodeContext ? entry.episode_poster_path : entry.media_item.poster_path
    const title = hasEpisodeContext
      ? (entry.episode_title ?? entry.media_item.title)
      : entry.media_item.title
    const placeholderLabel = hasEpisodeContext ? `${seasonNumber}-${episodeNumber}` : l('Movies')

    return {
      artworkAlt: l('{{title}} artwork', { title: entry.media_item.title }),
      artworkSrc: artwork,
      href: continuePath,
      id: entry.playback_progress.id,
      metaLabel: episodeLabel,
      placeholderLabel,
      progressPercent: percent,
      title,
    }
  })
  const continueWatchingErrorMessage = homeQuery.isError
    ? homeQuery.error instanceof Error
      ? homeQuery.error.message
      : l('Failed to load continue watching list')
    : null
  const recentlyAddedErrorMessage = homeQuery.isError
    ? homeQuery.error instanceof Error
      ? homeQuery.error.message
      : l('Failed to load recently added media')
    : null
  const libraryActionErrorMessage = scanMutation.isError
    ? scanMutation.error instanceof Error
      ? scanMutation.error.message
      : l('Failed to scan library')
    : null
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

  return (
    <>
      <HomeDashboardShell ariaLabel={l('Home')} currentUser={currentUser}>
        <div className="home-dashboard__content">
          <DashboardPageHeader>
            <h2>{l('Home')}</h2>
          </DashboardPageHeader>

          <ContinueWatchingSection
            errorMessage={continueWatchingErrorMessage}
            isLoading={homeQuery.isLoading}
            items={continueWatchingCards}
          />

          <LibrariesSection
            actionErrorMessage={libraryActionErrorMessage}
            canManageLibraries={isAdmin}
            isLoading={librariesLoading}
            libraryModules={libraryModules}
            pendingScanLibraryId={scanMutation.isPending ? (scanMutation.variables ?? null) : null}
            totalLibraryCount={libraries.length}
            onDeleteLibrary={(library) => {
              deleteLibraryMutation.reset()
              setPendingDeleteLibrary(library)
            }}
            onEditLibrary={(library) => {
              updateLibraryMutation.reset()
              setEditingLibrary(library)
            }}
            onScanLibrary={(library) => {
              scanMutation.reset()
              scanMutation.mutate(library.id)
            }}
          />

          {shouldShowRecentlyAdded ? (
            <LibraryContentSections
              errorMessage={recentlyAddedErrorMessage}
              groups={recentlyAddedGroups}
              isLoading={homeQuery.isLoading}
            />
          ) : null}
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
