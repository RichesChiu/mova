<p align="center">
  <img src="apps/mova-web/public/mova-logo-master-transparent.png" alt="Mova 标志" width="96" />
</p>

<h1 align="center">Mova</h1>

<p align="center">
  面向本地电影和剧集的轻量、安全、高效自托管媒体服务器。
</p>

## Mova 是什么

Mova 是一个用于整理、浏览和播放本地电影与剧集的自托管媒体服务器。服务端使用 Rust 构建，这是一门强调内存安全、稳定性能和资源效率的现代系统语言。

项目希望把媒体服务器体验保持得足够简单可靠：挂载媒体目录，扫描媒体库，按需补齐元数据，然后通过 Web、macOS 和 iOS 客户端浏览与播放。当前版本定位为 pre-1.0 MVP 预览版，适合本机、家用服务器和私人媒体库场景。

核心能力包括：

- 电影与剧集媒体库扫描、整理和 TMDB 元数据补全
- 多用户、媒体库访问控制和跨设备会话管理
- 继续观看、最近添加、搜索、通知和网页播放
- 后台扫描任务、实时进度同步和增量更新
- Docker 部署，以及 Web、macOS、iOS 多端接入

具体接口、扫描规则、实时协议和模块实现见下方“文档”章节。

## 部署

### 环境要求

- Docker
- Docker Compose
- 一个宿主机上的本地媒体目录

### 创建部署目录

```bash
mkdir -p mova
cd mova
```

### Docker Compose 示例

下面的配置可以直接保存为 `docker-compose.yml`。媒体目录以只读方式挂载，数据库与图片缓存保存在 Compose 文件所在目录的 `data/` 下。

```yaml
services:
  app:
    image: richeschiu/mova:latest
    container_name: mova-app
    depends_on:
      database:
        condition: service_healthy
    ports:
      - "36080:36080"
    environment:
      MOVA_DATABASE_URL: postgres://mova:postgres@database:5432/mova
      MOVA_WEB_DIST_DIR: /app/web
      MOVA_TMDB_ACCESS_TOKEN: ${MOVA_TMDB_ACCESS_TOKEN:-}
      MOVA_WORKER_CONCURRENCY: ${MOVA_WORKER_CONCURRENCY:-2}
    volumes:
      - ./data/cache:/app/data/cache
      - type: bind
        source: ${MOVA_MEDIA_ROOT:?MOVA_MEDIA_ROOT must be set}
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
    restart: unless-stopped
```

### 配置

在 `docker-compose.yml` 所在目录创建 `.env`：

```env
MOVA_MEDIA_ROOT=/absolute/path/to/media
MOVA_TMDB_ACCESS_TOKEN=
MOVA_WORKER_CONCURRENCY=2
```

- `MOVA_MEDIA_ROOT` 必填，会只读挂载到容器内固定目录 `/media`。
- `MOVA_TMDB_ACCESS_TOKEN` 用于启用 TMDB 自动刮削、远端海报/背景图以及元数据搜索与替换。不配置时服务仍可启动，并保留本地扫描、NFO/sidecar 读取、入库和播放能力，但会自动跳过所有 TMDB 请求。
- `MOVA_WORKER_CONCURRENCY` 控制进程内后台 worker 池并发数，默认值为 `2`。

### 获取 TMDB Access Token

1. 注册并登录 [TMDB](https://www.themoviedb.org/)，完成邮箱验证。
2. 打开账户设置中的 [API 页面](https://www.themoviedb.org/settings/api)，按页面要求申请 API 访问权限并接受 TMDB 条款。
3. 申请通过后，在同一页面复制 **API Read Access Token**。Mova 使用的是这段较长的 Bearer Token，不是 `API Key (v3 auth)`。
4. 将 Token 写入部署目录的 `.env`：

```env
MOVA_TMDB_ACCESS_TOKEN=你的_API_Read_Access_Token
```

Token 属于敏感凭据，不要提交到 Git 仓库或放进公开日志。TMDB 的官方认证说明见 [Application Authentication](https://developer.themoviedb.org/v4/docs/authentication-application)。

如果暂时不配置 Token，扫描条目会以 `skipped / metadata_provider_disabled` 完成本地入库，不会被记为刮削失败。后续补上 Token 并执行 `docker compose up -d` 重启服务，再重新扫描媒体库，即可只对需要远端元数据的条目补做 TMDB 刮削，无需重建数据库。

### 启动

```bash
docker compose up -d
```

默认地址：

- Web: `http://127.0.0.1:36080`
- Health: `http://127.0.0.1:36080/api/health`

启动后，Mova 会生成两个运行时目录：

- `data/postgres/`：PostgreSQL 数据库文件，用于保存媒体库、用户、元数据、播放进度、持久化通知与已读状态、后台任务和实时资源 revision。
- `data/cache/`：缓存海报、背景图和生成的媒体资源。删除媒体库时，也会清理该库独占引用的 TMDB 图片缓存。

当前仍处于 pre-1.0 MVP 预览版阶段，schema 变更继续直接修改 `migrations/0001_init.sql`。当前 schema 包含 realtime、后台任务、扫描检查点和通用通知表，无法平滑升级旧数据库：需要重置 `data/postgres/`、重新初始化数据库并重新扫描媒体库。

媒体目录只读挂载，Mova 不会修改你的原始媒体文件。

默认 Compose 文件会直接运行已发布的 `richeschiu/mova:latest` 镜像，不在部署机器上从源码构建。本地没有镜像时，`docker compose up -d` 会自动拉取；如果你想主动升级到最新发布镜像，自己先执行 `docker compose pull`，再执行 `docker compose up -d`。

已发布镜像覆盖 `linux/amd64` 和 `linux/arm64`。Windows 和 macOS 宿主机通过 Docker Desktop 运行同一个 Linux 镜像，Linux 宿主机通过 Docker Engine 或 Docker Desktop 运行，Docker 会自动选择匹配的架构。

应用服务名是 `app`，运行时容器固定为 `mova-app`；查看服务日志时使用 `docker compose logs -f app`。

### 首次使用

1. 容器启动后打开 Web 页面。
2. 在初始化页面创建第一个管理员。
3. 进入服务器设置并创建媒体库。
4. 选择容器内 `/media` 下的目录。
5. 保存媒体库后，Mova 会自动开始第一次扫描。

## 文档

- API: [docs/API.md](docs/API.md)
- SSE 同步协议: [docs/SSE.md](docs/SSE.md)
- 媒体库扫描与刮削设计: [docs/MEDIA_LIBRARY_SCAN.md](docs/MEDIA_LIBRARY_SCAN.md)
- TMDB 对接审查与目标契约: [docs/TMDB.md](docs/TMDB.md)
- Docker Hub Overview: [docs/DOCKER_HUB.md](docs/DOCKER_HUB.md)
- 前端: [apps/mova-web/README.md](apps/mova-web/README.md)
- 后端: [apps/mova-server/README.md](apps/mova-server/README.md)
- Crates: [crates/README.md](crates/README.md)

## 路线图与反馈

Mova 仍在积极迭代中。作者也在积极维护 Pad 和 macOS 客户端方向，让它们可以更自然地接入同一个自托管媒体服务器。

欢迎提交反馈、功能建议、客户端接入想法和体验改进意见。

## 许可证

当前许可证：`AGPL-3.0-only`。详见 [LICENSE](LICENSE)。
