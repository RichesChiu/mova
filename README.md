<p align="center">
  <img src="apps/mova-web/public/mova-logo-master-transparent.png" alt="Mova 标志" width="96" />
</p>

<h1 align="center">Mova</h1>

<p align="center">
  面向本地电影和剧集的轻量、安全、高效自托管媒体服务器。
</p>

## Mova 是什么

Mova 是一个用于整理、浏览和播放本地电影与剧集的自托管媒体服务器。服务端使用 Rust 构建，这是一门强调内存安全、稳定性能和资源效率的现代系统语言。

项目希望把媒体服务器体验保持得足够简单可靠：挂载媒体目录，扫描媒体库，按需补齐元数据，然后通过 Web、macOS 和 iOS 客户端浏览与播放。当前公开版本为 1.0 稳定版，适合部署在本机、家用服务器和私人媒体库环境。

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
      # 中国大陆访问 TMDB 时可选；填写宿主机实际 IP 和代理端口
      # 例如宿主机为 192.168.1.1、HTTP 代理端口为 7890：
      # http://192.168.1.1:7890
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
    restart: unless-stopped
```

媒体目录、TMDB Token 和代理地址都直接在这一份 `docker-compose.yml` 中配置。无需创建 `.env`，也无需合并额外配置片段。

#### 代理填写规则

`HTTP_PROXY` 和 `HTTPS_PROXY` 主要用于中国大陆网络访问 TMDB 元数据与图片。代理值必须是容器能够访问的完整 URL，格式为 `协议://宿主机实际地址:代理端口`。例如宿主机的局域网地址是 `192.168.1.1`，HTTP 代理端口是 `7890`，应填写：

```yaml
HTTP_PROXY: "http://192.168.1.1:7890"
HTTPS_PROXY: "http://192.168.1.1:7890"
```

请根据自己的宿主机地址和代理端口替换示例值，并确保代理程序允许来自 Docker 网络或局域网的连接。不要填写 `127.0.0.1` 或 `localhost`，它们在容器内指向 MOVA 容器自身，而不是宿主机。

如果 MOVA 容器已经可以直接访问 `api.themoviedb.org` 和 `image.tmdb.org`，例如宿主机、路由器或网络环境使用了对 Docker 生效的透明代理/TUN，则两个变量可以保持为空。仅在宿主机启动普通代理程序不会自动让容器继承代理；这种情况下仍需按上面的格式填写实际地址和端口。

`36080` 是 Mova Web 服务端口。PostgreSQL 不向宿主机发布端口，只能由同一 Compose 项目内的应用容器访问。示例中的数据库密码仅用于这个隔离的内部网络；如需修改，请同时更新 `MOVA_DATABASE_URL` 与 `POSTGRES_PASSWORD`。

如需使用已有的外部 PostgreSQL，将 `MOVA_DATABASE_URL` 改为容器可访问的数据库地址，同时删除 `app.depends_on.database` 和整个 `database` 服务。数据库需提前创建，连接账号需要拥有建表与执行迁移的权限；容器中的 `localhost` 指向 MOVA 容器自身，应填写数据库的实际 IP 或域名。Mova 启动时会自动执行数据库迁移。

这些代理变量只控制 MOVA 运行时对 TMDB 等外部服务的请求；如果 `docker compose pull` 无法访问 Docker Hub，需要在 Docker Desktop 或 Docker Engine 中单独配置代理。

如果通过 HTTPS 反向代理公开 Web 页面，请在 `app.environment` 中额外设置 `MOVA_SESSION_COOKIE_SECURE: "true"`，让浏览器只通过 HTTPS 发送登录 Cookie。本地纯 HTTP 部署保持默认值即可，否则浏览器不会回传 Cookie。

### 获取 TMDB Access Token

