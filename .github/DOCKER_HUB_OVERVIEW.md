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
    image: richeschiu/mova:preview
    depends_on:
      database:
        condition: service_healthy
    ports:
      - "36080:36080"
    environment:
      # TMDB API Read Access Token；留空时跳过远端元数据刮削
      MOVA_TMDB_ACCESS_TOKEN: ""
      # 宿主机代理地址；不使用代理时保持为空
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

媒体目录、TMDB Token 和代理都直接写在这一份 Compose 文件中，不需要创建 `.env`。

- `MOVA_TMDB_ACCESS_TOKEN`：登录 [TMDB API 设置](https://www.themoviedb.org/settings/api)申请并复制 **API Read Access Token**。留空时服务仍可启动和扫描本地媒体，但会跳过 TMDB 元数据和图片刮削。
- `HTTP_PROXY` / `HTTPS_PROXY`：填写容器可以访问的宿主机代理 IP，例如 `http://192.168.1.10:7890`；不能使用容器自身的 `127.0.0.1`。不需要代理时保持为空。
- `source`：替换为宿主机媒体目录的绝对路径；媒体目录会只读挂载到容器内 `/media`。
- 通过 HTTPS 反向代理公开 Web 页面时，在 `app.environment` 中增加 `MOVA_SESSION_COOKIE_SECURE: "true"`。

启动：

```bash
docker compose up -d
```

浏览器打开 `http://localhost:36080`，创建首个系统管理员，再进入服务器设置创建媒体库。

升级到最新预览镜像：

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
    image: richeschiu/mova:preview
    depends_on:
      database:
        condition: service_healthy
    ports:
      - "36080:36080"
    environment:
      # TMDB API Read Access Token; leave empty to skip remote metadata scraping
      MOVA_TMDB_ACCESS_TOKEN: ""
      # Proxy on the Docker host; leave empty when no proxy is needed
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

- `MOVA_TMDB_ACCESS_TOKEN`: request an **API Read Access Token** from [TMDB API settings](https://www.themoviedb.org/settings/api). Mova still starts and scans local media without it, but skips TMDB metadata and artwork.
- `HTTP_PROXY` / `HTTPS_PROXY`: use a host IP reachable from the container, such as `http://192.168.1.10:7890`, not the container's own `127.0.0.1`. Leave both values empty when no proxy is needed.
- `source`: replace it with the absolute path to the host media directory. It is mounted read-only at `/media`.
- When exposing the Web app through an HTTPS reverse proxy, add `MOVA_SESSION_COOKIE_SECURE: "true"` to `app.environment`.

Start Mova:

```bash
docker compose up -d
```

Open `http://localhost:36080`, create the initial system administrator, and then create a media library from Server Settings.

Upgrade to the newest preview image:

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
