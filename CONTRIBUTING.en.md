# Contributing to Mova

English · [简体中文](CONTRIBUTING.md)

Thank you for improving Mova. Keep each contribution focused, reviewable, and safe to upgrade.

## Before implementation

Search existing [Issues](https://github.com/RichesChiu/mova/issues) and Pull Requests first.

Open an Issue before coding when a change affects product behavior, public APIs, database schemas, deployment contracts, media-scanning rules, or needs architectural discussion. Reproducible bugs and work spanning multiple sessions or contributors should also start with an Issue. Small documentation, test, and maintenance changes may go directly to a Pull Request.

Never disclose a suspected vulnerability publicly. Follow [SECURITY.en.md](SECURITY.en.md).

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

Single-purpose Pull Requests are normally squash-merged into `master`. The repository automatically
deletes merged branches that live in this repository; fork contributors delete their own branch
after merge.

## Releases

Ordinary feature, fix, documentation, and maintenance Pull Requests never publish a release. A
maintainer starts a release from the latest `master` on `chore/release-X.Y.Z`, updates the workspace
version in `Cargo.toml` and `Cargo.lock`, and opens a Pull Request titled exactly:

```text
chore(release): prepare X.Y.Z
```

A release Pull Request containing only the workspace version, matching lockfile versions, and an
optional same-version release note uses a strict reduced validation path; any other change falls
back to the full CI suite. After that Pull Request passes and is merged, the `master` CI verifies that
the merge commit exactly matches the tested Pull Request tree, source, and check results. It also
falls back to the full CI suite whenever that proof is unavailable. After this gate passes, the
`Release` workflow:

1. creates an annotated `vX.Y.Z` tag from the verified commit;
2. builds, smoke-tests, and security-checks Linux `amd64` and `arm64` images;
3. publishes the immutable `richeschiu/mova:X.Y.Z` image;
4. moves `latest` for a stable version or `preview` for a SemVer prerelease; and
5. publishes the matching GitHub Release with generated notes.

When a release needs a prominent warning, experimental boundary, or migration note, add
`.github/release-notes/X.Y.Z[-prerelease].md` in the feature Pull Request. The Release workflow
includes that content in the annotated Git tag and prepends it to the generated GitHub Release notes.

Promoted `publish-*` candidate tags are deleted immediately after verification. Failed build or
verification candidates remain available for diagnostics for at most 72 hours, then a daily cleanup
job removes them. Cleanup deletes candidate tag names only; it never deletes shared image content,
immutable version tags, `latest`, or `preview`.

The workflow stops when the version tag belongs to another commit. A failed run for the same tagged
commit can be retried safely from GitHub Actions and continues the incomplete release; do not recreate
or move release tags manually. Repository maintainers must configure the `DOCKERHUB_USERNAME`
Actions variable and a `DOCKERHUB_TOKEN` Actions secret with read, write, and delete permissions.
