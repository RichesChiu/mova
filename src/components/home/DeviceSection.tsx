import { MovaIcon } from '../MovaIcon'
import { SectionTitle } from '../SectionTitle'
import { devices } from '../../data/homeContent'
import { useI18n } from '../../i18n-context'
import './DeviceSection.css'

export function DeviceSection() {
  const macDevice = devices.find((device) => device.id === 'macos')
  const { t } = useI18n()

  return (
    <section className="platform-section" aria-labelledby="devices-title">
      <div className="macos-showcase">
        <div className="macos-copy">
          <p className="eyebrow">{t('macOS 客户端')}</p>
          <h2>{t('专为 macOS 打造的')}<br />{t('原生体验')}</h2>
          <p>{macDevice ? t(macDevice.text) : null}</p>
          <div className="macos-actions" aria-label={t('macOS 平台说明')}>
            <span className="macos-coming-note">{t('即将到来')}</span>
            <span className="macos-store-status">{t('尚未上架 Mac App Store')}</span>
          </div>
        </div>
        <div className="macos-preview">
          <img
            src="/screenshots/macos-detail-perspective-matched.png"
            width="1672"
            height="941"
            loading="lazy"
            decoding="async"
            alt={t('MOVA macOS 原生客户端详情界面')}
          />
        </div>
      </div>

      <div className="platform-heading">
        <SectionTitle id="devices-title" title={t('跨平台支持')} />
        <p>{t('在你常用的设备上，随时访问你的媒体库')}</p>
      </div>

      <div className="device-grid" aria-label={t('MOVA 平台状态')}>
        {devices.map((device) => (
          <article
            className={`device-card${device.available ? '' : ' device-card--upcoming'}`}
            key={device.title}
          >
            <span className={`device-status ${device.available ? 'is-ready' : 'is-upcoming'}`}>
              {t(device.status)}
            </span>
            <span className={`device-icon device-icon--${device.id}`} aria-hidden="true">
              <img src={`/assets/mova-icons/platform/${device.id}.svg`} alt="" />
            </span>
            <h3>{t(device.title)}</h3>
            <p>{t(device.text)}</p>
            {device.action ? (
              <a
                className={`device-action device-action--${device.action.variant}`}
                href={device.action.href}
              >
                {t(device.action.label)}
                <MovaIcon name="arrow-right" />
              </a>
            ) : null}
          </article>
        ))}
      </div>

    </section>
  )
}
