import { QueryClientProvider } from '@tanstack/react-query'
import { BrowserRouter, Route, Routes } from 'react-router-dom'
import { AppShell } from './components/app-shell'
import { queryClient } from './lib/query-client'
import { HomePage } from './pages/home-page'
import { LibraryPage } from './pages/library-page'
import { LoginPage } from './pages/login-page'
import { MediaItemPage } from './pages/media-item-page'
import { MediaPlayerPage } from './pages/media-player-page'
import { ProfilePage } from './pages/profile-page'
import { SettingsPage } from './pages/settings-page'

const App = () => {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          <Route path="/media-items/:mediaItemId/play" element={<MediaPlayerPage />} />
          <Route element={<AppShell />}>
            <Route index element={<HomePage />} />
            <Route path="/libraries/:libraryId" element={<LibraryPage />} />
            <Route path="/media-items/:mediaItemId" element={<MediaItemPage />} />
            <Route path="/profile" element={<ProfilePage />} />
            <Route path="/settings" element={<SettingsPage />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  )
}

export default App
