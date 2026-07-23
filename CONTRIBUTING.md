# Contributing to Mova

Thank you for helping improve Mova. This guide keeps contributions focused, reviewable, and safe to merge while the project is moving quickly toward `1.0`.

## Before you start

Search existing Issues and Pull Requests before opening a new one.

Open an Issue before implementation when a change:

- adds or substantially changes product behavior;
- changes a public API, database schema, deployment contract, or media-scanning rule;
- needs product or architecture discussion;
- fixes a bug that needs a reproducible investigation; or
- is likely to span multiple sessions or contributors.

Small documentation fixes, tests, and narrowly scoped maintenance may go directly to a Pull Request. If you are unsure, open an Issue first.

Do not disclose security vulnerabilities in a public Issue. Contact the maintainer privately instead.

## Branches

External contributors should work from a fork. Maintainers may create branches in the main repository. In both cases, branch from the latest `master` and use lowercase kebab case:

```text
feat/continue-watching-filter
fix/scan-progress-regression
refactor/realtime-dispatcher
docs/docker-deployment
test/player-shortcuts
ci/pull-request-checks
chore/dependency-refresh
```

Keep one coherent outcome per branch. Do not mix unrelated refactors, formatting, or cleanup into a feature or fix.

## Commits

Use English [Conventional Commits](https://www.conventionalcommits.org/) with a specific scope:

```text
feat(player): add episode navigation
fix(scan): preserve authoritative progress
refactor(realtime): batch resource invalidations
docs(api): document notification events
test(player): cover autoplay recovery
chore(deps): update frontend tooling
```

Common types are `feat`, `fix`, `refactor`, `docs`, `test`, `ci`, and `chore`.

- Write the subject in the imperative mood and keep it concise.
- Explain motivation and non-obvious tradeoffs in the commit body when needed.
- Mark breaking changes with a `BREAKING CHANGE:` footer.
- Keep commits buildable when practical. Maintainers normally squash a single-purpose Pull Request when merging, but readable commits make review easier.

## Development and verification

Run checks proportional to the change and report only commands that completed successfully.

For frontend changes, run:

```bash
pnpm -C apps/mova-web test
pnpm -C apps/mova-web check
pnpm -C apps/mova-web build
```

For official website changes, run:

```bash
npm --prefix apps/mova-site run check:api-docs
npm --prefix apps/mova-site run lint
npm --prefix apps/mova-site run typecheck
npm --prefix apps/mova-site run build
```

For Rust changes, run targeted commands for the affected package, for example:

```bash
cargo check -p mova-server
cargo test -p mova-scan
```

Add or update tests for behavior changes. Visible UI changes should include before/after screenshots or a short recording in the Pull Request.

## Documentation and API changes

- Update relevant Markdown in the same Pull Request as the behavior change.
- Update `docs/API.md` and the appropriate topic document for route, request, response, field, error, or API behavior changes.
- Keep the root `README.md` focused on product positioning, deployment, first use, and major product direction.
- Never commit credentials, TMDB tokens, local database files, media, caches, generated build output, or private logs.

## Pre-1.0 database changes

Until the project declares the `1.0` schema stable, edit `migrations/0001_init.sql` directly instead of adding sequential migrations.

A schema Pull Request must update all affected Rust queries, response models, TypeScript types, tests, and documentation. It must also state clearly that existing development databases need to be rebuilt and media libraries rescanned unless the change is proven not to require it.

## Pull Requests

Pull Request titles must also use the Conventional Commit format because the title becomes the squash commit message.

Before requesting review:

- link the related Issue with `Closes #123` when one exists;
- describe the user-visible outcome and implementation boundary;
- list the exact verification commands that passed;
- attach UI evidence for visible changes;
- state API, database, deployment, and documentation impact;
- remove unrelated changes and temporary files; and
- convert the Pull Request from Draft only when it is ready to merge.

Maintainers may request changes, split an oversized Pull Request, or close work that conflicts with the current product direction. Approved single-purpose Pull Requests are normally squash-merged into `master`, and the merged branch is then deleted.
