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

## What Mova Is

Mova is a self-hosted media server for organizing, browsing, and playing local movies and series. Its server is built with Rust, a modern systems language focused on memory safety, predictable performance, and efficient resource usage.

The project aims to keep the media-server experience simple and dependable: mount a media folder, scan the library, enrich metadata when needed, and browse or play from a clean Web interface. The current release is a usable pre-1.0 MVP preview for local machines, home servers, and private media setups.

The Web home page is library-first: it shows continue watching when the active queue is non-empty, one five-column `Your Libraries` row containing at most five responsive 16:9 library cards, and the latest eight added media items grouped by every visible non-empty library, without a default time-window cutoff or a front-end merge of per-library title-sorted lists. The library `View all` link only appears when more than five libraries exist. Continue, library detail, and server-management pages use the shared empty-state panel with context-specific copy, while the empty home Continue module is hidden. Dashboard routes share a left navigation rail that stays anchored to the viewport, with the profile entry at the lower edge and a small lower-left expand handle when collapsed. The rail's `Continue` entry opens a dedicated page containing the bounded active Continue queue; a non-empty home section links to the same page through `View all`. Playback progress remains the per-file state source, while the queue keeps at most 20 unique movies or series and removes an entry when it is marked finished.

The Web interface defaults to Simplified Chinese on first initialization or when no valid language preference exists. A language explicitly selected in profile settings remains stored in the current browser.

Library editors can change only the library name, description, and TMDB metadata language; the root path remains read-only after creation. Libraries no longer have an enabled/disabled state: a newly created library always starts its initial scan, and every existing library remains available for manual scans. Changing the metadata language requires confirmation and then automatically starts a full-library metadata refresh in the selected language while reusing unchanged local probe analysis.

Server Settings keeps library cards aligned with an ellipsized single-line title, a smaller fixed two-line clamped description area, a compact scan-status marker beside the vertically centered three-dot menu, and a single-line ellipsized root path. Hovering the title, description, or root path opens an immediate pointed tooltip with its complete value; it prefers the top side and flips below when viewport space requires it. Successful scans use only the green marker and text instead of tinting the card. Edit, scan, and delete actions share the same three-dot menu used by home library cards.

Native clients authenticate with opaque short-lived access tokens plus rotating refresh tokens. Business APIs only accept the access token in `Authorization: Bearer ...`; refresh tokens are stored server-side as hashes, can be revoked per device session, and are used only through `/api/auth/refresh`.

Home and realtime synchronization are shared across Web, macOS, and iOS. `GET /api/home` returns a bounded home snapshot instead of making clients download every library catalog, while PostgreSQL-backed resource revisions provide durable change state. SSE only delivers batched invalidation hints and transient scan progress; clients recover after reconnect or foreground resume through `GET /api/realtime/state` and refresh only resources whose revisions changed. Library scans are stored as durable PostgreSQL background jobs and claimed by a bounded worker pool, so an HTTP request never owns the scan lifetime and pending work can resume after a server restart.

Login account identifiers can be regular usernames or email-form strings up to 254 characters. Mova treats them as exact account identifiers and does not verify email ownership or send email.

For UI review on machines with very small local libraries, the Web app also has an explicit development-only mock API switch. It is documented in [apps/mova-web/README.md](apps/mova-web/README.md) and is off by default, so real API errors are not hidden by mock data.

Series grouping is intentionally filename-first. Use filenames such as `Show.Name.S01E01.mkv`, `Show S01E01 - Episode 1.mkv`, `Show - S01E01.mkv`, `Show_S01E01.mkv`, or `ShowNameS01E01.mkv`; Mova does not infer series identity from arbitrary folder names. When an explicit season folder sits under a clean series folder such as `Study Group (2025)/Season 01/Study Group S01E01.mkv`, the folder year is used only as a metadata search hint. Local analysis produces a movie or series hypothesis and a title for progressive scan cards, but TMDB must confirm the corresponding remote type before the item is considered matched. When TMDB has both movie and TV candidates, Mova first validates the locally inferred type; when only the opposite type exists, Mova does not rewrite the local structure and instead stores `unmatched / remote_type_mismatch` for Other review. Movie files that resolve to the same confirmed TMDB movie are grouped into one detail page as multiple local versions, even when their local folders or punctuation differ; when a movie file name and a clean CJK parent folder disagree, the CJK folder name is only used as a fallback TMDB search candidate. No remote match, remote detection failures, type conflicts, malformed filenames, and scans completed without TMDB confirmation stay in the Other section.

