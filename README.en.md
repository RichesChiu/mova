<p align="center">
  <img src="apps/mova-web/public/mova-logo-master-transparent.png" alt="Mova logo" width="96" />
</p>

<h1 align="center">Mova</h1>

<p align="center">
  A lightweight, secure, and efficient self-hosted media server for local movies and series.
</p>

<p align="center">
  <a href="README.md">简体中文</a> · English
</p>

## What is Mova?

Mova is a Rust-based server for organizing, browsing, and playing local movies and series. It brings media libraries, metadata, user access, playback progress, and cross-device synchronization into one self-hosted service. Mova includes a Web interface and APIs for clients such as macOS and iOS apps.

Highlights:

- Scan movies and series, read NFO files and local artwork, and optionally enrich metadata through TMDB
- Support multiple files for one title, season and episode structures, and per-user playback progress
- Manage users, roles, library access, and sessions
- Continue watching, recently added items, search, notifications, and Web playback
- Run background scans, incremental updates, SSE synchronization, and persistent jobs
- Publish Docker images for `linux/amd64` and `linux/arm64`

See [GitHub Releases](https://github.com/RichesChiu/mova/releases) for stable versions and release notes.

## Quick deployment

You need Docker, Docker Compose, and a media directory on the host. Save the following as `docker-compose.yml`. Media is mounted read-only; PostgreSQL data and rebuildable caches are stored under `data/` beside the Compose file.

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
      # TMDB API Read Access Token; leave empty to use local metadata only
      MOVA_TMDB_ACCESS_TOKEN: ""
      # Optional for reaching TMDB, for example http://192.168.1.1:7890
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

Start the stack:

```bash
docker compose up -d
```

- Web: `http://127.0.0.1:36080`
- Health check: `http://127.0.0.1:36080/api/health`
- Logs: `docker compose logs -f app`

No `.env` file is required. On first launch, create the system administrator, open Server Settings, create a library, and select a directory under `/media`.

### TMDB token

The TMDB token enables remote metadata, posters, and backdrops. Without it, Mova still starts, scans, and plays local media while skipping TMDB requests.

1. Register and verify an account at [TMDB](https://www.themoviedb.org/).
2. Request API access from the account [API settings](https://www.themoviedb.org/settings/api).
3. Copy the **API Read Access Token**, not the `API Key (v3 auth)`.
4. Set `MOVA_TMDB_ACCESS_TOKEN` and run `docker compose up -d`.

Treat the token as a secret. Do not commit it or include it in public logs. See [TMDB Application Authentication](https://developer.themoviedb.org/v4/docs/authentication-application) for the official authentication guide.

### Proxy

`HTTP_PROXY` and `HTTPS_PROXY` can help the container reach TMDB from restricted networks. Use a complete URL reachable from the container. For a host at `192.168.1.1` with a proxy on port `7890`:

```yaml
HTTP_PROXY: "http://192.168.1.1:7890"
HTTPS_PROXY: "http://192.168.1.1:7890"
```

Do not use `127.0.0.1` or `localhost`; inside the container they refer to the container itself. The proxy must accept connections from the Docker network. Leave both values empty when Docker can already reach TMDB directly or Docker Desktop is correctly using the system proxy. Configure Docker Desktop or Docker Engine separately if pulling images from Docker Hub requires a proxy.

### External PostgreSQL and HTTPS

To use an external PostgreSQL server, point `MOVA_DATABASE_URL` at an address reachable from the container, then remove `depends_on` and the entire `database` service. Create the database first and grant the account permission to create tables and run migrations.

When exposing Mova through an HTTPS reverse proxy, add the following to `app.environment`:

```yaml
MOVA_SESSION_COOKIE_SECURE: "true"
```

Until the first administrator has been created, expose Mova only to a trusted host or private network.

## Upgrades and data

Back up the database before upgrading, then run:

```bash
docker compose pull
docker compose up -d
```

Mova applies database migrations in order at startup. The media mount remains read-only, and Mova does not modify source media files. See [Deployment and data maintenance](docs/DEPLOYMENT.md) for backup, restore, rollback, and external database guidance.

## Documentation

- [HTTP API](docs/API.md)
- [SSE synchronization](docs/SSE.md)
- [Media library scanning and metadata](docs/MEDIA_LIBRARY_SCAN.md)
- [Local NFO metadata](docs/NFO_METADATA.md)
- [TMDB integration contract](docs/TMDB_INTEGRATION.md)
- [TMDB v3 API reference](docs/TMDB.md)
- [Cache lifecycle](docs/LIBRARY_CACHE_LIFECYCLE.md)
- [Deployment and data maintenance](docs/DEPLOYMENT.md)
- [Third-party container software](docs/THIRD_PARTY_SOFTWARE.md)

## Community and contributing

- Website: [mova.hk](https://mova.hk)
- Telegram: [mova_feedback](https://t.me/mova_feedback)
- Bugs and ideas: [GitHub Issues](https://github.com/RichesChiu/mova/issues)
- Contributing: [English](CONTRIBUTING.md) · [简体中文](CONTRIBUTING.zh-CN.md)
- Security: [English](SECURITY.md) · [简体中文](SECURITY.zh-CN.md)

## License

Mova is licensed under [`AGPL-3.0-only`](LICENSE). If you provide a modified version of Mova to users over a network, you will generally need to offer those users the corresponding source code. The canonical English license text in this repository defines the actual rights and obligations; this summary is not legal advice.
