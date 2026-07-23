import { useI18n } from '../i18n-context'
import './SiteFooter.css'

export function SiteFooter({
  onOpenHome,
  onOpenPrivacy,
  onOpenSupport,
}: {
  onOpenHome: () => void
  onOpenPrivacy: () => void
  onOpenSupport: () => void
}) {
  const { language } = useI18n()
  const isChinese = language === 'zh'

  return (
    <footer className="site-footer">
      <div className="site-footer-inner">
        <p>{isChinese ? '© 2026 MOVA，自托管媒体服务。' : '© 2026 MOVA. Self-hosted media service.'}</p>

        <nav className="site-footer-nav" aria-label={isChinese ? '法律与支持' : 'Legal and support'}>
          <button type="button" onClick={onOpenHome}>
            {isChinese ? '首页' : 'Home'}
          </button>
          <button type="button" onClick={onOpenPrivacy}>
            {isChinese ? '隐私政策' : 'Privacy'}
          </button>
          <button type="button" onClick={onOpenSupport}>
            {isChinese ? '支持' : 'Support'}
          </button>
        </nav>
      </div>
    </footer>
  )
}
