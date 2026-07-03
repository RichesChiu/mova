import { MovaIcon, type IconName } from './MovaIcon'
import {
  dashboardAdminMenu,
  dashboardMetrics,
  dashboardPrimaryMenu,
  mediaCards,
} from '../data/homeContent'

export function DashboardPreview() {
  return (
    <div className="dashboard-shell" aria-label="MOVA 控制台界面预览">
      <div className="dashboard-topbar">
        <div className="dashboard-logo">
          <img src="/mova-logo-transparent-128.png" width="26" height="26" alt="" />
          <strong>MOVA</strong>
        </div>
        <div className="search-bar">
          <MovaIcon name="search" />
          <span>搜索影片、剧集、演员...</span>
        </div>
        <div className="topbar-actions">
          <span>
            <MovaIcon name="bell" />
          </span>
          <span>
            <MovaIcon name="settings" />
          </span>
          <img src="/mova-logo-pwa-192.png" width="28" height="28" alt="" />
        </div>
      </div>

      <div className="dashboard-body">
        <aside className="dashboard-sidebar" aria-label="控制台菜单">
          {dashboardPrimaryMenu.map((item, index) => (
            <span className={index === 0 ? 'active' : ''} key={item.label}>
              <MovaIcon name={item.icon} />
              {item.label}
            </span>
          ))}
          <div className="sidebar-divider" />
          {dashboardAdminMenu.map((item) => (
            <span key={item.label}>
              <MovaIcon name={item.icon} />
              {item.label}
            </span>
          ))}
          <div className="admin-chip">admin 管理员</div>
        </aside>

        <div className="dashboard-content">
          <section className="preview-panel media-panel" aria-label="继续观看">
            <h2>继续观看</h2>
            <div className="poster-grid">
              {mediaCards.map((card) => (
                <article className={`poster-card ${card.tone}`} key={card.title}>
                  <strong>{card.title}</strong>
                  <span>{card.meta}</span>
                </article>
              ))}
            </div>
          </section>

          <section className="preview-panel summary-panel" aria-label="媒体库概览">
            <h2>媒体库概览</h2>
            <div className="summary-list">
              {dashboardMetrics.map((metric) => (
                <Metric
                  icon={metric.icon}
                  label={metric.label}
                  value={metric.value}
                  key={metric.label}
                />
              ))}
            </div>
          </section>

          <section className="preview-panel status-panel" aria-label="服务器状态">
            <h2>服务器状态</h2>
            <div className="status-rings">
              <Ring label="CPU" value="18%" />
              <Ring label="内存" value="42%" />
              <Ring label="转码任务" value="2" />
            </div>
            <div className="storage-bar">
              <span>存储空间</span>
              <strong>8.2 TB / 16 TB</strong>
              <i />
            </div>
          </section>
        </div>
      </div>
    </div>
  )
}

function Metric({ icon, label, value }: { icon: IconName; label: string; value: string }) {
  return (
    <div className="metric-row">
      <span>
        <MovaIcon name={icon} />
        {label}
      </span>
      <strong>{value}</strong>
    </div>
  )
}

function Ring({ label, value }: { label: string; value: string }) {
  return (
    <div className="ring">
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  )
}
