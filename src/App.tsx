import { useEffect, useRef, useState } from 'react'
import { Header } from './components/Header'
import { navItems } from './data/homeContent'
import { ApiDocsPage } from './pages/ApiDocsPage'
import { HomePage } from './pages/HomePage'
import './App.css'

type Page = 'home' | 'api'

const getRoutePage = (): Page => {
  if (typeof window === 'undefined') {
    return 'home'
  }

  return window.location.pathname.replace(/\/$/, '') === '/api' || window.location.hash === '#api'
    ? 'api'
    : 'home'
}

function App() {
  const [page, setPage] = useState<Page>(() => getRoutePage())
  const [activeSection, setActiveSection] = useState('home')
  const [isHeaderHidden, setIsHeaderHidden] = useState(false)
  const lastScrollY = useRef(0)

  useEffect(() => {
    if (page !== 'home') {
      return undefined
    }

    const sectionIds = navItems.map((item) => item.id)
    const observer = new IntersectionObserver(
      (entries) => {
        const visibleEntry = entries
          .filter((entry) => entry.isIntersecting)
          .sort((a, b) => b.intersectionRatio - a.intersectionRatio)[0]

        if (visibleEntry?.target.id) {
          setActiveSection(visibleEntry.target.id)
        }
      },
      { rootMargin: '-35% 0px -50% 0px', threshold: [0.1, 0.35, 0.6] },
    )

    sectionIds.forEach((id) => {
      const section = document.getElementById(id)
      if (section) {
        observer.observe(section)
      }
    })

    return () => observer.disconnect()
  }, [page])

  useEffect(() => {
    const syncPageFromLocation = () => {
      const nextPage = getRoutePage()
      setPage(nextPage)

      if (nextPage === 'api' && window.location.hash === '#api') {
        window.history.replaceState(null, '', '/api')
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
        activeSection={page === 'api' ? 'api' : activeSection}
        isHidden={isHeaderHidden}
        onNavigate={handleHeaderNavigate}
      />

      <main>
        {page === 'api' ? (
          <ApiDocsPage onNavigate={scrollToSection} />
        ) : (
          <HomePage onNavigate={scrollToSection} onOpenApiDocs={openApiDocs} />
        )}
      </main>
    </div>
  )
}

export default App
