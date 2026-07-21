import { dockerUrl, githubUrl, navItems } from '../data/homeContent'
import { useI18n } from '../i18n-context'
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
        {navItems.map((item) => item.id === 'deploy' ? (
          <a key={item.id} href={dockerUrl} target="_blank" rel="noreferrer">
            {t(item.label)}
          </a>
        ) : (
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
        <a
          className="header-icon-link"
          href={githubUrl}
          target="_blank"
          rel="noreferrer"
          aria-label={t('打开 GitHub 仓库')}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path
              fill="currentColor"
              d="M12 2C6.48 2 2 6.58 2 12.26c0 4.53 2.87 8.37 6.84 9.73.5.1.68-.22.68-.49 0-.24-.01-.88-.01-1.73-2.78.62-3.37-1.37-3.37-1.37-.45-1.18-1.11-1.49-1.11-1.49-.91-.64.07-.63.07-.63 1 .07 1.53 1.06 1.53 1.06.9 1.57 2.36 1.12 2.93.86.09-.67.35-1.12.63-1.38-2.22-.26-4.55-1.14-4.55-5.06 0-1.12.39-2.03 1.03-2.75-.1-.26-.45-1.3.1-2.71 0 0 .84-.28 2.75 1.05A9.35 9.35 0 0 1 12 7.01c.85 0 1.7.12 2.5.34 1.9-1.33 2.74-1.05 2.74-1.05.55 1.41.2 2.45.1 2.71.64.72 1.03 1.63 1.03 2.75 0 3.93-2.34 4.75-4.57 5.01.36.32.68.95.68 1.91 0 1.38-.01 2.49-.01 2.83 0 .27.18.59.69.49A10.08 10.08 0 0 0 22 12.26C22 6.58 17.52 2 12 2Z"
            />
          </svg>
        </a>
        <a
          className="header-icon-link docker-link"
          href={dockerUrl}
          target="_blank"
          rel="noreferrer"
          aria-label={t('打开 Docker 镜像')}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path
              fill="currentColor"
              d="M4.25 11.25h2.5v-2.5h-2.5v2.5Zm3.25 0H10v-2.5H7.5v2.5Zm3.25 0h2.5v-2.5h-2.5v2.5Zm-3.25-3.25H10v-2.5H7.5V8Zm3.25 0h2.5v-2.5h-2.5V8Zm3.25 3.25h2.5v-2.5H14v2.5Zm6.1-.86c-.63-.43-1.58-.56-2.28-.34-.1-.75-.57-1.42-1.37-2.02l-.48-.36-.32.52c-.62 1.01-.77 2.33-.35 3.23-.66.37-1.78.35-5.82.35H3.11l-.08.45c-.26 1.58.03 2.92.86 3.98.94 1.2 2.48 1.8 4.58 1.8h.2c4.62 0 8.04-2.12 9.73-6.02.67.03 1.58-.1 2.24-.86l.35-.41-.89-.32Zm-11.43 6.5h-.19c-1.72 0-2.94-.44-3.64-1.31-.47-.59-.7-1.34-.68-2.28h9.25c2.67 0 3.8-.02 4.64-.44-1.56 2.62-4.3 4.03-8.1 4.03Z"
            />
          </svg>
        </a>
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
