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

Series grouping is intentionally filename-first. Use filenames such as `Show.Name.S01E01.mkv`, `Show S01E01 - Episode 1.mkv`, `Show - S01E01.mkv`, `Show_S01E01.mkv`, or `ShowNameS01E01.mkv`; Mova does not infer series identity from arbitrary folder names. When an explicit season folder sits under a clean series folder such as `Study Group (2025)/Season 01/Study Group S01E01.mkv`, the folder year is used only as a metadata search hint. Before TMDB enrichment succeeds, cards use the locally analyzed movie or series title; once TMDB succeeds, the TMDB title replaces the local title. Files without season/episode identity are checked against TMDB movie and TV results; TV matches without local season/episode identity, failed matches, and malformed filenames are stored with explicit metadata review status and stay in the Other section. If TMDB is disabled, metadata is marked as skipped and local movie/series detection still remains visible.

After a successful scan, later scans first match by file path and compare a lightweight fingerprint based on file size and modified time. Scanning is split into four phases: discover physical files, shallow filename grouping, group-by-group local analysis, then TMDB enrichment. The shallow pass only reads filenames and paths so it can build stable movie/series groups before expensive sidecar reads or `ffprobe`; each group is then fully analyzed, written, and pushed to the Web UI before the next group starts. Local analysis stores its own version, so unchanged files skip filename parsing, sidecar reads, `ffprobe`, and aggregation only when both the fingerprint and local analysis version still match. When an unchanged item still needs TMDB because it was never bound, still lacks a visible poster, or only has remote artwork URLs, Mova reuses the stored local analysis and goes straight to item-by-item TMDB enrichment. Local placeholder items are written group by group, then each successful TMDB result is written immediately so artwork appears progressively.

When `ffprobe` is available, Mova also stores resource-level technical tags such as HDR10, Dolby Vision, DTS-HD, and Atmos for each physical media file, then surfaces those tags as resource badges on detail pages.

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
docker compose up -d
```

Default endpoints:

- Web: `http://127.0.0.1:36080`
- Health: `http://127.0.0.1:36080/api/health`

After startup, Mova creates two runtime folders:

- `data/postgres/`: PostgreSQL database files for libraries, users, metadata, and playback progress.
- `data/cache/`: cached artwork and generated media assets.

During the current pre-MVP development stage, database schema changes can require rebuilding `data/postgres/`. The current scan pipeline schema stores local analysis versioning in `migrations/0001_init.sql`, so existing development databases should be rebuilt after pulling this change.

Your media folder is mounted read-only. Mova does not modify your original media files.

The default Compose file runs the published `richeschiu/mova:latest` image, so the deployment machine only needs `docker compose up -d` and does not build from source. Compose will pull the image when it is missing locally; when you want to upgrade to the latest published image, run `docker compose pull` yourself before `docker compose up -d`. For local development builds, run:

```bash
docker compose -f docker-compose.yml -f docker-compose.build.yml up -d --build
```

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
