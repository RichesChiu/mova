<p align="center">
  <img src="apps/mova-web/public/mova-logo-master-transparent.png" alt="Mova logo" width="96" />
</p>

<h1 align="center">Mova</h1>

<p align="center">
  A self-hosted media server for local movies and series, built around automatic organization, rich metadata, and a polished watching flow.
</p>

<p align="center">
  English | <a href="README.zh-CN.md">Chinese</a>
</p>

![Mova home](docs/assets/readme/home.png)

## What Mova Is

Mova is a self-hosted media server for organizing, browsing, and playing local movies and series. It is designed to keep the common media flow lightweight: mount a folder, scan your library, enrich metadata, continue watching, and manage access from a clean Web interface.

The current release is a usable MVP for local machines, home servers, and private media setups.

## Product Highlights

- Automatic movie and series detection: a new library starts scanning immediately, and later changes can be synced with `Scan Library`.
- Local-first fallback: Mova still imports and displays media from folder and file names even when no TMDB token is configured.
- Product-like Web experience: the home page, library page, detail page, and player are designed for regular use instead of feeling like admin panels.
- On-demand enrichment: cast, artwork, IMDb rating, and intro-skip data are fetched or analyzed only when needed, then stored persistently.
- Client-ready server API: the Web app uses session login, while native clients can use the token login flow.

## Core Features

- Automatic first scan and manual library rescan.
- Movie and series grouping with local fallback metadata.
- Multiple file versions for movies.
- Season and episode lists, next episode, and continue watching.
- Playback progress persistence and near-ending completion detection.
- Subtitle switching, audio track switching, and source file technical details.
- `Skip Intro` with on-demand intro analysis when the current resource is missing intro data.
- Dark and light themes, English and Chinese interface language preferences, stored locally in the browser.
- `Primary Admin`, administrator, and member roles.
- Per-library access control for members.

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

### First Run

1. Open the Web app after the containers start.
2. Create the first administrator on the bootstrap page.
3. Open server settings and create a media library.
4. Select a directory under the container path `/media`.
5. Save the library and Mova will start the first scan automatically.

### Data

Runtime data is mainly stored in:

- `data/postgres/`
- `data/cache/`

The media folder is mounted read-only. Mova does not modify your original media files.

During development, if rebuilding local data is acceptable, reset the database folder and restart:

```bash
rm -rf data/postgres
docker compose up -d --build
```

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
