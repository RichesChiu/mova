# AGENTS

This file contains the highest-priority, stable collaboration rules for this repository. Do not duplicate product documentation, API contracts, or implementation inventories here.

When instructions conflict, apply them in this order:

1. Explicit instructions from the user in the current conversation
2. The applicable `AGENTS.md` files
3. Other project documentation

## Scope and authorization

- The repository root is the only default write boundary. Do not create, modify, delete, stage, commit, push, or publish files in another project unless the user explicitly authorizes that project in the current request.
- Mounted or writable sibling repositories do not imply permission to modify them. For cross-client work, implement the contract and documentation here, then describe the required downstream changes.
- Read the relevant code before changing it. Preserve unrelated user changes in a dirty worktree and stage only files that belong to the current task.
- Do not create an Issue or branch, stage or commit files, push, open or merge a Pull Request, create a tag, or publish a release unless the user explicitly requests that workflow.
- `CONTRIBUTING.md` is the single source of truth for Issues, branches, commits, verification, Pull Requests, and merge policy.
- Route user-facing copy through the applicable localization catalog and keep every supported
  language synchronized.
- Report only checks, tests, builds, pushes, and releases that actually completed successfully.

## Architecture boundaries

- `apps/mova-server` owns HTTP/SSE transport, authentication integration, routing, process bootstrap, and runtime wiring. Keep business rules and SQL out of handlers.
- `crates/mova-application` owns business use cases and orchestration.
- `crates/mova-db` owns SQL, transactions, persistence, and database-to-domain mapping.
- `crates/mova-domain` owns shared domain models and IO-free domain helpers.
- `crates/mova-scan` owns media discovery, filename parsing, sidecar reading, probing, and subtitle/audio-track discovery.
- `apps/mova-web` and `apps/mova-site` consume public contracts; they must not become an alternative source of server business rules.

## Documentation

- Keep `README.md` focused on product positioning, core capabilities, deployment, first use, and major product direction. Do not add routine UI or implementation details.
- Update `docs/API.md` and the relevant topic document when routes, requests, responses, fields, errors, or API behavior change.
- Every `docs/API.md` change must update the corresponding public API content in `apps/mova-site` in the same change. After the change reaches `master`, confirm the `Deploy Site` GitHub Action completed; dispatch it manually when the path-based trigger did not run.
- Update an app or crate README only when its ownership boundary, entry point, or operating instructions change. Put detailed runtime behavior in the relevant topic document instead of duplicating it across READMEs.
- Specification documents describe the current contract. Do not add release-history wording such as “previously X, now Y”.

## Build and release

- A build-only request does not authorize a push. Image publishing does not authorize a Git commit or Git push unless the user asks for those actions too.
- Publish from the repository root with `./scripts/publish-docker-images.sh`; releases must produce
  Linux `amd64` and `arm64` manifests.
- Release immutable, annotated SemVer Git and image tags first. After verification, move mutable
  channel aliases such as `preview` or `latest` only to the intended manifest. Tag annotations
  summarize user-visible changes, verification, and any migration requirements.
- Inspect the immutable image and aliases after publishing. Report their digest and platforms, and never describe a partial release as complete.

## Database and contract stability

- `migrations/0001_init.sql` is the frozen `1.0` schema baseline. Do not rewrite it after the baseline change is merged; every later schema change must use the next sequential migration.
- Migrations must upgrade an initialized database in place. Do not require a destructive reset, discard user data, or add compatibility-only shadow fields unless the user explicitly authorizes that design.
- Update affected Rust queries, response models, TypeScript types, tests, and documentation in the same schema change. State whether the migration requires a media rescan or cache rebuild.
- HTTP API contract version `1` permits additive endpoints, fields, and error codes. Removing or changing existing contract semantics requires an explicit versioning decision and synchronized updates to `docs/API.md`, `apps/mova-site`, and affected clients.
- SSE evolves independently through `protocol_version`; a breaking event or recovery-semantics change requires a protocol version increase.
