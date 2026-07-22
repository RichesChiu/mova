# Mova

> 把本地影视库，变成你的私人流媒体。

**语言 / Language：中文 · [English](#english)**

Mova 是一个使用 Rust 构建的轻量、自托管媒体服务器，用于扫描、整理和播放本地电影与剧集。Web 端已集成在镜像中，macOS 和 iOS 客户端正在开发。

## 核心能力

- 扫描、整理电影和剧集媒体库
- 获取 TMDB 元数据、海报和背景图
- 多用户、媒体库访问权限和跨设备会话
- 继续观看、播放进度、最近添加、搜索和通知
- 后台扫描任务、实时进度和增量同步
- 只读挂载媒体目录，不修改原始媒体文件
- 同时支持 `linux/amd64` 和 `linux/arm64`

## 快速开始

先创建一个独立部署目录，不需要克隆源码仓库：

```bash
mkdir -p mova
cd mova
```

在目录中创建 `docker-compose.yml`，完整内容如下：

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
    restart: unless-stopped
```

`MOVA_TMDB_ACCESS_TOKEN` 用于启用 TMDB 自动刮削、海报/背景图以及元数据搜索与替换。获取方式：

1. 注册并登录 [TMDB](https://www.themoviedb.org/)，完成邮箱验证。
2. 进入账户设置的 [API 页面](https://www.themoviedb.org/settings/api)，申请 API 访问权限。
3. 复制 **API Read Access Token**，不要使用较短的 `API Key (v3 auth)`。
4. 把 Token 填入 `docker-compose.yml` 的 `MOVA_TMDB_ACCESS_TOKEN`，不要把包含真实 Token 的部署文件提交到 Git 仓库。

TMDB 官方说明：[Application Authentication](https://developer.themoviedb.org/v4/docs/authentication-application)。不配置 Token 时 Mova 仍可启动、扫描本地文件、读取 NFO/sidecar、入库和播放，但会自动跳过 TMDB 刮削，条目不会获得 TMDB 标题、简介和远端图片。后续配置 Token 并重启服务、重新扫描媒体库即可补做刮削。

启动 Mova：

```bash
docker compose up -d
```

打开 Web 页面：

```text
http://localhost:36080
```

首次打开后，创建首个系统管理员，然后进入服务器设置创建媒体库。

## Docker 镜像

```bash
docker pull richeschiu/mova:latest
```

当前发布平台：

- `linux/amd64`
- `linux/arm64`

Windows 和 macOS 用户可以通过 Docker Desktop 运行该 Linux 镜像，Linux 用户可以通过 Docker Engine 或 Docker Desktop 运行。

升级到最新镜像：

```bash
docker compose pull
docker compose up -d
```

## 数据与隐私

媒体目录以只读方式挂载。Mova 会将用户、媒体库、元数据、播放进度、通知和后台任务状态保存在独立的 PostgreSQL 数据库中，并将图片等资源保存在独立缓存目录中，不会修改原始媒体文件。

## 项目状态

Mova 目前处于 pre-1.0 预览阶段，适合本机、家庭服务器和私人媒体库场景。快速迭代期间可能出现破坏性数据库结构调整，升级后可能需要重建数据库并重新扫描媒体库。

## 相关链接

- [GitHub 源码](https://github.com/RichesChiu/mova)
- [部署与首次使用](https://github.com/RichesChiu/mova#部署)
- [API 与技术文档](https://github.com/RichesChiu/mova/tree/master/docs)
- [问题反馈](https://github.com/RichesChiu/mova/issues)
- [AGPL-3.0-only 许可证](https://github.com/RichesChiu/mova/blob/master/LICENSE)

---

## English

[返回中文](#mova)

> Turn your local media library into your own streaming service.

Mova is a lightweight, self-hosted media server built with Rust for scanning, organizing, and streaming local movies and TV shows. The Web app is included in the image, with macOS and iOS clients in development.

### Features

- Scan and organize movie and TV libraries
- Fetch TMDB metadata, posters, and backdrops
- Multi-user library access and cross-device sessions
- Continue watching, playback progress, recently added, search, and notifications
- Background scan jobs, realtime progress, and incremental synchronization
- Read-only media mounts that leave original files untouched
- Multi-platform images for `linux/amd64` and `linux/arm64`

### Quick Start

Create a standalone deployment directory. Cloning the source repository is not required:

```bash
mkdir -p mova
cd mova
```

Create `docker-compose.yml` in that directory with the following complete contents:

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
      # TMDB API Read Access Token; leave empty to skip remote metadata scraping
      MOVA_TMDB_ACCESS_TOKEN: ""
      # Background worker concurrency; 2 is suitable for most deployments
      MOVA_WORKER_CONCURRENCY: "2"
    volumes:
      - ./data/cache:/app/data/cache
      - type: bind
        # Host media directory: replace with an absolute path; mounted read-only
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
    restart: unless-stopped
```

`MOVA_TMDB_ACCESS_TOKEN` enables automatic TMDB scraping, remote artwork, and metadata search/replacement:

1. Create and verify a [TMDB](https://www.themoviedb.org/) account.
2. Open the account [API settings](https://www.themoviedb.org/settings/api) and apply for API access.
3. Copy the **API Read Access Token**, not the shorter `API Key (v3 auth)`.
4. Store it in `MOVA_TMDB_ACCESS_TOKEN` inside `docker-compose.yml`, and never commit a deployment file containing the real token to Git.

See TMDB's [Application Authentication](https://developer.themoviedb.org/v4/docs/authentication-application) documentation. Without the token, Mova still starts and supports local scanning, NFO/sidecar metadata, importing, and playback, but skips all TMDB scraping. Add the token later, restart Mova, and rescan the library to enrich previously skipped items.

Start Mova:

```bash
docker compose up -d
```

Open the Web app:

```text
http://localhost:36080
```

On first launch, create the initial system administrator and then create a media library from Server Settings.

### Docker Image

```bash
docker pull richeschiu/mova:latest
```

Published platforms:

- `linux/amd64`
- `linux/arm64`

Windows and macOS users can run the Linux image through Docker Desktop. Linux users can run the same image through Docker Engine or Docker Desktop.

Upgrade to the latest image:

```bash
docker compose pull
docker compose up -d
```

### Data and Privacy

Media directories are mounted read-only. Mova stores users, libraries, metadata, playback progress, notifications, and background job state in a separate PostgreSQL database. Artwork and generated resources are stored in a separate cache without modifying original media files.

### Project Status

Mova is currently a pre-1.0 preview for local machines, home servers, and private media libraries. Breaking database changes may occur during rapid development and can require rebuilding the database and rescanning media libraries.

### Links

- [GitHub Repository](https://github.com/RichesChiu/mova)
- [Deployment and First Run](https://github.com/RichesChiu/mova#部署)
- [API and Technical Documentation](https://github.com/RichesChiu/mova/tree/master/docs)
- [Issue Tracker](https://github.com/RichesChiu/mova/issues)
- [AGPL-3.0-only License](https://github.com/RichesChiu/mova/blob/master/LICENSE)
