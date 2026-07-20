import { MovaIcon } from '../MovaIcon'
import { SectionTitle } from '../SectionTitle'
import { devices, stats } from '../../data/homeContent'

export function DeviceSection() {
  return (
    <section className="section-block" aria-labelledby="devices-title">
      <SectionTitle id="devices-title" title="在所有设备上，随时随地享受" />

      <div className="device-grid">
        {devices.map((device) => (
          <article
            className={`device-card ${device.images.length ? 'device-card--showcase' : 'device-card--upcoming'}`}
            key={device.title}
          >
            {device.images.length ? (
              <div
                className={`device-gallery ${device.images.length === 2 ? 'device-gallery--two' : ''}`}
              >
                {device.images.map((image, index) => (
                  <div className="device-frame" key={image}>
                    <img
                      src={image}
                      alt={`${device.title} MOVA 界面预览 ${index + 1}`}
                      loading="lazy"
                      decoding="async"
                    />
                  </div>
                ))}
              </div>
            ) : (
              <div className="device-coming-soon" aria-hidden="true">
                <span>开发中</span>
              </div>
            )}
            <span className="device-icon">
              <MovaIcon name={device.icon} />
            </span>
            <h3>{device.title}</h3>
            <p>{device.text}</p>
            {device.notices && (
              <div className="device-notices" aria-label="平台支持说明">
                {device.notices.map((notice) => (
                  <span key={notice}>{notice}</span>
                ))}
              </div>
            )}
          </article>
        ))}
      </div>

      <div className="stats-strip">
        {stats.map((stat) => (
          <article className="stat-item" key={stat.title}>
            <MovaIcon name={stat.icon} className="stat-icon" />
            <div>
              <strong>{stat.value}</strong>
              <span>{stat.title}</span>
            </div>
            <p>{stat.text}</p>
          </article>
        ))}
      </div>
    </section>
  )
}
