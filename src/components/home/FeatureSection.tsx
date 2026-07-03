import { MovaIcon } from '../MovaIcon'
import { SectionTitle } from '../SectionTitle'
import { features } from '../../data/homeContent'

export function FeatureSection() {
  return (
    <section className="section-block" id="features" aria-labelledby="features-title">
      <SectionTitle id="features-title" title="强大功能，全面掌控你的媒体" />

      <div className="feature-grid">
        {features.map((feature) => (
          <article className="feature-card" key={feature.title}>
            <span className="feature-icon">
              <MovaIcon name={feature.icon} />
            </span>
            <h3>{feature.title}</h3>
            <p>{feature.text}</p>
          </article>
        ))}
      </div>
    </section>
  )
}
