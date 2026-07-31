import { useI18n } from '../i18n-context'
import './LegalPage.css'

const TMDB_HOME_URL = 'https://www.themoviedb.org'
const TMDB_LOGO_PATH = '/assets/tmdb/tmdb-logo-blue-short.svg'
const TMDB_ATTRIBUTION_NOTICE =
  'This product uses the TMDB API but is not endorsed or certified by TMDB.'

export function CreditsPage() {
  const { language } = useI18n()
  const isChinese = language === 'zh'

  return (
    <article className="legal-page credits-page">
      <div className="legal-page-inner">
        <header className="legal-page-header">
          <p className="legal-page-kicker">MOVA · CREDITS</p>
          <h1>{isChinese ? '鸣谢与数据来源' : 'Credits & Data Sources'}</h1>
          <p className="legal-page-summary">
            {isChinese
              ? 'MOVA 是独立开发的开源自托管媒体服务。这里列出为媒体资料提供支持的第三方来源。'
              : 'MOVA is an independently developed, open-source self-hosted media service. This page identifies third-party sources that support its media information.'}
          </p>
        </header>

        <div className="legal-sections">
          <section className="legal-section credits-provider" aria-labelledby="credits-tmdb-title">
            <div className="credits-provider-heading">
              <h2 id="credits-tmdb-title">{isChinese ? '数据与图片' : 'Data & artwork'}</h2>
              <a
                aria-label={isChinese ? '访问 TMDB' : 'Visit TMDB'}
                className="credits-tmdb-logo-link"
                href={TMDB_HOME_URL}
                rel="noreferrer"
                target="_blank"
              >
                <img alt="" src={TMDB_LOGO_PATH} />
              </a>
            </div>
            <p className="credits-required-notice">{TMDB_ATTRIBUTION_NOTICE}</p>
            <p>
              {isChinese
                ? 'MOVA 使用 TMDB 提供媒体元数据和图片。TMDB 不认可、认证或赞助 MOVA。'
                : 'MOVA uses TMDB for media metadata and artwork. TMDB does not endorse, certify, or sponsor MOVA.'}
            </p>
            <p>
              <a href={TMDB_HOME_URL} rel="noreferrer" target="_blank">
                {isChinese ? '访问 The Movie Database' : 'Visit The Movie Database'}
              </a>
            </p>
          </section>
        </div>
      </div>
    </article>
  )
}
