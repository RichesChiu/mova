# AGENTS

This file contains the highest-priority, stable collaboration rules for this repository. Keep it concise and do not duplicate product documentation from `README.md`, API contracts from `docs/API.md`, or implementation details owned by directory-level `AGENTS.md` files.

When instructions conflict, apply them in this order:

1. Explicit instructions from the user in the current conversation
2. The applicable `AGENTS.md` files
3. Other project documentation

The root `AGENTS.md` defines repository-wide rules. A directory-level `AGENTS.md` adds rules for files in that directory and takes precedence for that scope.

## Scope and collaboration

- The repository root is the only default write boundary. Do not create, modify, delete, stage, commit, push, or publish files in another project unless the user explicitly authorizes that project in the current request.
- Mounted or writable sibling repositories do not imply permission to modify them. For cross-client work, implement the contract and documentation here, then describe the required downstream changes.
- Read the relevant code before changing it. Do not implement from memory or stale conversation context.
- Preserve unrelated user changes in a dirty worktree. Stage only files that belong to the current task.
- User-facing copy defaults to English unless the current request specifies another language.
- Use the lowest-friction verification available. Rust is installed on the host, so run targeted `cargo check` and `cargo test` commands directly unless isolation is required.
- Report only checks, tests, builds, pushes, and releases that actually completed successfully.

## Documentation ownership

- Update relevant Markdown in the same change whenever a feature, API, behavior, runtime contract, or product direction changes.
- Keep the root `README.md` focused on product positioning, core capabilities, deployment, first use, and major product direction. Do not add routine UI or implementation details.
- Update `docs/API.md` and the relevant topic document when routes, requests, responses, fields, or API behavior change.
- Update the closest app, crate, or topic README when internal behavior, module ownership, or operating instructions change.
- Do not add release-history wording such as “previously X, now Y” to specification documents. Describe the current contract directly.

## Issues, branches, and pull requests

This repository is hosted on GitHub, so use **pull request (PR)** rather than GitLab's **merge request (MR)** in project communication.

- Create an Issue before implementation when work is externally contributed, spans multiple sessions, needs product or architecture discussion, changes a public API or database schema, or requires reproducible bug investigation. Small owner-directed fixes and routine maintenance may skip an Issue.
- Write Issue titles in English. Include context, current behavior, expected outcome or proposal, acceptance criteria, and compatibility or data impact when applicable.
- Start branches from an up-to-date `master`. Use a lowercase, kebab-case name with one of these prefixes:
  - `feat/` for product features
  - `fix/` for defects
  - `refactor/` for behavior-preserving restructuring
  - `docs/` for documentation-only changes
  - `chore/` for maintenance and release work
  - `test/` for test-only work
  - `ci/` for automation and workflow changes
- Keep each branch and PR focused on one coherent outcome. Do not mix unrelated cleanup into the same change.
- Prefer a PR into `master`, use Draft status while incomplete, and link its Issue with `Closes #...` when one exists.
- PR descriptions must summarize behavior changes, list completed verification, include screenshots for visible UI changes, identify API or schema effects, and state whether documentation was updated.
- Prefer squash merge for a normal single-purpose PR. Never force-push or rewrite `master`.
- Maintainer-only changes limited to `AGENTS.md` files may be committed directly to `master` when the repository owner authorizes them. They do not require an Issue or PR because they change collaboration policy rather than product behavior.
- Directly merging and pushing `master` is allowed only when the repository owner explicitly requests the merge or release and PR tooling is unavailable. Complete the same review and verification first, and preserve a clear merge or release commit.
- Delete merged remote branches when they are no longer needed, unless ongoing release work still references them.

## Commits and verification

- Review `git status`, the complete diff, and `git diff --check` before staging.
- Use English Conventional Commit messages, for example:
  - `feat(player): add episode navigation`
  - `fix(scan): preserve authoritative progress`
  - `refactor(realtime): batch resource invalidations`
  - `docs(api): document notification events`
  - `chore(release): publish preview image`
- Choose a specific scope such as `player`, `scan`, `settings`, `libraries`, `auth`, `api`, or `realtime`.
- Before commit, run checks proportional to the change, such as targeted Rust tests, frontend tests, TypeScript compilation, and the Vite production build.
- Do not commit generated output, temporary review artifacts, local credentials, database contents, or unrelated formatting churn.

## Build and release

- “Build and publish”, “publish the image”, and equivalent explicit requests authorize building the current repository and pushing its Docker image. A build-only request does not authorize a push.
- Image publishing does not authorize a Git commit or Git push unless the user asks for those actions too.
- Publish from the repository root with `./scripts/publish-docker-images.sh`.
- The publishing script must produce Linux `amd64` and `arm64` manifests. Windows and macOS users run the same Linux image through Docker Desktop.
- The script checks the required base-image platforms and publishes missing base images before the application image. Set `MOVA_PUBLISH_BASE_IMAGES=1` only when the base images must be republished deliberately.
- Preview releases use an immutable Docker tag such as `richeschiu/mova:1.0.0-preview.2`. After it is verified, move both `richeschiu/mova:preview` and, during the pre-1.0 phase, `richeschiu/mova:latest` to the exact same manifest.
- Preview Git tags use annotated SemVer prerelease names such as `v1.0.0-preview.2`. The annotation must summarize user-visible highlights, important fixes, verification, and any breaking or data-reset requirements.
- After publishing, run `docker buildx imagetools inspect` for the immutable tag and both aliases. Report the digest, available platforms, and whether all committed changes are included.
- If a platform or alias update fails, state it explicitly. Do not describe a partial release as complete.

## Pre-1.0 product and database policy

- The project remains in rapid pre-1.0 development. Breaking feature, API, schema, UI, configuration, and directory changes are acceptable when they produce a clearer current design.
- Remove superseded fields, routes, UI, configuration, and data structures instead of adding compatibility layers, dual paths, or fallbacks for obsolete behavior.
- Add backward compatibility only when the user explicitly requests it in the current conversation.
- Until the user declares the `1.0` database stable, keep a single migration at `migrations/0001_init.sql`. Modify that file rather than adding sequential migrations.
- Choose schema design for clarity and correct domain modeling, not compatibility with an existing development database. Update Rust queries, response models, TypeScript types, and documentation together.
- Changes to `migrations/0001_init.sql` do not update an initialized database. Before a local rebuild or restart, delete and reinitialize development database data without creating a backup.
- For every schema change, clearly state whether an existing database can migrate safely or must be rebuilt and rescanned.

## Repository map

- `apps/mova-server`: Rust HTTP service, routes, handlers, bootstrap, and runtime integration
- `apps/mova-web`: React and Vite web client
- `crates/mova-application`: application-layer business logic
- `crates/mova-db`: SQL, persistence, and synchronization
- `crates/mova-domain`: shared domain models
- `crates/mova-scan`: media discovery, parsing, probing, and sidecars
- `migrations`: database schema initialization
- `scripts`: media-analysis and release tooling

Follow all applicable directory-level instructions for cross-directory changes:

- `apps/mova-web/AGENTS.md`
- `apps/mova-server/AGENTS.md`
- `crates/AGENTS.md`
- `migrations/AGENTS.md`
- `scripts/AGENTS.md`
