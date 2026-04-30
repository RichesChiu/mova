<p align="center">
  <img src="apps/mova-web/public/mova-logo-master-transparent.png" alt="Mova logo" width="96" />
</p>

<h1 align="center">Mova</h1>

<p align="center">
  A lightweight, secure, and efficient self-hosted media server for local movies and series.
</p>

<p align="center">
  English | <a href="README.zh-CN.md">Chinese</a>
</p>

![Mova home](docs/assets/readme/home.png)

## What Mova Is

Mova is a self-hosted media server for organizing, browsing, and playing local movies and series. Its server is built with Rust, a modern systems language focused on memory safety, predictable performance, and efficient resource usage.

The project aims to keep the media-server experience simple and dependable: mount a media folder, scan the library, enrich metadata when needed, and browse or play from a clean Web interface. The current release is a usable MVP for local machines, home servers, and private media setups.

## Screenshots

### Detail Page And Light Theme

![Mova detail page with light theme](docs/assets/readme/theme.png)

### Server Settings

![Mova server settings](docs/assets/readme/server-setting.png)

## Deployment

### Requirements

- Docker
- Docker Compose
- A local media folder on the host machine

### Configure

```bash
cp .env.example .env
```

Common configuration:

```env
MOVA_MEDIA_ROOT=/absolute/path/to/media
MOVA_TMDB_ACCESS_TOKEN=
MOVA_OMDB_API_KEY=
HTTP_PROXY=
HTTPS_PROXY=
```

- `MOVA_MEDIA_ROOT` is required and is mounted read-only into the container at `/media`.
- `MOVA_TMDB_ACCESS_TOKEN` is optional. Scanning, importing, and playback still work without it.
- `MOVA_OMDB_API_KEY` is optional and is used to fill IMDb ratings when an `imdb_id` is available.

### Start

```bash
docker compose up -d --build
```

Default endpoints:

- Web: `http://127.0.0.1:36080`
- Health: `http://127.0.0.1:36080/api/health`

After startup, Mova creates two runtime folders:

- `data/postgres/`: PostgreSQL database files for libraries, users, metadata, and playback progress.
- `data/cache/`: cached artwork and generated media assets.

Your media folder is mounted read-only. Mova does not modify your original media files.

### First Run

1. Open the Web app after the containers start.
2. Create the first administrator on the bootstrap page.
3. Open server settings and create a media library.
4. Select a directory under the container path `/media`.
5. Save the library and Mova will start the first scan automatically.

## Documentation

- API: [docs/API.md](docs/API.md)
- Frontend: [apps/mova-web/README.md](apps/mova-web/README.md)
- Backend: [apps/mova-server/README.md](apps/mova-server/README.md)
- Crates: [crates/README.md](crates/README.md)

## Roadmap And Feedback

Mova is still under active development. The author is also actively maintaining Pad and macOS app directions so they can connect naturally to the same self-hosted media server.

Feedback, feature requests, client integration ideas, and usability suggestions are welcome.

## License

Current license: `AGPL-3.0-only`. See [LICENSE](LICENSE).
