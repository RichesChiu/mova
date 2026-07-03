import { DashboardPreview } from '../DashboardPreview'
import { MovaIcon } from '../MovaIcon'
import { heroBadges } from '../../data/homeContent'

export function HeroSection({
  onNavigate,
  onOpenApiDocs,
}: {
  onNavigate: (sectionId: string) => void
  onOpenApiDocs: () => void
}) {
  return (
    <section className="hero-section" id="home" aria-labelledby="hero-title">
      <div className="hero-bg hero-bg-left" aria-hidden="true" />
      <div className="hero-bg hero-bg-right" aria-hidden="true" />

      <div className="hero-copy">
        <p className="eyebrow">open source media center</p>
        <h1 id="hero-title">
          <span className="hero-title-brand">MOVA</span>
          <span className="hero-title-main">自托管流媒体服务器</span>
        </h1>
        <p className="hero-lede">
          打造属于你自己的私人媒体中心，集中管理电影、剧集、音乐和照片，随时随地畅享高品质流媒体体验。
        </p>

        <div className="hero-actions" aria-label="首屏操作">
          <button className="primary-action" type="button" onClick={() => onNavigate('deploy')}>
            开始部署
            <MovaIcon name="arrow-right" className="button-icon" />
          </button>
          <button className="secondary-action" type="button" onClick={onOpenApiDocs}>
            API 文档
            <MovaIcon name="docs" className="button-icon" />
          </button>
        </div>

        <div className="hero-badges" aria-label="MOVA 特性">
          {heroBadges.map((badge) => (
            <span key={badge.label}>
              <MovaIcon name={badge.icon} />
              {badge.label}
            </span>
          ))}
        </div>
      </div>

      <DashboardPreview />
    </section>
  )
}
