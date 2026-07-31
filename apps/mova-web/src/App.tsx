import { QueryClientProvider } from '@tanstack/react-query'
import { lazy, Suspense } from 'react'
import { BrowserRouter, Route, Routes, useLocation } from 'react-router-dom'
import { AppShell } from './components/app-shell'
import { RouteLoadErrorBoundary } from './components/route-load-error-boundary'
import { useI18n } from './i18n'
import { loadLazyRoute, resetLazyRouteRecovery } from './lib/lazy-route'
import { queryClient } from './lib/query-client'

const AboutPage = lazy(() =>
  loadLazyRoute(() =>
    import('./pages/about-page').then((module) => ({ default: module.AboutPage })),
  ),
)
const ContinuePage = lazy(() =>
  loadLazyRoute(() =>
    import('./pages/continue-page').then((module) => ({ default: module.ContinuePage })),
  ),
)
const HomePage = lazy(() =>
  loadLazyRoute(() => import('./pages/home-page').then((module) => ({ default: module.HomePage }))),
)
const LibrariesPage = lazy(() =>
  loadLazyRoute(() =>
    import('./pages/libraries-page').then((module) => ({ default: module.LibrariesPage })),
  ),
)
const LibraryPage = lazy(() =>
  loadLazyRoute(() =>
    import('./pages/library-page').then((module) => ({ default: module.LibraryPage })),
  ),
)
const LoginPage = lazy(() =>
  loadLazyRoute(() =>
    import('./pages/login-page').then((module) => ({ default: module.LoginPage })),
  ),
)
const MediaItemPage = lazy(() =>
  loadLazyRoute(() =>
    import('./pages/media-item-page').then((module) => ({ default: module.MediaItemPage })),
  ),
)
const MediaPlayerPage = lazy(() =>
  loadLazyRoute(() =>
    import('./pages/media-player-page').then((module) => ({ default: module.MediaPlayerPage })),
  ),
)
const ProfilePage = lazy(() =>
  loadLazyRoute(() =>
    import('./pages/profile-page').then((module) => ({ default: module.ProfilePage })),
  ),
)
const SearchPage = lazy(() =>
  loadLazyRoute(() =>
    import('./pages/search-page').then((module) => ({ default: module.SearchPage })),
  ),
)
const SettingsPage = lazy(() =>
  loadLazyRoute(() =>
    import('./pages/settings-page').then((module) => ({ default: module.SettingsPage })),
  ),
)

const RouteFallback = () => {
  const { l } = useI18n()

  return (
    <div aria-live="polite" className="app-route-fallback" role="status">
      {l('Loading…')}
    </div>
  )
}

const AppRoutes = () => {
  const { l } = useI18n()
  const location = useLocation()

  const reloadApplication = () => {
    resetLazyRouteRecovery()
    window.location.reload()
  }

  return (
    <RouteLoadErrorBoundary
      description={l(
        'The page files could not be loaded. Reload to use the latest application version.',
      )}
      onReload={reloadApplication}
      reloadLabel={l('Reload page')}
      resetKey={`${location.pathname}${location.search}`}
      title={l('Page unavailable')}
    >
      <Suspense fallback={<RouteFallback />}>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          <Route path="/media-items/:mediaItemId/play" element={<MediaPlayerPage />} />
          <Route element={<AppShell />}>
            <Route index element={<HomePage />} />
            <Route path="/about" element={<AboutPage />} />
            <Route path="/libraries" element={<LibrariesPage />} />
            <Route path="/libraries/:libraryId" element={<LibraryPage />} />
            <Route path="/media-items/:mediaItemId" element={<MediaItemPage />} />
            <Route path="/profile" element={<ProfilePage />} />
            <Route path="/search" element={<SearchPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route path="/continue" element={<ContinuePage />} />
          </Route>
        </Routes>
      </Suspense>
    </RouteLoadErrorBoundary>
  )
}

const App = () => {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <AppRoutes />
      </BrowserRouter>
    </QueryClientProvider>
  )
}

export default App
