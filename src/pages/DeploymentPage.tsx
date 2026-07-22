import { useState } from 'react'
import { MovaIcon } from '../components/MovaIcon'
import { dockerUrl } from '../data/homeContent'
import { useI18n } from '../i18n-context'
import './DeploymentPage.css'

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

const composePreviewZh = `services:
  app:
    image: richeschiu/mova:latest
    ports:
      - "36080:36080"
    environment:
      MOVA_DATABASE_URL: postgres://mova:••••••••@database:5432/mova
      MOVA_TMDB_ACCESS_TOKEN: ""
    volumes:
      - ./data/cache:/app/data/cache
      - type: bind
        source: /你的媒体目录
        target: /media
        read_only: true

  database:
    image: postgres:18`

const composePreviewEn = `services:
  app:
    image: richeschiu/mova:latest
    ports:
      - "36080:36080"
    environment:
      MOVA_DATABASE_URL: postgres://mova:••••••••@database:5432/mova
      MOVA_TMDB_ACCESS_TOKEN: ""
    volumes:
      - ./data/cache:/app/data/cache
      - type: bind
        source: /your/media/path
        target: /media
        read_only: true

  database:
    image: postgres:18`

export function DeploymentPage({ onNavigate }: { onNavigate: (sectionId: string) => void }) {
  const { language } = useI18n()
  const [hasCopied, setHasCopied] = useState(false)
  const isChinese = language === 'zh'
  const composeExample = isChinese ? composeExampleZh : composeExampleEn

  const copyCompose = async () => {
    await navigator.clipboard.writeText(composeExample)
    setHasCopied(true)
    window.setTimeout(() => setHasCopied(false), 1800)
  }

  return (
    <div className="deploy-page">
      <section className="deploy-hero" aria-labelledby="deploy-title">
        <div className="deploy-hero-copy">
          <p className="eyebrow">Docker Deployment</p>
          <h1 id="deploy-title">{isChinese ? '用 Docker 运行 MOVA' : 'Run MOVA with Docker'}</h1>
          <p>
            {isChinese
              ? '准备 Docker 和一个可读取的媒体目录，再使用 Compose 同时运行 MOVA 与 PostgreSQL。所有必要配置都集中在一个文件中。'
              : 'Prepare Docker and a readable media directory, then use Compose to run MOVA with PostgreSQL. Everything required lives in one file.'}
          </p>
          <div className="deploy-actions">
            <a href="#deploy-compose">
              {isChinese ? '查看 Compose' : 'View Compose'}
              <MovaIcon name="arrow-right" />
            </a>
            <a href={dockerUrl} target="_blank" rel="noreferrer">
              {isChinese ? 'Docker 镜像' : 'Docker image'}
              <MovaIcon name="arrow-right" />
            </a>
            <button type="button" onClick={() => onNavigate('api')}>
              {isChinese ? 'API 文档' : 'API docs'}
              <MovaIcon name="arrow-right" />
            </button>
          </div>
        </div>

        <aside className="deploy-terminal" aria-label={isChinese ? 'Docker Compose 配置预览' : 'Docker Compose configuration preview'}>
          <div className="deploy-terminal-bar" aria-hidden="true">
            <span />
            <span />
            <span />
            <strong>docker-compose.yml</strong>
          </div>
          <pre><code>{isChinese ? composePreviewZh : composePreviewEn}</code></pre>
          <div className="deploy-terminal-status">
            <i aria-hidden="true" />
            {isChinese ? 'MOVA · PostgreSQL · 只读媒体目录' : 'MOVA · PostgreSQL · Read-only media'}
          </div>
        </aside>
      </section>

      <div className="deploy-content">
        <section className="deploy-section" id="deploy-requirements">
          <SectionHeading
            eyebrow="Environment"
            title={isChinese ? '部署环境' : 'Environment'}
            text={isChinese
              ? '无需源码和本地构建，只需要 Docker、Compose V2 与宿主机媒体目录。'
              : 'No source checkout or local build is needed—only Docker, Compose V2, and a host media directory.'}
          />
          <div className="deploy-requirement-grid">
            <article>
              <strong>Docker</strong>
              <p>{isChinese ? 'Linux 使用 Docker Engine，macOS 与 Windows 使用 Docker Desktop。' : 'Use Docker Engine on Linux or Docker Desktop on macOS and Windows.'}</p>
            </article>
            <article>
              <strong>Compose V2</strong>
              <p>{isChinese ? '负责运行 MOVA 和 PostgreSQL，并管理依赖与持久化目录。' : 'Runs MOVA and PostgreSQL while managing dependencies and persistent data.'}</p>
            </article>
            <article>
              <strong>amd64 / arm64</strong>
              <p>{isChinese ? '正式镜像覆盖两种 Linux 架构，Docker 会自动选择对应版本。' : 'The published image supports both Linux architectures and Docker selects the correct one.'}</p>
            </article>
            <article>
              <strong>{isChinese ? '媒体目录' : 'Media directory'}</strong>
              <p>{isChinese ? '准备宿主机绝对路径，Compose 会将其只读挂载到 /media。' : 'Provide an absolute host path; Compose mounts it read-only at /media.'}</p>
            </article>
          </div>
        </section>

        <section className="deploy-section deploy-compose-section" id="deploy-compose">
          <SectionHeading
            eyebrow="Docker Compose"
            title={isChinese ? '完整 Compose 配置' : 'Complete Compose configuration'}
            text={isChinese
              ? '保存为 docker-compose.yml。复制后只需修改媒体路径、数据库密码，以及可选的 TMDB Token。'
              : 'Save as docker-compose.yml. After copying, only update the media path, database password, and optional TMDB token.'}
          />
          <div className="deploy-compose-block">
            <div className="deploy-compose-toolbar">
              <span>docker-compose.yml</span>
              <button type="button" onClick={() => void copyCompose()}>
                {hasCopied ? (isChinese ? '已复制' : 'Copied') : (isChinese ? '复制配置' : 'Copy configuration')}
              </button>
            </div>
            <pre className="deploy-code"><code>{composeExample}</code></pre>
          </div>
          <div className="deploy-compose-meta">
            <article>
              <strong>{isChinese ? '必须修改' : 'Required changes'}</strong>
              <p><code>/absolute/path/to/media</code><br /><code>change_this_password</code></p>
            </article>
            <article>
              <strong>{isChinese ? '可选配置' : 'Optional setting'}</strong>
              <p><code>MOVA_TMDB_ACCESS_TOKEN</code></p>
            </article>
            <article>
              <strong>{isChinese ? '持久化数据' : 'Persistent data'}</strong>
              <p><code>./data/postgres</code><br /><code>./data/cache</code></p>
            </article>
          </div>
        </section>

        <section className="deploy-section" id="deploy-after">
          <SectionHeading
            eyebrow="After Deployment"
            title={isChinese ? '部署完成后' : 'After deployment'}
            text={isChinese
              ? '打开网页端创建管理员，再从容器内的 /media 目录建立媒体库。'
              : 'Open the Web app to create an administrator, then create a library from /media inside the container.'}
          />
          <div className="deploy-after-grid">
            <article><span>Web</span><strong>http://127.0.0.1:36080</strong></article>
            <article><span>{isChinese ? '健康检查' : 'Health check'}</span><strong>/api/health</strong></article>
            <article><span>{isChinese ? '容器媒体目录' : 'Container media path'}</span><strong>/media</strong></article>
          </div>
        </section>
      </div>
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
