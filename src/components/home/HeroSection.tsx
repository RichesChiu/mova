import { MovaIcon } from '../MovaIcon'
import { heroBadges } from '../../data/homeContent'
import { useI18n } from '../../i18n-context'
import './HeroSection.css'

const heroBadgeIconSources = {
  'data-shield': '/assets/mova-icons/hero/privacy.svg',
  rocket: '/assets/mova-icons/hero/open-source.svg',
  'multi-terminal': '/assets/mova-icons/hero/cross-device.svg',
  scalable: '/assets/mova-icons/hero/evolving.svg',
} as const

export function HeroSection({
  onOpenDeployment,
  onOpenApiDocs,
}: {
  onOpenDeployment: () => void
  onOpenApiDocs: () => void
}) {
  const { t } = useI18n()

  return (
    <section className="hero-section" id="home" aria-labelledby="hero-title">
      <div className="hero-stage">
        <div className="hero-copy">
          <p className="eyebrow">MOVA</p>
          <h1 id="hero-title">{t('属于你自己的')}<br />{t('流媒体中心')}</h1>
          <p className="hero-kicker">{t('MOVA 是美观、好用的自托管流媒体服务器')}</p>
          <p className="hero-lede">
            {t('统一管理电影、剧集、音乐和照片，并通过网页与 macOS 随时访问。')}
          </p>

          <div className="hero-actions" aria-label={t('首屏操作')}>
            <a
              className="primary-action"
              href="/deploy"
              onClick={(event) => {
                event.preventDefault()
                onOpenDeployment()
              }}
            >
              {t('开始部署')}
              <MovaIcon name="arrow-right" className="button-icon" />
            </a>
            <button className="secondary-action" type="button" onClick={onOpenApiDocs}>
              {t('查看 API')}
              <MovaIcon name="arrow-right" className="button-icon" />
            </button>
          </div>
        </div>

        <div className="hero-preview">
          <img
            src="/screenshots/hero-dashboard-perspective-cut.png"
            width="1696"
            height="927"
            alt={t('MOVA 网页端媒体库首页界面')}
            decoding="async"
          />
        </div>
      </div>

      <div className="hero-badges" aria-label={t('MOVA 核心优势')}>
        {heroBadges.map((badge) => (
          <article key={badge.label}>
            <div className="hero-badge-icon" aria-hidden="true">
              <img
                className="hero-badge-svg"
                src={heroBadgeIconSources[badge.icon]}
                width="20"
                height="20"
                alt=""
              />
            </div>
            <div className="hero-badge-copy">
              <strong>{t(badge.label)}</strong>
              <span>{t(badge.text)}</span>
            </div>
          </article>
        ))}
      </div>
    </section>
  )
}
