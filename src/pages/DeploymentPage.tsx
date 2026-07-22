import { MovaIcon } from '../components/MovaIcon'
import { dockerUrl, githubUrl } from '../data/homeContent'
import { useI18n } from '../i18n-context'
import './DeploymentPage.css'

const deploymentSections = [
  ['deploy-requirements', '环境要求', 'Requirements'],
  ['deploy-compose', 'Docker Compose 示例', 'Docker Compose example'],
  ['deploy-quick-start', '启动服务', 'Launch'],
  ['deploy-operations', '运行与升级', 'Operations'],
  ['deploy-first-use', '首次使用', 'First use'],
] as const

const launchCommandZh = `# 保存并按照注释修改 docker-compose.yml 后启动
docker compose up -d

# 确认两个服务均为运行或健康状态
docker compose ps`

const launchCommandEn = `# Save and update docker-compose.yml as annotated, then launch
docker compose up -d

# Confirm both services are running or healthy
docker compose ps`

const composeExampleZh = `services:
  app:
    image: richeschiu/mova:latest
    container_name: mova-app
    depends_on:
      database:
        condition: service_healthy
    ports:
      - "36080:36080"
    environment:
      # 数据库密码必须与下方 POSTGRES_PASSWORD 保持一致
      MOVA_DATABASE_URL: postgres://mova:change_this_password@database:5432/mova
      # 发布镜像内置的网页端目录，请保持默认值
      MOVA_WEB_DIST_DIR: /app/web
      # 可选：填写 TMDB API Read Access Token；留空仍可扫描和播放本地媒体
      MOVA_TMDB_ACCESS_TOKEN: ""
      # 后台扫描 Worker 并发数，低配置设备建议保持 2
      MOVA_WORKER_CONCURRENCY: "2"
    volumes:
      # 海报、背景图等运行时缓存
      - ./data/cache:/app/data/cache
      - type: bind
        # 必填：替换为宿主机媒体目录的绝对路径
        source: /absolute/path/to/media
        target: /media
        # MOVA 不会修改原始媒体文件
        read_only: true
    restart: unless-stopped

  database:
    image: postgres:18
    environment:
      POSTGRES_USER: mova
      # 必填：修改为强密码，并同步修改上方 MOVA_DATABASE_URL
      POSTGRES_PASSWORD: change_this_password
      POSTGRES_DB: mova
      PGDATA: /var/lib/postgresql/18/docker
    volumes:
      # PostgreSQL 数据持久化目录，请定期备份
      - ./data/postgres:/var/lib/postgresql
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U mova -d mova"]
      interval: 5s
      timeout: 5s
      retries: 12
    shm_size: 256mb
    restart: unless-stopped`

const composeExampleEn = `services:
  app:
    image: richeschiu/mova:latest
    container_name: mova-app
    depends_on:
      database:
        condition: service_healthy
    ports:
      - "36080:36080"
    environment:
      # Must use the same password as POSTGRES_PASSWORD below
      MOVA_DATABASE_URL: postgres://mova:change_this_password@database:5432/mova
      # Web directory bundled in the published image; keep this value unchanged
      MOVA_WEB_DIST_DIR: /app/web
      # Optional TMDB API Read Access Token; leave empty for local scanning and playback
      MOVA_TMDB_ACCESS_TOKEN: ""
      # Background scan worker concurrency; keep 2 on lower-powered devices
      MOVA_WORKER_CONCURRENCY: "2"
    volumes:
      # Runtime cache for posters, backdrops, and related assets
      - ./data/cache:/app/data/cache
      - type: bind
        # Required: replace with the absolute path to your host media directory
        source: /absolute/path/to/media
        target: /media
        # MOVA never modifies the original media files
        read_only: true
    restart: unless-stopped

  database:
    image: postgres:18
    environment:
      POSTGRES_USER: mova
      # Required: use a strong password and update MOVA_DATABASE_URL above to match
      POSTGRES_PASSWORD: change_this_password
      POSTGRES_DB: mova
      PGDATA: /var/lib/postgresql/18/docker
    volumes:
      # Persistent PostgreSQL data; back up this directory regularly
      - ./data/postgres:/var/lib/postgresql
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U mova -d mova"]
      interval: 5s
      timeout: 5s
      retries: 12
    shm_size: 256mb
    restart: unless-stopped`

