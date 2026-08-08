# Contributing to Mova

English · [简体中文](CONTRIBUTING.zh-CN.md)

Thank you for improving Mova. Keep each contribution focused, reviewable, and safe to upgrade.

## Before implementation

Search existing [Issues](https://github.com/RichesChiu/mova/issues) and Pull Requests first.

Open an Issue before coding when a change affects product behavior, public APIs, database schemas, deployment contracts, media-scanning rules, or needs architectural discussion. Reproducible bugs and work spanning multiple sessions or contributors should also start with an Issue. Small documentation, test, and maintenance changes may go directly to a Pull Request.

Never disclose a suspected vulnerability publicly. Follow [SECURITY.md](SECURITY.md).

## Branches and commits

External contributors should work from a fork. Start from the latest `master`, keep one outcome per branch, and use lowercase kebab case:

```text
feat/continue-watching-filter
fix/scan-progress-regression
refactor/realtime-dispatcher
docs/docker-deployment
test/player-shortcuts
ci/pull-request-checks
chore/dependency-refresh
```

Use English [Conventional Commits](https://www.conventionalcommits.org/) with a specific scope:

```text
feat(player): add episode navigation
fix(scan): preserve authoritative progress
docs(api): document notification events
```

Keep the subject concise and imperative. Explain non-obvious decisions in the body and mark breaking changes with a `BREAKING CHANGE:` footer. Do not mix unrelated refactors or formatting into a feature or fix.

## Local development

The root `docker-compose.yml` runs the published image. To run the current source checkout:

```bash
cp .env.example .env
# Set MOVA_MEDIA_PATH and optional TMDB or proxy values in .env.
docker compose -f compose.source.yaml up -d --build
```

The source stack listens on `http://127.0.0.1:36080`, stores PostgreSQL data and rebuildable caches under `data/`, and mounts media read-only.

```bash
docker compose -f compose.source.yaml logs -f app
docker compose -f compose.source.yaml down
```

Do not combine the deployment and source Compose files. Never commit credentials, local databases, media, caches, generated output, or private logs.

## Verification

Run checks proportional to the change and add tests for behavior changes.

```bash
# Web
pnpm -C apps/mova-web test
pnpm -C apps/mova-web check
pnpm -C apps/mova-web build

# Website
npm --prefix apps/mova-site run check:api-docs
npm --prefix apps/mova-site run lint
npm --prefix apps/mova-site run typecheck
npm --prefix apps/mova-site run build

# Rust examples
cargo check -p mova-server
cargo test -p mova-scan
```

Visible UI changes should include before/after screenshots or a short recording.

## Contracts and migrations

- Update relevant Markdown with behavior changes.
- Update `docs/API.md` and the matching topic document when routes, requests, responses, fields, errors, or semantics change.
- Keep the official website API content synchronized with `docs/API.md` and run `check:api-docs`.
- Keep `README.md` focused on the product, deployment, first use, and major direction.
- Do not edit an applied migration. Add the next sequential migration and make it upgrade initialized databases in place.
- Schema changes must update affected Rust queries, response models, TypeScript types, tests, and documentation. State whether a rescan or cache rebuild is required.
- HTTP contract version `1` permits additive endpoints, optional fields, and error codes. Removing or changing existing semantics requires an explicit versioning proposal. Breaking SSE changes require a `protocol_version` increase.

## Pull Requests

Use a Conventional Commit title because it becomes the squash commit message. A ready Pull Request should:

- link the Issue with `Closes #123` when one exists;
- explain the outcome, scope, and important tradeoffs;
- list the exact checks that passed;
- include UI evidence when applicable;
- state API, database, deployment, and documentation impact; and
- contain no unrelated or temporary files.

Single-purpose Pull Requests are normally squash-merged into `master`; delete the merged branch afterward.