1. 注册并登录 [TMDB](https://www.themoviedb.org/)，完成邮箱验证。
2. 打开账户设置中的 [API 页面](https://www.themoviedb.org/settings/api)，按页面要求申请 API 访问权限并接受 TMDB 条款。
3. 申请通过后，在同一页面复制 **API Read Access Token**。Mova 使用的是这段较长的 Bearer Token，不是 `API Key (v3 auth)`。
4. 将 Token 填入 `docker-compose.yml` 的应用环境变量：

```yaml
MOVA_TMDB_ACCESS_TOKEN: "你的_API_Read_Access_Token"
```

Token 属于敏感凭据，不要提交到 Git 仓库或放进公开日志。TMDB 的官方认证说明见 [Application Authentication](https://developer.themoviedb.org/v4/docs/authentication-application)。

如果暂时不配置 Token，扫描条目会以 `skipped / metadata_provider_disabled` 完成本地入库，不会被记为刮削失败。后续在 `docker-compose.yml` 中补上 Token 并执行 `docker compose up -d` 重启服务，再重新扫描媒体库，即可只对需要远端元数据的条目补做 TMDB 刮削，无需重建数据库。

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

从任何 Preview 版本首次升级到 1.0 都需要执行最后一次数据库重建并重新扫描媒体库；原始媒体文件不会被修改。完成 1.0 初始化后，后续版本均通过顺序迁移原地升级。升级、备份、恢复和回滚步骤见 [部署与数据维护](docs/DEPLOYMENT.md)。

媒体目录只读挂载，Mova 不会修改你的原始媒体文件。

默认 Compose 文件直接运行 `richeschiu/mova:latest`，不在部署机器上从源码构建。`latest` 指向当前稳定版本，`preview` 仅用于后续预发布验证。主动升级时先备份数据库，再执行 `docker compose pull` 和 `docker compose up -d`。

已发布镜像覆盖 `linux/amd64` 和 `linux/arm64`。Windows 和 macOS 宿主机通过 Docker Desktop 运行同一个 Linux 镜像，Linux 宿主机通过 Docker Engine 或 Docker Desktop 运行，Docker 会自动选择匹配的架构。

应用服务名是 `app`；查看服务日志时使用 `docker compose logs -f app`。

### 首次使用

首次系统管理员创建完成前，只应从可信本机或受控局域网访问 `36080`，不要将未初始化的服务直接暴露到公网。初始化完成后，再配置 HTTPS 反向代理；公网部署同时启用 `MOVA_SESSION_COOKIE_SECURE: "true"`。

1. 容器启动后打开 Web 页面。
2. 在初始化页面创建第一个管理员。
3. 进入服务器设置并创建媒体库。
4. 选择容器内 `/media` 下的目录。
5. 保存媒体库后，Mova 会自动开始第一次扫描。

## 文档

- API: [docs/API.md](docs/API.md)
- SSE 同步协议: [docs/SSE.md](docs/SSE.md)
- 媒体库扫描与刮削设计: [docs/MEDIA_LIBRARY_SCAN.md](docs/MEDIA_LIBRARY_SCAN.md)
- TMDB 服务端接入契约: [docs/TMDB_INTEGRATION.md](docs/TMDB_INTEGRATION.md)
- TMDB v3 API 参考: [docs/TMDB.md](docs/TMDB.md)
- 媒体库缓存生命周期: [docs/LIBRARY_CACHE_LIFECYCLE.md](docs/LIBRARY_CACHE_LIFECYCLE.md)
- 部署、升级、备份与恢复: [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)
- 容器第三方软件与对应源码: [docs/THIRD_PARTY_SOFTWARE.md](docs/THIRD_PARTY_SOFTWARE.md)
- 前端: [apps/mova-web/README.md](apps/mova-web/README.md)
- 官方网站: [apps/mova-site/README.md](apps/mova-site/README.md)
- 后端: [apps/mova-server/README.md](apps/mova-server/README.md)
- Crates: [crates/README.md](crates/README.md)

## 路线图与反馈

Mova 仍在积极迭代中。作者也在积极维护 Pad 和 macOS 客户端方向，让它们可以更自然地接入同一个自托管媒体服务器。

欢迎提交反馈、功能建议、客户端接入想法和体验改进意见。

## 参与贡献

欢迎通过 Issue 反馈问题或提出功能建议，也欢迎提交 Pull Request。开始前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)，其中说明了 Issue 适用场景、分支命名、Conventional Commits、验证要求和 PR 流程。

## 许可证

当前许可证：`AGPL-3.0-only`。详见 [LICENSE](LICENSE)。
