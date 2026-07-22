import { useState } from 'react'
import { MovaIcon } from '../components/MovaIcon'
import { dockerUrl } from '../data/homeContent'
import { useI18n } from '../i18n-context'
import './DeploymentPage.css'

const composeExampleZh = `services:
  app:
    image: richeschiu/mova:preview
    container_name: mova-app
    depends_on:
      database:
        condition: service_healthy
    ports:
      - "36080:36080"
    environment:
      MOVA_DATABASE_URL: postgres://mova:postgres@database:5432/mova
      MOVA_WEB_DIST_DIR: /app/web
      # TMDB API Read Access Token；留空时会跳过远端元数据刮削
      MOVA_TMDB_ACCESS_TOKEN: ""
      # 后台 worker 并发数，普通部署保持 2 即可
      MOVA_WORKER_CONCURRENCY: "2"
    volumes:
      - ./data/cache:/app/data/cache
      - type: bind
        # 宿主机媒体目录：替换为实际绝对路径，容器内只读挂载
        source: /absolute/path/to/media
        target: /media
        read_only: true
    restart: unless-stopped

  database:
    image: postgres:18
    environment:
      POSTGRES_USER: mova
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: mova
      PGDATA: /var/lib/postgresql/18/docker
    volumes:
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
    image: richeschiu/mova:preview
    container_name: mova-app
    depends_on:
      database:
        condition: service_healthy
    ports:
      - "36080:36080"
    environment:
      MOVA_DATABASE_URL: postgres://mova:postgres@database:5432/mova
      MOVA_WEB_DIST_DIR: /app/web
      # TMDB API Read Access Token; remote metadata scraping is skipped when empty
      MOVA_TMDB_ACCESS_TOKEN: ""
      # Background worker concurrency; keep 2 for a typical deployment
      MOVA_WORKER_CONCURRENCY: "2"
    volumes:
      - ./data/cache:/app/data/cache
      - type: bind
        # Host media directory: replace with the actual absolute path; mounted read-only
        source: /absolute/path/to/media
        target: /media
        read_only: true
    restart: unless-stopped

  database:
    image: postgres:18
    environment:
      POSTGRES_USER: mova
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: mova
      PGDATA: /var/lib/postgresql/18/docker
    volumes:
      - ./data/postgres:/var/lib/postgresql
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U mova -d mova"]
      interval: 5s
      timeout: 5s
      retries: 12
    shm_size: 256mb
    restart: unless-stopped`

export function DeploymentPage({ onNavigate }: { onNavigate: (sectionId: string) => void }) {
  const { language } = useI18n()
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle')
  const isChinese = language === 'zh'
  const composeExample = isChinese ? composeExampleZh : composeExampleEn
  const copyLabel = copyState === 'copied'
    ? (isChinese ? '已复制' : 'Copied')
    : copyState === 'failed'
      ? (isChinese ? '复制失败' : 'Copy failed')
      : (isChinese ? '复制' : 'Copy')

  const copyCompose = async () => {
    try {
      await navigator.clipboard.writeText(composeExample)
      setCopyState('copied')
    } catch {
      setCopyState('failed')
    }

    window.setTimeout(() => setCopyState('idle'), 1800)
  }

  return (
    <div className="deploy-page">
      <section className="deploy-hero" aria-labelledby="deploy-title">
        <div className="deploy-hero-copy">
          <p className="eyebrow">1.0.0 Preview · Docker Deployment</p>
          <h1 id="deploy-title">{isChinese ? '用 Docker 运行 MOVA' : 'Run MOVA with Docker'}</h1>
          <p>
            {isChinese
              ? '当前公开版本为 1.0.0 Preview。准备 Docker 和一个可读取的媒体目录，再使用 Compose 同时运行 MOVA 与 PostgreSQL。'
              : 'The current public release is 1.0.0 Preview. Prepare Docker and a readable media directory, then use Compose to run MOVA with PostgreSQL.'}
          </p>
          <div className="deploy-actions">
            <a href="#deploy-compose">
              {isChinese ? '查看 Compose' : 'View Compose'}
              <MovaIcon name="arrow-right" />
            </a>
            <a href={dockerUrl} target="_blank" rel="noreferrer">
              {isChinese ? 'Preview 镜像' : 'Preview image'}
              <MovaIcon name="arrow-right" />
            </a>
            <button type="button" onClick={() => onNavigate('api')}>
              {isChinese ? 'API 文档' : 'API docs'}
              <MovaIcon name="arrow-right" />
            </button>
          </div>
        </div>
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
              ? '保存为 docker-compose.yml。复制后只需修改媒体目录，并按需填写 TMDB Token。'
              : 'Save as docker-compose.yml. After copying, update the media directory and optionally add a TMDB token.'}
          />
          <div className="deploy-preview-note">
            <div>
              <strong>{isChinese ? '当前公开体验通道' : 'Current public preview channel'}</strong>
              <code>richeschiu/mova:preview</code>
            </div>
            <p>
              {isChinese
                ? '需要固定版本时可改用 richeschiu/mova:1.0.0-preview.1。Preview 阶段的 schema 仍可能变化，升级后如不兼容，需要重建数据库并重新扫描媒体库。'
                : 'Use richeschiu/mova:1.0.0-preview.1 to pin this release. The schema may still change during Preview; incompatible upgrades require rebuilding the database and rescanning libraries.'}
            </p>
          </div>
          <div className="deploy-compose-block">
            <div className="deploy-compose-toolbar">
              <span>docker-compose.yml · 1.0.0 Preview</span>
              <button type="button" onClick={() => void copyCompose()}>
                {copyState === 'idle' ? (isChinese ? '复制配置' : 'Copy configuration') : copyLabel}
              </button>
            </div>
            <pre className="deploy-code"><code>{composeExample}</code></pre>
          </div>
          <div className="deploy-compose-meta">
            <article>
              <strong>{isChinese ? '必须修改' : 'Required change'}</strong>
              <p><code>/absolute/path/to/media</code></p>
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
