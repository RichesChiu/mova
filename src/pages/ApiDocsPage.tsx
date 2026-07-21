import { MovaIcon } from '../components/MovaIcon'
import { dockerUrl } from '../data/homeContent'
import { useI18n } from '../i18n-context'
import './ApiDocsPage.css'
import {
  apiCommonNotes,
  apiEndpointGroups,
  apiErrorExample,
  apiIdRelations,
  apiOverviewCards,
  apiPlaybackFlow,
  apiSourceLinks,
  apiStatusCodes,
  apiSuccessExample,
  type ApiEndpoint,
  type HttpMethod,
} from '../data/apiDocs'

export function ApiDocsPage({ onNavigate }: { onNavigate: (sectionId: string) => void }) {
  const { language, t } = useI18n()
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
          <h1 id="api-title">{t('MOVA API 文档')}</h1>
          <p className="api-hero-lede">
            {t('根据服务端文档整理当前 mova-server 已实现的 HTTP 接口，覆盖鉴权、媒体库扫描、媒体条目、播放进度、媒体流和播放器接入需要的 ID 流转。')}
          </p>
          <div className="api-hero-actions">
            <a className="primary-action" href={dockerUrl} target="_blank" rel="noreferrer">
              {t('查看部署方式')}
              <MovaIcon name="arrow-right" className="button-icon" />
            </a>
            <button className="secondary-action" type="button" onClick={() => onNavigate('home')}>
              {t('返回首页')}
              <MovaIcon name="home" className="button-icon" />
            </button>
          </div>
        </div>

        <div className="api-hero-panel" aria-label={t('API 摘要')}>
          <div>
            <strong>{endpointTotal}</strong>
            <span>{t('已整理接口')}</span>
          </div>
          <div>
            <strong>{apiEndpointGroups.length}</strong>
            <span>{t('接口分组')}</span>
          </div>
          <div>
            <strong>{methodCounts.GET}</strong>
            <span>{t('GET 接口')}</span>
          </div>
          <div>
            <strong>2</strong>
            <span>{t('登录方式')}</span>
          </div>
        </div>
      </section>

      <aside className="api-source-notice" aria-labelledby="api-source-title">
        <div>
          <p className="eyebrow">Source of Truth</p>
          <h2 id="api-source-title">{t('完整细节请以项目文档为准')}</h2>
          <p>{language === 'zh' ? (
            <>
              本页提供便于快速查阅的接口摘要。SSE 的 revision 同步、断线恢复、
              <code>resync.required</code>、<code>session.invalidated</code>、扫描进度事件，
              以及扫描和元数据规则不在此完整展开，请前往项目仓库查看最新文档。
            </>
          ) : (
            <>
              This page is a quick endpoint reference. For SSE revision synchronization, reconnect behavior,
              <code>resync.required</code>, <code>session.invalidated</code>, scan progress events, scanning rules,
              and metadata details, see the latest project documentation.
            </>
          )}</p>
        </div>
        <div className="api-source-links">
          <a href={apiSourceLinks.api} target="_blank" rel="noreferrer">
            {t('完整 API.md')}
            <MovaIcon name="arrow-right" />
          </a>
          <a href={apiSourceLinks.sse} target="_blank" rel="noreferrer">
            {t('完整 SSE.md')}
            <MovaIcon name="arrow-right" />
          </a>
          <a href={apiSourceLinks.repository} target="_blank" rel="noreferrer">
            {t('MOVA 项目仓库')}
            <MovaIcon name="arrow-right" />
          </a>
        </div>
      </aside>

      <section className="api-layout" aria-label={t('API 文档内容')}>
        <aside className="api-sidebar">
          <strong>{t('文档目录')}</strong>
          <button type="button" onClick={() => scrollToApiSection('api-overview')}>
            {t('通用说明')}
          </button>
          {apiEndpointGroups.map((group) => (
            <button type="button" key={group.id} onClick={() => scrollToApiSection(`api-${group.id}`)}>
              {t(group.title)}
            </button>
          ))}
          <button type="button" onClick={() => scrollToApiSection('api-id-relations')}>
            {t('ID 关系')}
          </button>
        </aside>

        <div className="api-content">
          <section className="api-doc-card" id="api-overview">
            <div className="api-section-heading">
              <p className="eyebrow">General</p>
              <h2>{t('通用说明')}</h2>
            </div>

            <div className="api-overview-grid">
              {apiOverviewCards.map((card) => (
                <article key={card.label}>
                  <span>{t(card.label)}</span>
                  <strong>{card.value}</strong>
                  <p>{t(card.text)}</p>
                </article>
              ))}
            </div>

            <div className="api-note-grid">
              <div>
                <h3>{t('关键规则')}</h3>
                <ul>
                  {apiCommonNotes.map((note) => (
                    <li key={note}>{t(note)}</li>
                  ))}
                </ul>
              </div>
              <div>
                <h3>{t('常见状态码')}</h3>
                <div className="status-code-grid">
                  {apiStatusCodes.map(([code, text]) => (
                    <span key={code}>
                      <strong>{code}</strong>
                      {t(text)}
                    </span>
                  ))}
                </div>
              </div>
            </div>

            <div className="api-code-grid">
              <div>
                <h3>{t('成功响应')}</h3>
                <pre>
                  <code>{apiSuccessExample}</code>
                </pre>
              </div>
              <div>
                <h3>{t('错误响应')}</h3>
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
                <h2>{t(group.title)}</h2>
                <p>{t(group.summary)}</p>
              </div>

              <div className="api-highlight-list">
                {group.highlights.map((highlight) => (
                  <span key={highlight}>{t(highlight)}</span>
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
              <h2>{t('ID 关系与播放流转')}</h2>
              <p>
                {t('前端接入播放器时最容易混淆的是媒体库、媒体条目、媒体文件、音轨和字幕的 ID。下面按使用顺序整理一遍。')}
              </p>
            </div>

            <div className="id-relation-grid">
              {apiIdRelations.map(([id, text]) => (
                <article key={id}>
                  <strong>{id}</strong>
                  <p>{t(text)}</p>
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
  const { t } = useI18n()

  return (
    <article className="endpoint-row">
      <span className={`method-badge method-${endpoint.method.toLowerCase()}`}>{endpoint.method}</span>
      <code>{endpoint.path}</code>
      <p>{t(endpoint.description)}</p>
    </article>
  )
}
