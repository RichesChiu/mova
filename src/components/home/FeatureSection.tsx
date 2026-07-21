import { MovaIcon } from '../MovaIcon'
import { SectionTitle } from '../SectionTitle'
import { features } from '../../data/homeContent'
import { useI18n } from '../../i18n-context'
import './FeatureSection.css'

export function FeatureSection() {
  const { t } = useI18n()

  return (
    <section className="section-block" id="features" aria-labelledby="features-title">
      <div className="feature-intro">
        <p className="eyebrow">Core capabilities</p>
        <SectionTitle
          id="features-title"
          title={<>{t('强大功能，')}<br />{t('全面掌控你的媒体')}</>}
        />
      </div>

      <div className="feature-grid" aria-label={t('MOVA 核心功能')}>
        {features.slice(0, 5).map((feature) => (
          <article className="feature-card" key={feature.title}>
            <span className="feature-icon">
              <MovaIcon name={feature.icon} />
            </span>
            <h3>{t(feature.title)}</h3>
            <p>{t(feature.text)}</p>
          </article>
        ))}
      </div>
    </section>
  )
}
