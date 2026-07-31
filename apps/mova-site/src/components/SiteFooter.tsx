import { useI18n } from '../i18n-context'
import './SiteFooter.css'

export function SiteFooter({
  onOpenCredits,
  onOpenHome,
  onOpenPrivacy,
}: {
  onOpenCredits: () => void
  onOpenHome: () => void
  onOpenPrivacy: () => void
}) {
  const { language } = useI18n()
  const isChinese = language === 'zh'

  return (
    <footer className="site-footer">
      <div className="site-footer-inner">
        <p>{isChinese ? '© 2026 MOVA，自托管媒体服务。' : '© 2026 MOVA. Self-hosted media service.'}</p>

        <nav className="site-footer-nav" aria-label={isChinese ? '页脚导航' : 'Footer navigation'}>
          <button type="button" onClick={onOpenHome}>
            {isChinese ? '首页' : 'Home'}
          </button>
          <button type="button" onClick={onOpenPrivacy}>
            {isChinese ? '隐私政策' : 'Privacy'}
          </button>
          <button type="button" onClick={onOpenCredits}>
            {isChinese ? '鸣谢与数据来源' : 'Credits & Data Sources'}
          </button>
        </nav>
      </div>
    </footer>
  )
}
