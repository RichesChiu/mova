import { useState } from 'react'
import { MovaIcon } from '../components/MovaIcon'
import { dockerUrl } from '../data/homeContent'
import { useI18n } from '../i18n-context'
import './DeploymentPage.css'

const composeExampleZh = `services:
  app:
    image: richeschiu/mova:latest
    # 使用外部 PostgreSQL 时：修改下方 MOVA_DATABASE_URL，
    # 并删除这个 depends_on 块与文件末尾的 database 服务
    depends_on:
      database:
        condition: service_healthy
    ports:
      - "36080:36080"
    environment:
      # Compose 内部数据库连接；需要修改凭据时同时修改 database.POSTGRES_PASSWORD
      MOVA_DATABASE_URL: "postgres://mova:postgres@database:5432/mova"
      # TMDB API Read Access Token；留空时会跳过远端元数据刮削
      MOVA_TMDB_ACCESS_TOKEN: ""
      # 中国大陆访问 TMDB 时可选；例如 http://192.168.1.1:7890
      HTTP_PROXY: ""
      HTTPS_PROXY: ""
    volumes:
      - ./data/cache:/app/data/cache
      - type: bind
        # 宿主机媒体目录：替换为实际绝对路径，容器内只读挂载
        source: /absolute/path/to/media
        target: /media
        read_only: true
    restart: unless-stopped

  database:
    # MOVA_DATABASE_URL 指向外部 PostgreSQL 时，删除整个 database 服务
    image: postgres:18
    environment:
      POSTGRES_USER: mova
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: mova
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
    image: richeschiu/mova:latest
    # External PostgreSQL: replace MOVA_DATABASE_URL below, then remove this
    # depends_on block and the database service at the end of the file
    depends_on:
      database:
        condition: service_healthy
    ports:
      - "36080:36080"
    environment:
      # Internal database connection; keep the password in sync with database.POSTGRES_PASSWORD
      MOVA_DATABASE_URL: "postgres://mova:postgres@database:5432/mova"
      # TMDB API Read Access Token; remote metadata scraping is skipped when empty
      MOVA_TMDB_ACCESS_TOKEN: ""
      # Optional for reaching TMDB; for example http://192.168.1.1:7890
      HTTP_PROXY: ""
      HTTPS_PROXY: ""
    volumes:
      - ./data/cache:/app/data/cache
      - type: bind
        # Host media directory: replace with the actual absolute path; mounted read-only
        source: /absolute/path/to/media
        target: /media
        read_only: true
    restart: unless-stopped

  database:
    # Delete this entire service when MOVA_DATABASE_URL points to external PostgreSQL
    image: postgres:18
    environment:
      POSTGRES_USER: mova
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: mova
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
  const { language, t } = useI18n()
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
          <p className="eyebrow">Mova 1.0 · Docker Deployment</p>
          <h1 id="deploy-title">{isChinese ? '用 Docker 运行 MOVA' : 'Run MOVA with Docker'}</h1>
          <p>
            {isChinese
              ? '准备 Docker 和一个可读取的媒体目录，再使用一份 Compose 配置同时运行 MOVA 与 PostgreSQL。'
              : 'Prepare Docker and a readable media directory, then use one Compose configuration to run MOVA with PostgreSQL.'}
          </p>
          <div className="deploy-actions">
            <a href="#deploy-compose">
              {isChinese ? '查看 Compose' : 'View Compose'}
              <MovaIcon name="arrow-right" />
            </a>
            <a href={dockerUrl} target="_blank" rel="noreferrer">
              {isChinese ? '稳定版镜像' : 'Stable image'}
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
              ? '复制并保存为 docker-compose.yml，在同一份文件中填写媒体目录、TMDB Token 和可选代理，然后直接启动。'
              : 'Copy and save as docker-compose.yml, set the media path, TMDB token, and optional proxy in this one file, then start the stack.'}
          />
          <div className="deploy-release-note">
            <div>
              <strong>{isChinese ? '稳定发布通道' : 'Stable release channel'}</strong>
              <code>richeschiu/mova:latest</code>
            </div>
            <p>
              {isChinese
                ? 'latest 指向当前稳定版本，preview 仅用于后续预发布验证。从任何 Preview 版本首次升级到 1.0，都需要完成最后一次数据库重建并重新扫描。'
                : 'latest points to the current stable release, while preview is reserved for future pre-release validation. Moving from any Preview release to 1.0 requires one final database rebuild and library rescan.'}
            </p>
          </div>
          <div className="deploy-compose-block">
            <div className="deploy-compose-toolbar">
              <span>docker-compose.yml · Mova 1.0</span>
              <button type="button" onClick={() => void copyCompose()}>
                {copyState === 'idle' ? (isChinese ? '复制配置' : 'Copy configuration') : copyLabel}
              </button>
            </div>
            <pre className="deploy-code"><code>{composeExample}</code></pre>
          </div>
          <div className="deploy-compose-meta">
            <article>
              <strong>{isChinese ? '唯一必改项' : 'Only required change'}</strong>
              <p><code>/absolute/path/to/media</code></p>
            </article>
            <article>
              <strong>{isChinese ? '可选配置' : 'Optional setting'}</strong>
              <p>
                <code>MOVA_TMDB_ACCESS_TOKEN</code><br />
                <code>HTTP_PROXY / HTTPS_PROXY</code>
              </p>
            </article>
            <article>
              <strong>{isChinese ? '持久化数据' : 'Persistent data'}</strong>
              <p><code>./data/postgres</code><br /><code>./data/cache</code></p>
            </article>
          </div>
          <div className="deploy-compose-guidance deploy-proxy-guide">
            <strong>{t('代理填写规则')}</strong>
            <p>{t('HTTP_PROXY 和 HTTPS_PROXY 主要用于中国大陆网络访问 TMDB 元数据与图片。代理值必须是容器能够访问的完整 URL，格式为“协议://宿主机实际地址:代理端口”。')}</p>
            <p>{t('假设宿主机的局域网地址是 192.168.1.1，HTTP 代理端口是 7890，应按下面的方式填写；请根据自己的实际地址和端口替换示例值。')}</p>
            <pre><code>{`HTTP_PROXY: "http://192.168.1.1:7890"\nHTTPS_PROXY: "http://192.168.1.1:7890"`}</code></pre>
            <p>{t('代理程序必须允许来自 Docker 网络或局域网的连接。不要使用 127.0.0.1 或 localhost，它们在容器内指向 MOVA 容器自身。')}</p>
            <p>{t('如果 MOVA 容器已经能直接访问 TMDB，例如透明代理、TUN 或路由器代理已经对 Docker 生效，则两个变量可以留空。仅在宿主机启动普通代理程序不会自动让容器继承代理。')}</p>
            <p>{t('这里的代理只影响 MOVA 运行时请求；Docker Hub 镜像拉取代理仍需在 Docker Desktop 或 Docker Engine 中配置。')}</p>
          </div>
          <p className="deploy-compose-guidance">
            {isChinese
              ? '通过 HTTPS 反向代理公开 Web 页面时，在 app.environment 中额外设置 MOVA_SESSION_COOKIE_SECURE: "true"。本地纯 HTTP 部署保持默认值即可。'
              : 'When exposing the Web app through an HTTPS reverse proxy, add MOVA_SESSION_COOKIE_SECURE: "true" to app.environment. Keep the default for local HTTP deployments.'}
          </p>
          <p className="deploy-compose-guidance">
            {isChinese
              ? '36080 是 Web/API 端口；PostgreSQL 不向宿主机发布端口，只能由 Compose 内的应用访问。MOVA_DATABASE_URL 与 POSTGRES_PASSWORD 必须使用同一密码。'
              : 'Port 36080 serves Web/API traffic. PostgreSQL publishes no host port and is reachable only by the app inside Compose. MOVA_DATABASE_URL and POSTGRES_PASSWORD must use the same password.'}
          </p>
          <p className="deploy-compose-guidance">
            {isChinese
              ? '使用外部 PostgreSQL 时，将 MOVA_DATABASE_URL 改为容器可访问的数据库地址，并删除 app.depends_on.database 与整个 database 服务。外部数据库需提前创建，连接账号需要拥有建表、修改表结构和执行迁移的权限；容器内的 localhost 指向 MOVA 容器自身。'
              : 'To use external PostgreSQL, replace MOVA_DATABASE_URL with an address reachable from the container, then remove app.depends_on.database and the entire database service. Create the database first and grant the account permission to create and alter tables and run migrations; localhost inside the container refers to the Mova container itself.'}
          </p>
        </section>

        <section className="deploy-section" id="deploy-tmdb">
          <SectionHeading
            eyebrow="TMDB Metadata"
            title={t('获取 TMDB Access Token')}
            text={t('TMDB Token 用于获取影片与剧集的元数据、海报、背景图、标题 Logo 和评分。')}
          />
          <ol className="deploy-token-steps">
            <li>
              <strong>{t('注册并验证账户')}</strong>
              <p>
                {t('打开 TMDB，注册或登录账户并完成邮箱验证。')}{' '}
                <a href="https://www.themoviedb.org/" target="_blank" rel="noreferrer">TMDB</a>
              </p>
            </li>
            <li>
              <strong>{t('申请 API 访问权限')}</strong>
              <p>
                {t('建议使用桌面浏览器打开账户设置中的 API 页面，按页面要求提交申请并接受 TMDB 条款。')}{' '}
                <a href="https://www.themoviedb.org/settings/api" target="_blank" rel="noreferrer">{t('打开 TMDB API 设置')}</a>
              </p>
            </li>
            <li>
              <strong>{t('复制正确的 Token')}</strong>
              <p>{t('申请通过后，在同一页面复制 API Read Access Token。MOVA 使用的是这段较长的 Bearer Token，不是 API Key (v3 auth)。')}</p>
            </li>
            <li>
              <strong>{t('写入 Compose 并重启')}</strong>
              <p>{t('将 Token 填入 docker-compose.yml 的 MOVA_TMDB_ACCESS_TOKEN，然后执行 docker compose up -d。')}</p>
              <pre><code>{`MOVA_TMDB_ACCESS_TOKEN: "${t('你的_API_Read_Access_Token')}"`}</code></pre>
            </li>
          </ol>
          <div className="deploy-compose-guidance deploy-token-note">
            <p>{t('Token 属于敏感凭据，不要提交到 Git 仓库或写入公开日志。')}</p>
            <p>{t('不配置 Token 时，MOVA 仍可启动、扫描本地文件并完成入库，但会跳过 TMDB 元数据与图片刮削。后续补上 Token 并重启服务后，重新扫描媒体库即可补齐远端元数据，无需重建数据库。')}</p>
            <a href="https://developer.themoviedb.org/docs/authentication-application" target="_blank" rel="noreferrer">
              {t('查看 TMDB 官方认证说明')}
            </a>
          </div>
        </section>

        <section className="deploy-section" id="deploy-after">
          <SectionHeading
            eyebrow="After Deployment"
            title={isChinese ? '部署完成后' : 'After deployment'}
            text={isChinese
              ? '只从可信本机或受控局域网打开网页端并创建管理员，再从容器内的 /media 目录建立媒体库。'
              : 'Open the Web app from a trusted local machine or controlled LAN to create the administrator, then create a library from /media inside the container.'}
          />
          <div className="deploy-after-grid">
            <article><span>Web</span><strong>http://127.0.0.1:36080</strong></article>
            <article><span>{isChinese ? '健康检查' : 'Health check'}</span><strong>/api/health</strong></article>
            <article><span>{isChinese ? '容器媒体目录' : 'Container media path'}</span><strong>/media</strong></article>
          </div>
          <p className="deploy-compose-guidance">
            {isChinese
              ? '首个系统管理员创建完成前，不要将未初始化的服务暴露到公网。初始化完成后再配置 HTTPS 反向代理。'
              : 'Do not expose an uninitialized service to the Internet. Configure an HTTPS reverse proxy only after creating the first system administrator.'}
          </p>
        </section>

        <section className="deploy-section" id="deploy-maintenance">
          <SectionHeading
            eyebrow="Data Maintenance"
            title={isChinese ? '升级与数据维护' : 'Upgrades and data maintenance'}
            text={isChinese
              ? '稳定升级前先备份 PostgreSQL；数据库是权威状态，图片与媒体派生缓存可以按需重建。'
              : 'Back up PostgreSQL before stable upgrades. The database is authoritative; artwork and derived media caches can be rebuilt on demand.'}
          />
          <div className="deploy-after-grid">
            <article>
              <span>{isChinese ? 'Preview → 1.0' : 'Preview → 1.0'}</span>
              <strong>{isChinese ? '一次重建并重扫' : 'One rebuild and rescan'}</strong>
            </article>
            <article>
              <span>{isChinese ? '1.0 数据库基线' : '1.0 database baseline'}</span>
              <strong>{isChinese ? '正式冻结 0001' : 'Freeze migration 0001'}</strong>
            </article>
            <article>
              <span>{isChinese ? '1.0 后续升级' : 'Upgrades after 1.0'}</span>
              <strong>{isChinese ? '顺序迁移' : 'Sequential migrations'}</strong>
            </article>
          </div>
          <p className="deploy-compose-guidance">
            <a
              href="https://github.com/RichesChiu/mova/blob/master/docs/DEPLOYMENT.md"
              target="_blank"
              rel="noreferrer"
            >
              {isChinese ? '查看完整的升级、备份、恢复与回滚步骤' : 'Read the complete upgrade, backup, restore, and rollback guide'}
            </a>
          </p>
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
