import { MovaIcon } from '../MovaIcon'
import { SectionTitle } from '../SectionTitle'
import { devices, stats } from '../../data/homeContent'

export function DeviceSection() {
  return (
    <section className="section-block" aria-labelledby="devices-title">
      <SectionTitle id="devices-title" title="在所有设备上，随时随地享受" />

      <div className="device-grid">
        {devices.map((device) => (
          <article className="device-card" key={device.title}>
            <div className="device-frame">
              <img src={device.image} alt={`${device.title} 上的 MOVA 界面`} />
            </div>
            <span className="device-icon">
              <MovaIcon name={device.icon} />
            </span>
            <h3>{device.title}</h3>
            <p>{device.text}</p>
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
