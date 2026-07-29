import { navItems } from '../data/homeContent'
import { useI18n } from '../i18n-context'
import { HeaderBrandLinks } from './HeaderBrandLinks'
import './Header.css'

export function Header({
  activeSection,
  isHidden,
  onNavigate,
}: {
  activeSection: string
  isHidden: boolean
  onNavigate: (sectionId: string) => void
}) {
  const { language, setLanguage, t } = useI18n()

  return (
    <header className={`site-header${isHidden ? ' site-header-hidden' : ''}`}>
      <button
        className="brand"
        type="button"
        onClick={() => onNavigate('home')}
        aria-label={t('返回 MOVA 首页')}
      >
        <img className="brand-mark" src="/mova-logo-transparent-128.png" width="42" height="42" alt="" />
        <span>MOVA</span>
      </button>

      <nav className="site-nav" aria-label={t('主要导航')}>
        {navItems.map((item) => (
          <button
            key={item.id}
            className={activeSection === item.id ? 'active' : ''}
            type="button"
            onClick={() => onNavigate(item.id)}
          >
            {t(item.label)}
          </button>
        ))}
      </nav>

      <div className="header-actions">
        <HeaderBrandLinks />
        <div className="language-switch" role="group" aria-label={language === 'zh' ? '语言' : 'Language'}>
          <button
            className={language === 'zh' ? 'active' : ''}
            type="button"
            onClick={() => setLanguage('zh')}
            aria-label={t('切换为中文')}
            aria-pressed={language === 'zh'}
          >
            中
          </button>
          <button
            className={language === 'en' ? 'active' : ''}
            type="button"
            onClick={() => setLanguage('en')}
            aria-label={t('切换为英文')}
            aria-pressed={language === 'en'}
          >
            EN
          </button>
        </div>
      </div>
    </header>
  )
}
