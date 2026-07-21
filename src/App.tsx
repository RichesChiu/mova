import { useEffect, useRef, useState } from 'react'
import { Header } from './components/Header'
import { SiteFooter } from './components/SiteFooter'
import { ApiDocsPage } from './pages/ApiDocsPage'
import { HomePage } from './pages/HomePage'
import { PrivacyPage } from './pages/PrivacyPage'
import { SupportPage } from './pages/SupportPage'
import { useI18n } from './i18n-context'
import './App.css'

type Page = 'home' | 'api' | 'privacy' | 'support'

const pagePaths: Record<Exclude<Page, 'home'>, string> = {
  api: '/api',
  privacy: '/privacy',
  support: '/support',
}

const getRoutePage = (): Page => {
  if (typeof window === 'undefined') {
    return 'home'
  }

  const path = window.location.pathname.replace(/\/$/, '')
  const hashRoute = window.location.hash.replace(/^#/, '')

  if (path === '/api' || hashRoute === 'api') return 'api'
  if (path === '/privacy' || hashRoute === 'privacy') return 'privacy'
  if (path === '/support' || hashRoute === 'support') return 'support'
  return 'home'
}

function App() {
  const { language } = useI18n()
  const [page, setPage] = useState<Page>(() => getRoutePage())
  const [isHeaderHidden, setIsHeaderHidden] = useState(false)
  const lastScrollY = useRef(0)

  useEffect(() => {
    const titles: Record<Page, { zh: string; en: string }> = {
      home: { zh: 'MOVA 自托管媒体服务', en: 'MOVA Self-hosted Media Service' },
      api: { zh: 'API 文档 · MOVA', en: 'API Documentation · MOVA' },
      privacy: { zh: '隐私政策 · MOVA', en: 'Privacy Policy · MOVA' },
      support: { zh: '支持 · MOVA', en: 'Support · MOVA' },
    }

    document.title = titles[page][language]
  }, [language, page])

  useEffect(() => {
    const syncPageFromLocation = () => {
      const nextPage = getRoutePage()
      setPage(nextPage)

      if (nextPage !== 'home' && window.location.hash === `#${nextPage}`) {
        window.history.replaceState(null, '', pagePaths[nextPage])
      }
    }

    syncPageFromLocation()
    window.addEventListener('popstate', syncPageFromLocation)
    window.addEventListener('hashchange', syncPageFromLocation)

    return () => {
      window.removeEventListener('popstate', syncPageFromLocation)
      window.removeEventListener('hashchange', syncPageFromLocation)
    }
  }, [])

  useEffect(() => {
    let frameId = 0

    const updateHeaderVisibility = () => {
      const currentScrollY = window.scrollY
      const scrollDelta = currentScrollY - lastScrollY.current
      const isAtTop = currentScrollY <= 8

      if (isAtTop) {
        setIsHeaderHidden(false)
        lastScrollY.current = currentScrollY
      } else if (Math.abs(scrollDelta) > 6) {
        setIsHeaderHidden(scrollDelta > 0 && currentScrollY > 80)
        lastScrollY.current = currentScrollY
      }

      frameId = 0
    }

    const handleScroll = () => {
      if (frameId === 0) {
        frameId = window.requestAnimationFrame(updateHeaderVisibility)
      }
    }

    lastScrollY.current = window.scrollY
    window.addEventListener('scroll', handleScroll, { passive: true })

    return () => {
      window.removeEventListener('scroll', handleScroll)
      if (frameId !== 0) {
        window.cancelAnimationFrame(frameId)
      }
    }
  }, [])

  const openApiDocs = () => {
    if (window.location.pathname !== '/api') {
      window.history.pushState(null, '', '/api')
    }

    setPage('api')
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }

  const openPage = (nextPage: Exclude<Page, 'home'>) => {
    const path = pagePaths[nextPage]
    if (window.location.pathname !== path) {
      window.history.pushState(null, '', path)
    }

    setPage(nextPage)
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }

  const handleHeaderNavigate = (targetId: string) => {
    if (targetId === 'api') {
      openApiDocs()
      return
    }

    scrollToSection(targetId)
  }

  const scrollToSection = (sectionId: string) => {
    if (page !== 'home') {
      window.history.pushState(null, '', '/')
      setPage('home')
    }

    window.requestAnimationFrame(() => {
      document.getElementById(sectionId)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    })
  }

  return (
    <div className="app-shell">
      <Header
        activeSection={page === 'home' ? 'home' : page === 'api' ? 'api' : ''}
        isHidden={isHeaderHidden}
        onNavigate={handleHeaderNavigate}
      />

      <main>
        {page === 'api' ? (
          <ApiDocsPage onNavigate={handleHeaderNavigate} />
        ) : page === 'privacy' ? (
          <PrivacyPage />
        ) : page === 'support' ? (
          <SupportPage onOpenPrivacy={() => openPage('privacy')} />
        ) : (
          <HomePage onOpenApiDocs={openApiDocs} />
        )}
      </main>

      <SiteFooter
        onOpenHome={() => scrollToSection('home')}
        onOpenPrivacy={() => openPage('privacy')}
        onOpenSupport={() => openPage('support')}
      />
    </div>
  )
}

export default App
