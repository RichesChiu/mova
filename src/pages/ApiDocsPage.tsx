import { MovaIcon } from '../components/MovaIcon'
import {
  apiCommonNotes,
  apiEndpointGroups,
  apiErrorExample,
  apiIdRelations,
  apiOverviewCards,
  apiPlaybackFlow,
  apiStatusCodes,
  apiSuccessExample,
  type ApiEndpoint,
  type HttpMethod,
} from '../data/apiDocs'

export function ApiDocsPage({ onNavigate }: { onNavigate: (sectionId: string) => void }) {
  const endpointTotal = apiEndpointGroups.reduce((total, group) => total + group.endpoints.length, 0)
  const methodCounts = apiEndpointGroups
    .flatMap((group) => group.endpoints)
    .reduce<Record<HttpMethod, number>>(
      (counts, endpoint) => ({
        ...counts,
        [endpoint.method]: counts[endpoint.method] + 1,
      }),
      { GET: 0, POST: 0, PATCH: 0, PUT: 0, DELETE: 0, HEAD: 0 },
    )

  const scrollToApiSection = (sectionId: string) => {
    document.getElementById(sectionId)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  return (
    <div className="api-page">
      <section className="api-hero" aria-labelledby="api-title">
        <div>
          <p className="eyebrow">API Reference</p>
          <h1 id="api-title">MOVA API 文档</h1>
          <p className="api-hero-lede">
            根据服务端文档整理当前 mova-server 已实现的 HTTP 接口，覆盖鉴权、媒体库扫描、
            媒体条目、播放进度、媒体流和播放器接入需要的 ID 流转。
          </p>
          <div className="api-hero-actions">
            <button className="primary-action" type="button" onClick={() => onNavigate('deploy')}>
              查看部署方式
              <MovaIcon name="arrow-right" className="button-icon" />
            </button>
            <button className="secondary-action" type="button" onClick={() => onNavigate('home')}>
              返回首页
              <MovaIcon name="home" className="button-icon" />
            </button>
          </div>
        </div>

        <div className="api-hero-panel" aria-label="API 摘要">
          <div>
            <strong>{endpointTotal}</strong>
            <span>已整理接口</span>
          </div>
          <div>
            <strong>{apiEndpointGroups.length}</strong>
            <span>接口分组</span>
          </div>
          <div>
            <strong>{methodCounts.GET}</strong>
            <span>GET 接口</span>
          </div>
          <div>
            <strong>2</strong>
            <span>登录方式</span>
          </div>
        </div>
      </section>

      <section className="api-layout" aria-label="API 文档内容">
        <aside className="api-sidebar">
          <strong>文档目录</strong>
          <button type="button" onClick={() => scrollToApiSection('api-overview')}>
            通用说明
          </button>
          {apiEndpointGroups.map((group) => (
            <button type="button" key={group.id} onClick={() => scrollToApiSection(`api-${group.id}`)}>
              {group.title}
            </button>
          ))}
          <button type="button" onClick={() => scrollToApiSection('api-id-relations')}>
            ID 关系
          </button>
        </aside>

        <div className="api-content">
          <section className="api-doc-card" id="api-overview">
            <div className="api-section-heading">
              <p className="eyebrow">General</p>
              <h2>通用说明</h2>
            </div>

            <div className="api-overview-grid">
              {apiOverviewCards.map((card) => (
                <article key={card.label}>
                  <span>{card.label}</span>
                  <strong>{card.value}</strong>
                  <p>{card.text}</p>
                </article>
              ))}
            </div>

            <div className="api-note-grid">
              <div>
                <h3>关键规则</h3>
                <ul>
                  {apiCommonNotes.map((note) => (
                    <li key={note}>{note}</li>
                  ))}
                </ul>
              </div>
              <div>
                <h3>常见状态码</h3>
                <div className="status-code-grid">
                  {apiStatusCodes.map(([code, text]) => (
                    <span key={code}>
                      <strong>{code}</strong>
                      {text}
                    </span>
                  ))}
                </div>
              </div>
            </div>

            <div className="api-code-grid">
              <div>
                <h3>成功响应</h3>
                <pre>
                  <code>{apiSuccessExample}</code>
                </pre>
              </div>
              <div>
                <h3>错误响应</h3>
                <pre>
                  <code>{apiErrorExample}</code>
                </pre>
              </div>
            </div>
          </section>

          {apiEndpointGroups.map((group) => (
            <section className="api-doc-card" id={`api-${group.id}`} key={group.id}>
              <div className="api-section-heading">
                <p className="eyebrow">Endpoint Group</p>
                <h2>{group.title}</h2>
                <p>{group.summary}</p>
              </div>

              <div className="api-highlight-list">
                {group.highlights.map((highlight) => (
                  <span key={highlight}>{highlight}</span>
                ))}
              </div>

              <div className="endpoint-list">
                {group.endpoints.map((endpoint) => (
                  <EndpointRow endpoint={endpoint} key={`${endpoint.method}-${endpoint.path}`} />
                ))}
              </div>
            </section>
          ))}

          <section className="api-doc-card" id="api-id-relations">
            <div className="api-section-heading">
              <p className="eyebrow">Player Flow</p>
              <h2>ID 关系与播放流转</h2>
              <p>
                前端接入播放器时最容易混淆的是媒体库、媒体条目、媒体文件、音轨和字幕的 ID。
                下面按使用顺序整理一遍。
              </p>
            </div>

            <div className="id-relation-grid">
              {apiIdRelations.map(([id, text]) => (
                <article key={id}>
                  <strong>{id}</strong>
                  <p>{text}</p>
                </article>
              ))}
            </div>

            <ol className="api-flow-list">
              {apiPlaybackFlow.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
          </section>
        </div>
      </section>
    </div>
  )
}

function EndpointRow({ endpoint }: { endpoint: ApiEndpoint }) {
  return (
    <article className="endpoint-row">
      <span className={`method-badge method-${endpoint.method.toLowerCase()}`}>{endpoint.method}</span>
      <code>{endpoint.path}</code>
      <p>{endpoint.description}</p>
    </article>
  )
}
