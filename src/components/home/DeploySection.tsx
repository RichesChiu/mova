import { MovaIcon } from '../MovaIcon'
import { deploySteps } from '../../data/homeContent'

export function DeploySection() {
  return (
    <section className="section-block deploy-section" id="deploy" aria-labelledby="deploy-title">
      <div>
        <p className="eyebrow">Docker deploy</p>
        <h2 id="deploy-title">一条命令，把私人媒体中心跑起来</h2>
        <p>
          Mova 默认使用已发布镜像，不需要在部署机器上从源码构建。配置媒体目录后启动容器，
          再进入 Web 页面创建管理员和媒体库。
        </p>
      </div>

      <div className="deploy-card">
        <pre>
          <code>{`cp .env.example .env
MOVA_MEDIA_ROOT=/absolute/path/to/media
docker compose up -d`}</code>
        </pre>
        <ol>
          {deploySteps.map((step) => (
            <li key={step.text}>
              <MovaIcon name={step.icon} />
              {step.text}
            </li>
          ))}
        </ol>
      </div>
    </section>
  )
}
