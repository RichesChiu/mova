<p align="center">
  <img src="apps/mova-web/public/mova-logo-master-transparent.png" alt="Mova 标志" width="96" />
</p>

<h1 align="center">Mova</h1>

<p align="center">
  面向本地电影和剧集的轻量、安全、高效自托管媒体服务器。
</p>

<p align="center">
  简体中文 · <a href="README.en.md">English</a>
</p>

## Mova 是什么

Mova 使用 Rust 构建，用于整理、浏览和播放本地电影与剧集。它把媒体目录、元数据、用户权限、播放进度和多端同步集中在一个可自行部署的服务中，并提供内置 Web 界面以及面向 macOS、iOS 等客户端的 API。

主要能力：

- 扫描电影与剧集，识别 NFO 和本地图片，并可通过 TMDB 补齐元数据
- 支持同一条目的多文件版本、季集结构和独立播放进度
- 多用户、角色权限、媒体库访问控制和会话管理
- 继续观看、最近添加、搜索、通知与网页播放
- 后台扫描、增量更新、SSE 实时同步和持久化任务状态
- 发布 `linux/amd64` 与 `linux/arm64` Docker 镜像

稳定版本和变更记录见 [GitHub Releases](https://github.com/RichesChiu/mova/releases)。

## 快速部署

需要 Docker、Docker Compose，以及宿主机上的媒体目录。将下面内容保存为 `docker-compose.yml`；媒体文件以只读方式挂载，数据库与可重建缓存保存在当前目录的 `data/` 下。

```yaml
services:
  app:
    image: richeschiu/mova:latest
    depends_on:
      database:
        condition: service_healthy
    ports:
      - "36080:36080"
    environment:
      MOVA_DATABASE_URL: "postgres://mova:postgres@database:5432/mova"
      # TMDB API Read Access Token；留空时仅使用本地元数据
      MOVA_TMDB_ACCESS_TOKEN: ""
      # 中国大陆访问 TMDB 时可选，例如 http://192.168.1.1:7890
      HTTP_PROXY: ""
      HTTPS_PROXY: ""
    volumes:
      - ./data/cache:/app/data/cache
      - type: bind
        # 替换为宿主机媒体目录的绝对路径
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

直接启动：

```bash
docker compose up -d
```

- Web：`http://127.0.0.1:36080`
- 健康检查：`http://127.0.0.1:36080/api/health`
- 日志：`docker compose logs -f app`

无需创建 `.env`。首次打开 Web 页面后，创建系统管理员，再进入服务器设置创建媒体库并选择 `/media` 下的目录。

### TMDB Token

TMDB Token 用于远端元数据、海报和背景图。不配置时 Mova 仍可启动、扫描和播放，并会跳过 TMDB 请求。

1. 注册并登录 [TMDB](https://www.themoviedb.org/)，完成邮箱验证。
2. 在账户设置的 [API 页面](https://www.themoviedb.org/settings/api)申请访问权限。
3. 复制 **API Read Access Token**，不要使用 `API Key (v3 auth)`。
4. 将它填入 `MOVA_TMDB_ACCESS_TOKEN`，然后执行 `docker compose up -d`。

Token 是敏感凭据，不要提交到仓库或公开日志。官方认证说明见 [TMDB Application Authentication](https://developer.themoviedb.org/v4/docs/authentication-application)。

### 代理

`HTTP_PROXY` 和 `HTTPS_PROXY` 主要用于中国大陆网络访问 TMDB。填写容器可访问的完整地址，例如宿主机 IP 为 `192.168.1.1`、代理端口为 `7890`：

```yaml
HTTP_PROXY: "http://192.168.1.1:7890"
HTTPS_PROXY: "http://192.168.1.1:7890"
```

不要填写 `127.0.0.1` 或 `localhost`，它们在容器中指向容器自身。代理程序必须允许来自 Docker 网络的连接。若 Docker 已能直接访问 TMDB，或 Docker Desktop 已正确使用系统代理，可以留空。Docker Hub 拉取代理需在 Docker Desktop 或 Docker Engine 中另行配置。

### 外部 PostgreSQL 与 HTTPS

使用外部 PostgreSQL 时，将 `MOVA_DATABASE_URL` 改为容器可访问的数据库地址，并删除 `depends_on` 与整个 `database` 服务。目标数据库需预先创建，连接账号需具备建表和执行迁移的权限。

通过 HTTPS 反向代理公开服务时，在 `app.environment` 中增加：

```yaml
MOVA_SESSION_COOKIE_SECURE: "true"
```

首次管理员创建完成前，只应从可信本机或受控局域网访问服务。

## 升级与数据

升级前先备份数据库，然后执行：

```bash
docker compose pull
docker compose up -d
```

Mova 会按顺序自动执行数据库迁移。媒体目录始终只读挂载，服务不会修改原始媒体文件。备份、恢复、回滚和外部数据库说明见 [部署与数据维护](docs/DEPLOYMENT.md)。

## 文档

- [HTTP API](docs/API.md)
- [SSE 同步协议](docs/SSE.md)
- [媒体库扫描与刮削](docs/MEDIA_LIBRARY_SCAN.md)
- [NFO 本地元数据](docs/NFO_METADATA.md)
- [TMDB 接入契约](docs/TMDB_INTEGRATION.md)
- [TMDB v3 API 参考](docs/TMDB.md)
- [缓存生命周期](docs/LIBRARY_CACHE_LIFECYCLE.md)
- [部署与数据维护](docs/DEPLOYMENT.md)
- [容器第三方软件](docs/THIRD_PARTY_SOFTWARE.md)

## 社区与贡献

- 官网：[mova.hk](https://mova.hk)
- Telegram：[mova_feedback](https://t.me/mova_feedback)
- 问题与建议：[GitHub Issues](https://github.com/RichesChiu/mova/issues)
- 贡献指南：[English](CONTRIBUTING.md) · [简体中文](CONTRIBUTING.zh-CN.md)
- 安全报告：[English](SECURITY.md) · [简体中文](SECURITY.zh-CN.md)

## 许可证

Mova 使用 [`AGPL-3.0-only`](LICENSE) 许可证。通过网络向用户提供修改后的 Mova 时，通常需要向这些用户提供对应源代码；具体权利和义务以仓库中的官方英文许可证原文为准。本段只是便于理解的说明，不构成法律意见。
