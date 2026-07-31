import { useEffect, useRef, useState } from 'react'
import { Header } from './components/Header'
import { SiteFooter } from './components/SiteFooter'
import { ApiDocsPage } from './pages/ApiDocsPage'
import { CreditsPage } from './pages/CreditsPage'
import { DeploymentPage } from './pages/DeploymentPage'
import { HomePage } from './pages/HomePage'
import { PrivacyPage } from './pages/PrivacyPage'
import { SupportPage } from './pages/SupportPage'
import { useI18n } from './i18n-context'
import './App.css'

type Page = 'home' | 'deploy' | 'api' | 'credits' | 'privacy' | 'support'

const pagePaths: Record<Exclude<Page, 'home'>, string> = {
  deploy: '/deploy',
  api: '/api',
  credits: '/credits',
  privacy: '/privacy',
  support: '/support',
}

const getRoutePage = (): Page => {
  if (typeof window === 'undefined') {
    return 'home'
  }

  const path = window.location.pathname.replace(/\/$/, '')
  const hashRoute = window.location.hash.replace(/^#/, '')

  if (path === '/deploy' || hashRoute === 'deploy') return 'deploy'
  if (path === '/api' || hashRoute === 'api') return 'api'
  if (path === '/credits' || hashRoute === 'credits') return 'credits'
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
      deploy: { zh: '部署文档 · MOVA', en: 'Deployment Guide · MOVA' },
      api: { zh: 'API 文档 · MOVA', en: 'API Documentation · MOVA' },
      credits: { zh: '鸣谢与数据来源 · MOVA', en: 'Credits & Data Sources · MOVA' },
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
    if (targetId === 'deploy') {
      openPage('deploy')
      return
    }

    if (targetId === 'api') {
      openApiDocs()
      return
    }

    if (targetId === 'support') {
      openPage('support')
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
        activeSection={
          page === 'home'
            ? 'home'
            : page === 'deploy'
              ? 'deploy'
              : page === 'api'
                ? 'api'
                : page === 'support'
                  ? 'support'
                  : ''
        }
        isHidden={isHeaderHidden}
        onNavigate={handleHeaderNavigate}
      />

      <main>
        {page === 'deploy' ? (
          <DeploymentPage onNavigate={handleHeaderNavigate} />
        ) : page === 'api' ? (
          <ApiDocsPage onNavigate={handleHeaderNavigate} />
        ) : page === 'credits' ? (
          <CreditsPage />
        ) : page === 'privacy' ? (
          <PrivacyPage />
        ) : page === 'support' ? (
          <SupportPage onOpenPrivacy={() => openPage('privacy')} />
        ) : (
          <HomePage onOpenDeployment={() => openPage('deploy')} onOpenApiDocs={openApiDocs} />
        )}
      </main>

      <SiteFooter
        onOpenCredits={() => openPage('credits')}
        onOpenHome={() => scrollToSection('home')}
        onOpenPrivacy={() => openPage('privacy')}
      />
    </div>
  )
}

export default App
