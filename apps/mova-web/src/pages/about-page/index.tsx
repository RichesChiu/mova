import { useOutletContext } from 'react-router-dom'
import type { AppShellOutletContext } from '../../components/app-shell'
import { useI18n } from '../../i18n'
import { TMDB_ATTRIBUTION_NOTICE, TMDB_HOME_URL, TMDB_LOGO_PATH } from '../../lib/tmdb-attribution'
import { DashboardPageHeader } from '../home-page/dashboard-page-header'
import { HomeDashboardShell } from '../home-page/home-dashboard-shell'
import './about-page.scss'

const MOVA_REPOSITORY_URL = 'https://github.com/RichesChiu/mova'
const WECHAT_DONATION_QR_PATH = '/assets/support/wechat-donation-qr.png'

export const AboutPage = () => {
  const { currentUser } = useOutletContext<AppShellOutletContext>()
  const { l } = useI18n()

  return (
    <HomeDashboardShell ariaLabel={l('About & Credits')} currentUser={currentUser}>
      <div className="home-dashboard__content home-dashboard__content--about">
        <DashboardPageHeader className="about-page__header">
          <h2>{l('About & Credits')}</h2>
        </DashboardPageHeader>

        <div className="about-page">
          <article className="catalog-block about-page__panel">
            <section className="about-page__section" aria-labelledby="about-mova-title">
              <div className="about-page__brand">
                <img alt="" height="64" src="/mova-logo-web-64.png" width="64" />
                <h3 id="about-mova-title">MOVA</h3>
                <p>{l('Your self-hosted media library.')}</p>
              </div>
              <p className="about-page__copy">
                {l(
                  'MOVA is open-source software for organizing and watching media from your own server.',
                )}
              </p>
              <a
                className="about-page__link text-link"
                href={MOVA_REPOSITORY_URL}
                rel="noreferrer"
                target="_blank"
              >
                {l('View source code')}
              </a>
            </section>

            <div className="about-page__divider" aria-hidden="true" />

            <section className="about-page__section" aria-labelledby="about-tmdb-title">
              <div className="about-page__provider-heading">
                <h3 id="about-tmdb-title">{l('Data & artwork')}</h3>
                <a
                  aria-label={l('Visit TMDB')}
                  className="about-page__tmdb-logo-link"
                  href={TMDB_HOME_URL}
                  rel="noreferrer"
                  target="_blank"
                >
                  <img alt="" src={TMDB_LOGO_PATH} />
                </a>
              </div>
              <p className="about-page__notice">{l(TMDB_ATTRIBUTION_NOTICE)}</p>
              <p className="about-page__copy">
                {l(
                  'MOVA uses TMDB for media metadata and artwork. TMDB does not endorse, certify, or sponsor MOVA.',
                )}
              </p>
              <a
                className="about-page__link text-link"
                href={TMDB_HOME_URL}
                rel="noreferrer"
                target="_blank"
              >
                {l('Visit TMDB')}
              </a>
            </section>

            <div className="about-page__divider" aria-hidden="true" />

            <section className="about-page__section" aria-labelledby="about-support-title">
              <h3 id="about-support-title">{l('Support the author')}</h3>
              <p className="about-page__copy">
                {l('If MOVA is useful to you, you can support its continued development.')}
              </p>
              <figure className="about-page__donation">
                <a
                  aria-label={l('Open the WeChat donation QR code')}
                  className="about-page__donation-link"
                  href={WECHAT_DONATION_QR_PATH}
                  rel="noreferrer"
                  target="_blank"
                >
                  <span className="about-page__donation-qr">
                    <img alt={l('WeChat donation QR code')} src={WECHAT_DONATION_QR_PATH} />
                  </span>
                </a>
                <figcaption>{l('Scan with WeChat')}</figcaption>
              </figure>
            </section>
          </article>
        </div>
      </div>
    </HomeDashboardShell>
  )
}
