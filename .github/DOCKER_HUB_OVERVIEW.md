# Mova

**语言 / Language：中文 · [English](#english)**

## Docker Compose 部署

创建部署目录：

```bash
mkdir -p mova
cd mova
```

保存以下内容为 `docker-compose.yml`：

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
      # TMDB API Read Access Token；留空时跳过远端元数据刮削
      MOVA_TMDB_ACCESS_TOKEN: ""
      # 中国大陆访问 TMDB 时可选；例如 http://192.168.1.1:7890
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

媒体目录、TMDB Token 和代理都直接写在这一份 Compose 文件中，不需要创建 `.env`。

- `MOVA_DATABASE_URL`：连接 Compose 内部的 PostgreSQL，密码必须与 `database.POSTGRES_PASSWORD` 一致。数据库不向宿主机发布端口。
- 外部 PostgreSQL：将 `MOVA_DATABASE_URL` 改为容器可访问的实际地址，同时删除 `app.depends_on.database` 和整个 `database` 服务。外部数据库需提前创建，连接账号需要拥有建表与执行迁移的权限。
- `MOVA_TMDB_ACCESS_TOKEN`：登录 [TMDB API 设置](https://www.themoviedb.org/settings/api)申请并复制 **API Read Access Token**。留空时服务仍可启动和扫描本地媒体，但会跳过 TMDB 元数据和图片刮削。
- `HTTP_PROXY` / `HTTPS_PROXY`：填写容器可以访问的宿主机代理 IP，例如 `http://192.168.1.1:7890`；不能使用容器自身的 `127.0.0.1`。容器可以直接访问 TMDB 时保持为空。
- `source`：替换为宿主机媒体目录的绝对路径；媒体目录会只读挂载到容器内 `/media`。
- 通过 HTTPS 反向代理公开 Web 页面时，在 `app.environment` 中增加 `MOVA_SESSION_COOKIE_SECURE: "true"`。

启动：

```bash
docker compose up -d
```

`36080` 是 Web/API 端口；PostgreSQL 只存在于 Compose 内部网络。创建首个系统管理员前，只能从可信本机或受控局域网访问，不要把未初始化的服务暴露到公网。完成初始化后，再配置 HTTPS 反向代理并设置 `MOVA_SESSION_COOKIE_SECURE: "true"`。

`latest` 指向当前稳定版本，`preview` 仅用于后续预发布验证。从任何 Preview 版本首次升级到 1.0 都需要最后一次重建数据库并重新扫描媒体库；1.0 之后使用顺序迁移原地升级。完整的升级、备份、恢复和回滚步骤见 [部署与数据维护](https://github.com/RichesChiu/mova/blob/master/docs/DEPLOYMENT.md)。

升级到当前发布镜像前先备份数据库，然后执行：

```bash
docker compose pull
docker compose up -d
```

镜像支持 `linux/amd64` 和 `linux/arm64`。Windows 和 macOS 可通过 Docker Desktop 运行同一个 Linux 镜像。

## 链接

- [Mova 官网](https://mova.hk)
- [Telegram 交流群](https://t.me/mova_feedback)
- [GitHub 源码](https://github.com/RichesChiu/mova)
- [问题反馈](https://github.com/RichesChiu/mova/issues)
- [API 与技术文档](https://github.com/RichesChiu/mova/tree/master/docs)

---

## English

[返回中文](#mova)

### Docker Compose

Create a deployment directory:

```bash
mkdir -p mova
cd mova
```

Save the following as `docker-compose.yml`:

```yaml
services:
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
      # Internal Compose database connection; keep the password in sync with database.POSTGRES_PASSWORD
      MOVA_DATABASE_URL: "postgres://mova:postgres@database:5432/mova"
      # TMDB API Read Access Token; leave empty to skip remote metadata scraping
      MOVA_TMDB_ACCESS_TOKEN: ""
      # Optional proxy for reaching TMDB; for example http://192.168.1.1:7890
      HTTP_PROXY: ""
      HTTPS_PROXY: ""
    volumes:
      - ./data/cache:/app/data/cache
      - type: bind
        # Replace with the absolute path to the host media directory
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
    restart: unless-stopped
```

Configure the media directory, TMDB token, and proxy directly in this Compose file. No `.env` file is required.

- `MOVA_DATABASE_URL`: connects to PostgreSQL inside the Compose network. Its password must match `database.POSTGRES_PASSWORD`. No database port is published to the host.
- External PostgreSQL: replace `MOVA_DATABASE_URL` with an address reachable from the container, then remove `app.depends_on.database` and the entire `database` service. Create the database first and grant the account permission to create tables and run migrations.
- `MOVA_TMDB_ACCESS_TOKEN`: request an **API Read Access Token** from [TMDB API settings](https://www.themoviedb.org/settings/api). Mova still starts and scans local media without it, but skips TMDB metadata and artwork.
- `HTTP_PROXY` / `HTTPS_PROXY`: use a host IP reachable from the container, such as `http://192.168.1.1:7890`, not the container's own `127.0.0.1`. Leave both values empty when the container can reach TMDB directly.
- `source`: replace it with the absolute path to the host media directory. It is mounted read-only at `/media`.
- When exposing the Web app through an HTTPS reverse proxy, add `MOVA_SESSION_COOKIE_SECURE: "true"` to `app.environment`.

Start Mova:

```bash
docker compose up -d
```

Port `36080` serves Web/API traffic; PostgreSQL remains inside the Compose network. Before creating the initial system administrator, access Mova only from a trusted local machine or controlled LAN and do not expose the uninitialized service to the Internet. After initialization, configure an HTTPS reverse proxy and set `MOVA_SESSION_COOKIE_SECURE: "true"`.

`latest` points to the current stable release, while `preview` is reserved for future pre-release validation. Moving from any Preview release to 1.0 requires one final database rebuild and library rescan; later 1.x releases upgrade in place through sequential migrations. See [Deployment and data maintenance](https://github.com/RichesChiu/mova/blob/master/docs/DEPLOYMENT.md) for upgrade, backup, restore, and rollback procedures.

Back up the database, then upgrade to the current release image:

```bash
docker compose pull
docker compose up -d
```

The image supports `linux/amd64` and `linux/arm64`. Windows and macOS can run the same Linux image through Docker Desktop.

### Links

- [Mova Website](https://mova.hk)
- [Telegram Community](https://t.me/mova_feedback)
- [GitHub Repository](https://github.com/RichesChiu/mova)
- [Issue Tracker](https://github.com/RichesChiu/mova/issues)
- [API and Technical Documentation](https://github.com/RichesChiu/mova/tree/master/docs)