After a successful scan, later scans first match by file path and compare a lightweight fingerprint based on file size and modified time. Scanning is split into four phases: discover physical files, shallow filename grouping, group-by-group local analysis, then TMDB enrichment. The shallow pass only reads filenames and paths so it can build stable movie/series groups before expensive sidecar reads or `ffprobe`; each group is then fully analyzed, written with `metadata_status = pending`, and pushed to the Web UI before the next group starts. Pending scan cards stay in the locally inferred Movies or Series section; only a completed group whose remote type remains unknown or conflicts with local structure enters Other. Local analysis stores its own version, so unchanged files skip filename parsing, sidecar reads, `ffprobe`, and aggregation only when both the fingerprint and local analysis version still match. When an unchanged item still needs TMDB because it has no TMDB provider binding, sits in Other, failed earlier, was previously skipped before TMDB was enabled, was left pending by an interrupted scan, or only has remote artwork URLs that need local caching, Mova reuses the stored local analysis and goes straight to item-by-item TMDB enrichment. Automatic matching stays conservative; broader candidate review belongs to the manual metadata search flow. Artwork fields keep their own semantics: series, season, episode, poster, and backdrop values are not substituted from another level or another image field. Already matched and unchanged items stay stable even if TMDB has no poster for them. A pending local write does not clear existing artwork; only a completed `matched` metadata write can clear artwork fields when the remote item truly has no image. Each successful TMDB result is written immediately so artwork appears progressively.

When `ffprobe` is available, Mova also stores resource-level technical tags such as 4K, 1080p, HDR10, Dolby Vision, DTS-HD, and Atmos for each physical media file, then surfaces those tags as resource badges on detail pages.

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
MOVA_WORKER_CONCURRENCY=2
HTTP_PROXY=
HTTPS_PROXY=
```

- `MOVA_MEDIA_ROOT` is required and is mounted read-only into the container at `/media`.
- `MOVA_TMDB_ACCESS_TOKEN` is optional. Scanning, importing, and playback still work without it.
- `MOVA_OMDB_API_KEY` is optional and is used to fill IMDb ratings when an `imdb_id` is available.
- `MOVA_WORKER_CONCURRENCY` controls the bounded in-process background worker pool and defaults to `2`.

### Start

```bash
docker compose up -d
```

Default endpoints:

- Web: `http://127.0.0.1:36080`
- Health: `http://127.0.0.1:36080/api/health`

After startup, Mova creates two runtime folders:

- `data/postgres/`: PostgreSQL database files for libraries, users, metadata, playback progress, durable background jobs, and realtime resource revisions.
- `data/cache/`: cached artwork and generated media assets. Deleting a library also removes its unshared TMDB artwork cache files.

During the current pre-1.0 MVP preview stage, database schema changes can require rebuilding `data/postgres/`. This realtime/background-job revision changes `migrations/0001_init.sql` directly, so an existing database is not upgraded in place: reset `data/postgres/`, initialize it again, and rescan media libraries.

Your media folder is mounted read-only. Mova does not modify your original media files.

The default Compose file runs the published `richeschiu/mova:latest` image, so the deployment machine only needs `docker compose up -d` and does not build from source. Compose will pull the image when it is missing locally; when you want to upgrade to the latest published image, run `docker compose pull` yourself before `docker compose up -d`.

For local source builds, set this in your local `.env`:

```dotenv
COMPOSE_FILE=docker-compose.yml:docker-compose.build.yml
```

Then local startup uses the same short shape:

```bash
docker compose up -d --build
```

The published image and build base images are Linux multi-architecture images for `linux/amd64` and `linux/arm64`. Windows and macOS hosts run the same Linux image through Docker Desktop, and Linux hosts run it through Docker Engine or Docker Desktop. Docker selects the matching architecture automatically. The release entrypoint is `./scripts/publish-docker-images.sh`; it checks whether the build base image tags already include the required platforms and publishes missing base images before pushing `richeschiu/mova:latest`.

The app service is named `app`, and the runtime container is fixed as `mova-app`; use `docker compose logs -f app` when following server logs.

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