const operationsCommand = `# 查看状态
docker compose ps

# 查看服务日志
docker compose logs -f app

# 拉取并启动最新发布镜像
docker compose pull
docker compose up -d`

export function DeploymentPage({ onNavigate }: { onNavigate: (sectionId: string) => void }) {
  const { language } = useI18n()
  const isChinese = language === 'zh'

  const scrollToSection = (sectionId: string) => {
    document.getElementById(sectionId)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  return (
    <div className="deploy-page">
      <section className="deploy-hero" aria-labelledby="deploy-title">
        <div className="deploy-hero-copy">
          <p className="eyebrow">Docker Deployment</p>
          <h1 id="deploy-title">{isChinese ? '部署你的 MOVA' : 'Deploy your MOVA'}</h1>
          <p>
            {isChinese
              ? '使用 Docker Compose 在服务器、NAS 或个人电脑上运行 MOVA。媒体目录以只读方式挂载，数据库与缓存保留在你的设备中。'
              : 'Run MOVA on a server, NAS, or personal computer with Docker Compose. Media is mounted read-only, while the database and cache stay on your device.'}
          </p>
          <div className="deploy-actions">
            <a href={dockerUrl} target="_blank" rel="noreferrer">
              {isChinese ? '查看 Docker 镜像' : 'View Docker image'}
              <MovaIcon name="arrow-right" />
            </a>
            <button type="button" onClick={() => onNavigate('api')}>
              {isChinese ? '查看 API 文档' : 'View API docs'}
              <MovaIcon name="arrow-right" />
            </button>
          </div>
        </div>

        <aside className="deploy-terminal" aria-label={isChinese ? '部署命令预览' : 'Deployment command preview'}>
          <div className="deploy-terminal-bar" aria-hidden="true">
            <span />
            <span />
            <span />
            <strong>mova / docker-compose</strong>
          </div>
          <pre><code>{isChinese ? launchCommandZh : launchCommandEn}</code></pre>
          <div className="deploy-terminal-status">
            <i aria-hidden="true" />
            {isChinese ? '服务默认运行于 127.0.0.1:36080' : 'Service runs on 127.0.0.1:36080 by default'}
          </div>
        </aside>
      </section>

      <section className="deploy-summary" aria-label={isChinese ? '部署摘要' : 'Deployment summary'}>
        <article>
          <span>01</span>
          <div>
            <strong>{isChinese ? '准备环境' : 'Prepare'}</strong>
            <p>{isChinese ? 'Docker、Compose 与媒体目录' : 'Docker, Compose, and a media directory'}</p>
          </div>
        </article>
        <article>
          <span>02</span>
          <div>
            <strong>{isChinese ? '填写配置' : 'Configure'}</strong>
            <p>{isChinese ? '直接修改 Compose 中的示例值' : 'Update the example values in Compose'}</p>
          </div>
        </article>
        <article>
          <span>03</span>
          <div>
            <strong>{isChinese ? '启动服务' : 'Launch'}</strong>
            <p><code>docker compose up -d</code></p>
          </div>
        </article>
      </section>

      <section className="deploy-layout" aria-label={isChinese ? '部署文档内容' : 'Deployment documentation'}>
        <aside className="deploy-sidebar">
          <strong>{isChinese ? '部署目录' : 'Contents'}</strong>
          {deploymentSections.map(([id, zh, en]) => (
            <button type="button" key={id} onClick={() => scrollToSection(id)}>
              {isChinese ? zh : en}
            </button>
          ))}
          <a href={`${githubUrl}#部署`} target="_blank" rel="noreferrer">
            {isChinese ? '项目部署原文' : 'Source deployment guide'}
            <MovaIcon name="arrow-right" />
          </a>
        </aside>

        <div className="deploy-content">
          <section className="deploy-section" id="deploy-requirements">
            <SectionHeading
              eyebrow="Requirements"
              title={isChinese ? '环境要求' : 'Requirements'}
              text={isChinese ? 'MOVA 使用发布好的多架构 Linux 镜像，支持 amd64 与 arm64。' : 'MOVA uses a published multi-architecture Linux image for amd64 and arm64.'}
            />
            <div className="deploy-requirement-grid">
              <article><strong>Docker</strong><p>{isChinese ? 'Linux 使用 Docker Engine；macOS 与 Windows 可使用 Docker Desktop。' : 'Use Docker Engine on Linux or Docker Desktop on macOS and Windows.'}</p></article>
              <article><strong>Compose V2</strong><p>{isChinese ? '使用 docker compose 命令管理应用和 PostgreSQL。' : 'Use docker compose to manage the app and PostgreSQL.'}</p></article>
              <article><strong>{isChinese ? '媒体目录' : 'Media directory'}</strong><p>{isChinese ? '准备一个宿主机上的电影或剧集目录，并确保 Docker 可以读取。' : 'Prepare a movie or series directory on the host that Docker can read.'}</p></article>
            </div>
          </section>

          <section className="deploy-section" id="deploy-compose">
            <SectionHeading
              eyebrow="Docker Compose"
              title={isChinese ? 'Docker Compose 示例' : 'Docker Compose example'}
              text={isChinese
                ? '将下面的完整配置保存为 docker-compose.yml，并按照注释修改媒体路径、数据库密码和可选 Token。'
                : 'Save this complete configuration as docker-compose.yml, then update the media path, database password, and optional token as annotated.'}
            />
            <pre className="deploy-code"><code>{isChinese ? composeExampleZh : composeExampleEn}</code></pre>
            <div className="deploy-callout">
              <strong>{isChinese ? '数据持久化' : 'Persistent data'}</strong>
              <code>./data/postgres</code>
              <code>./data/cache</code>
              <span>{isChinese ? '媒体目录只读挂载到容器内 /media' : 'The media directory is mounted read-only at /media'}</span>
            </div>
          </section>

          <section className="deploy-section" id="deploy-quick-start">
            <SectionHeading
              eyebrow="Launch"
              title={isChinese ? '启动服务' : 'Launch the service'}
              text={isChinese
                ? '在 docker-compose.yml 所在目录执行启动命令，Docker 会自动拉取正式镜像。'
                : 'Run the launch command from the directory containing docker-compose.yml. Docker pulls the published image automatically.'}
            />
            <pre className="deploy-code"><code>{isChinese ? launchCommandZh : launchCommandEn}</code></pre>
            <div className="deploy-callout">
              <strong>{isChinese ? '启动后访问' : 'Open after launch'}</strong>
              <code>http://127.0.0.1:36080</code>
              <span>{isChinese ? '健康检查：/api/health' : 'Health check: /api/health'}</span>
            </div>
          </section>

          <section className="deploy-section" id="deploy-operations">
            <SectionHeading
              eyebrow="Operations"
              title={isChinese ? '运行与升级' : 'Operations and upgrades'}
              text={isChinese ? '容器服务名为 app，运行时容器名固定为 mova-app。' : 'The Compose service is app and the runtime container is named mova-app.'}
            />
            <pre className="deploy-code"><code>{operationsCommand}</code></pre>
            <p className="deploy-note">
              {isChinese
                ? 'pre-1.0 阶段的数据库结构可能无法平滑升级。涉及 schema 变更时，请先阅读项目最新 README，并做好数据备份。'
                : 'During pre-1.0, database schema changes may not support in-place upgrades. Check the latest project README and back up your data first.'}
            </p>
          </section>

          <section className="deploy-section" id="deploy-first-use">
            <SectionHeading
              eyebrow="First Run"
              title={isChinese ? '首次使用' : 'First use'}
              text={isChinese ? '服务启动完成后，通过网页端完成初始化。' : 'Complete setup in the Web app after the service starts.'}
            />
            <ol className="deploy-steps">
              {(isChinese ? [
                '打开 MOVA 网页端并创建第一个管理员。',
                '进入服务器设置，新建媒体库。',
                '选择容器内 /media 下的目录。',
                '保存后等待第一次后台扫描完成。',
              ] : [
                'Open the MOVA Web app and create the first administrator.',
                'Open server settings and create a media library.',
                'Choose a directory under /media inside the container.',
                'Save and wait for the first background scan to finish.',
              ]).map((step, index) => (
                <li key={step}><span>{String(index + 1).padStart(2, '0')}</span><p>{step}</p></li>
              ))}
            </ol>
          </section>
        </div>
      </section>
    </div>
  )
}

function SectionHeading({ eyebrow, title, text }: { eyebrow: string; title: string; text: string }) {
  return (
    <div className="deploy-section-heading">
      <p className="eyebrow">{eyebrow}</p>
      <h2>{title}</h2>
      <p>{text}</p>
    </div>
  )
}
