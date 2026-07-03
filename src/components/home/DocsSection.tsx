import { MovaIcon } from '../MovaIcon'
import { SectionTitle } from '../SectionTitle'
import { docs } from '../../data/homeContent'

export function DocsSection() {
  return (
    <section className="section-block docs-section" id="docs" aria-labelledby="docs-title">
      <SectionTitle id="docs-title" title="文档入口清晰，部署维护更轻松" />
      <div className="docs-grid">
        {docs.map((doc) => (
          <article key={doc.title}>
            <span className="docs-icon">
              <MovaIcon name={doc.icon} />
            </span>
            <h3>{doc.title}</h3>
            <p>{doc.text}</p>
          </article>
        ))}
      </div>
    </section>
  )
}
